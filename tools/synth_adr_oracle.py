#!/usr/bin/env python3
"""Drives the ORG core 2 envelope-chain and strip-law code through `sharc-org` passes.

The envelope generator of the Core2 pass (`pc 0x1274EB..0x127AFF`, 8 slots, 3 chains per slot,
`docs/research/synth-adr-and-strip-law.md`) is a record chain: per pass `y' = y a + b` on the
record `DM(F - 18)` points at. This module sets the slot-0 controller word `DM(0xC1DCC)`, pokes the
chain records every pass (the installer rewrites them every 16th pass) and reads the slot-0 frame
back, and sweeps strip word 3 to read the cutoff coefficient `R` (`DM(0xC2924 + 1)`).

Needs the local ORG core 2 image (`python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram DIR`)
and `cargo build --release -p nord-sharc`. Nothing from the firmware is stored in the repository.
"""

from __future__ import annotations

import csv
import os
import struct
import subprocess
import tempfile
from pathlib import Path

# VERIFIED: slot 0 of the envelope loop, listing 0x71EB.. (pc 0x1274EB..), `I3 = DM(0xC10B3)`.
FRAME = 0xC1188  # F = I3 of slot 0; PEx words at F + o, PEy at F + o + 1
FRAME_LO = FRAME - 32
FRAME_WORDS = 64
CONTROLLER = 0xC1DCC  # slot 0 controller word (PEx); PEy reads DM(0xC1DCD)
CHAIN0 = 0xC0DC8  # first chain: attack, decay, release records of 4 words [a, b, thr, link]
ATTACK, DECAY, RELEASE = 0xC0DC8, 0xC0DCC, 0xC0DD0
FAST_NEG, FAST_POS = 0xC0DD4, 0xC0DD8  # start records selected by the sign field of the word
STATE_Y = -20  # frame offset of the chain-1 state y
STATE_PTR = -18  # frame offset of the current record pointer
R_WORDS = 0xC2924  # DM(0xC2924) = R of the PEx lane, DM(0xC2925) = R of the PEy lane (channel 0)
STRIP0 = 0xE0000


def f2u(x: float) -> int:
    """float32 bits of a Python float."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def u2f(u: int) -> float:
    """float of float32 bits."""
    return struct.unpack("<f", struct.pack("<I", u & 0xFFFFFFFF))[0]


def build_command(
    sharc_org: str,
    image: str,
    passes: int,
    pre: dict[int, int],
    pokes: list[tuple[int, int, int]],
    msgs: list[tuple[int, int, int]],
    bufs: list[str],
    csv_path: str,
) -> list[str]:
    """Command line of one scripted run; `pokes` are `(pass, address, word)`, `msgs` are
    `(pass, strip word offset from 0xE0000, word)`."""
    cmd = [sharc_org, image, "--dev", "0x282c0380=1", "--dev", "0x3108d008=5"]
    cmd += ["--passes", str(passes)]
    for a, v in sorted(pre.items()):
        cmd += ["--pre", f"{a:#x}={v:#x}"]
    for p, a, v in pokes:
        cmd += ["--poke", f"{p}:{a:#x}={v:#x}"]
    for p, w, v in msgs:
        cmd += ["--msg", f"{p}:{w}={v:#x}"]
    for b in bufs:
        cmd += ["--buf", b]
    return cmd + ["--csv", csv_path]


def parse_rows(text: str) -> dict[tuple[int, int], dict[int, int]]:
    """`(pass, channel) -> {index: raw word}` from the `--csv` text of `sharc-org`."""
    out: dict[tuple[int, int], dict[int, int]] = {}
    for row in csv.DictReader(text.splitlines()):
        key = (int(row["pass"]), int(row["channel"]))
        out.setdefault(key, {})[int(row["index"])] = int(row["raw"], 16)
    return out


def run_script(
    sharc_org: str, image: str, passes: int, **kw
) -> dict[tuple[int, int], dict[int, int]]:
    """Runs `sharc-org` and returns the parsed capture rows (raises if the pass faults)."""
    with tempfile.TemporaryDirectory() as tmp:
        cp = os.path.join(tmp, "o.csv")
        cmd = build_command(
            sharc_org,
            image,
            passes,
            kw.get("pre", {}),
            kw.get("pokes", []),
            kw.get("msgs", []),
            kw["bufs"],
            cp,
        )
        r = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if "fault" in r.stdout or not Path(cp).exists():
            raise RuntimeError(r.stdout[-1500:] + r.stderr[-500:])
        return parse_rows(Path(cp).read_text())


def chain_pokes(records: dict[int, tuple[float, float, float, int]], passes: int):
    """Pokes that rewrite the records `{address: (a, b, thr, link)}` before every pass from 1."""
    out = []
    for p in range(1, passes):
        for addr, (a, b, thr, link) in records.items():
            out += [(p, addr + k, w) for k, w in enumerate((f2u(a), f2u(b), f2u(thr), link))]
    return out


def run_slot0(
    sharc_org: str,
    image: str,
    passes: int,
    controller: dict[int, int],
    records: dict[int, tuple[float, float, float, int]],
) -> list[dict[int, int]]:
    """Slot-0 frame words (offset from F -> PEx word) after each pass.

    `controller[p]` is the word poked into `DM(0xC1DCC)` before pass `p` (pass 0 is a `--pre`)."""
    pokes = chain_pokes(records, passes)
    pokes += [(p, CONTROLLER, w) for p, w in controller.items() if p > 0]
    rows = run_script(
        sharc_org,
        image,
        passes,
        pre={CONTROLLER: controller.get(0, 0)},
        pokes=pokes,
        bufs=[f"{FRAME_LO:#x}:{FRAME_WORDS}:f32"],
    )
    out = []
    for p in range(passes):
        cur = rows[(p, 0)]
        out.append({o: cur[o + 32] for o in range(-32, 32)})
    return out


def sweep_cutoff(sharc_org: str, image: str, words: list[int]) -> list[tuple[int, float]]:
    """`R` of channel 0 (PEy lane) after writing strip-0 word 3 = `w` for each `w` in `words`."""
    msgs = [(i, 3, w & 0xFFFFFFFF) for i, w in enumerate(words)]
    rows = run_script(sharc_org, image, len(words) + 1, msgs=msgs, bufs=[f"{R_WORDS:#x}:2:f32"])
    return [(w & 0xFFFFFFFF, u2f(rows[(i + 1, 0)][1])) for i, w in enumerate(words)]


def default_paths() -> tuple[str, str]:
    """`sharc-org` binary and the ORG core 2 image from NS4_SHARC_ORG / NS4_ORG_IMAGE."""
    return (
        os.environ.get("NS4_SHARC_ORG", "target/release/sharc-org"),
        os.environ.get("NS4_ORG_IMAGE", "SRAM_ORG/core2_sram.bin"),
    )


def main() -> None:
    org, image = default_paths()
    recs = {
        ATTACK: (1.0, 0.1, 0.0, DECAY),
        DECAY: (0.8, 0.0, 0.5, DECAY),
        RELEASE: (0.9, 0.0, 0.0, RELEASE),
    }
    for p, fr in enumerate(run_slot0(org, image, 16, {0: 0x01000100}, recs)):
        print(p, f"{fr[STATE_Y]:#010x}", f"{fr[STATE_PTR]:#x}")


if __name__ == "__main__":
    main()
