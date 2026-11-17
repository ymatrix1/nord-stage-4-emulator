#!/usr/bin/env python3
"""Synth filter links B and C in the ORG core 2 pass (SHARC oracle probes).

**B, resonance position (`W28 = DM(0xC2926)`).** The ARM writes `kFltRes` as the float word
`w = knob / 127 - 0.5` into the layer common block word 7 (`DM(0xE02EF)`, layer A). The pass
smooths it with `w10 <- 0.995 w10 + 0.005 target` once per pass and turns it into the table position
`idx(n) = 64 + 128 w (1 - 0.995^(n + 1))` at pass `n`. The value read after 60 passes
(`64 + 33.25 w`, `docs/research/synth-filter-type-corners.md`) is the transient `n = 59`
(`128 (1 - 0.995^60) = 33.25`); the settled position is `128 knob / 127`. `res` prints both.

**C, stage calls per pass.** `calls` pokes the stage address into the 16 slot records
`DM(0xC0B8C + 38 k)` and counts, with `sharc-org --trace PC:PC`, how often the stage entry runs in
one pass once the real ARM load stream has settled (`docs/research/org-system-run.md`).

    cargo build --release -p nord-sharc
    uv run python tools/ns4_dsp_ldr.py target/DSP/dsp_image_2_ORG.bin --sram DIR
    NS4_ORG_IMAGE=DIR/core2_sram.bin python tools/synth_filter_links.py res
    NS4_ORG_IMAGE=DIR/core2_sram.bin python tools/synth_filter_links.py calls --script flat.script
"""

from __future__ import annotations

import argparse
import csv
import re
import struct
import subprocess
import sys

import synth_adr_oracle as adr
from synth_cutoff_note_probe import knob_word, voice_word

W28_ADDRESS = 0xC2926
COMMON_RES_WORD = 0x2E9 + 6  # layer A common word 7
# VERIFIED (listing 0x1260BC..0x1261CC, docs/research/synth-stage-equations.md s4): the strip word
# smoother `w <- 0.995 w + 0.005 target`, run once per pass; measured in `res_trace` (error < 0.02).
SMOOTHER_POLE = 0.995
# VERIFIED (run, `res_trace`): the table position of the settled word is `128 w` above the centre.
POSITION_SPAN, POSITION_CENTRE = 128.0, 64.0
# VERIFIED (run, `docs/research/synth-filter-links.md` s3): slot records and call counts.
SLOT_STRIDE, SLOT_STAGE_WORD, SLOTS = 38, 0xC0B8C, 16
STAGES = {
    "LP12 0x1253BD": 0x1253BD,
    "LP24 0x12566E": 0x12566E,
    "Ladder24 0x12549D": 0x12549D,
    "type3 0x1257B2": 0x1257B2,
    "BP 0x125913": 0x125913,
    "HP 0x125A50": 0x125A50,
}


def res_word(knob: int) -> int:
    """The ARM word of `kFltRes`: float32 `-0.5 + knob / 127`."""
    return struct.unpack("<I", struct.pack("<f", -0.5 + knob / 127.0))[0]


def position(word: int) -> float:
    """Table position `idx + fraction` of a `W28` word."""
    return (word >> 24) + (word & 0xFFFFFF) / float(1 << 24)


def settled_position(knob: int) -> float:
    """Settled table position of the resonance word: `128 knob / 127`."""
    return POSITION_CENTRE + POSITION_SPAN * (knob / 127.0 - 0.5)


def position_after(knob: int, n: int) -> float:
    """Table position read after pass `n` (0-based): the smoother has run `n + 1` times."""
    w = knob / 127.0 - 0.5
    return POSITION_CENTRE + POSITION_SPAN * w * (1.0 - SMOOTHER_POLE ** (n + 1))


def res_trace(org: str, image: str, knob: int, passes: int) -> list[float]:
    """`W28` position after every pass for one resonance knob with a sounding voice (note 60)."""
    msgs = [(0, 3, knob_word(80)), (0, 0x2E9, knob_word(80)), (0, COMMON_RES_WORD, res_word(knob))]
    rows = adr.run_script(
        org,
        image,
        passes,
        msgs=msgs,
        pre={adr.CONTROLLER: voice_word(60)},
        bufs=[f"{W28_ADDRESS:#x}:1"],
    )
    return [position(rows[(p, 0)][0]) for p in range(passes)]


def slot_pokes(stage: int, pass_no: int = 44) -> list[str]:
    """`--msg` arguments that write `stage` into the 16 slot records before pass `pass_no`."""
    out = []
    for k in range(SLOTS):
        out += ["--msg", f"{pass_no}:{SLOT_STAGE_WORD + SLOT_STRIDE * k:#x}={stage:#x}"]
    return out


PASS_RX = re.compile(r"^pass (\d+): (\d+) steps")
TRACE_RX = re.compile(r"^\s+trace pc ([0-9a-f]+)")


def count_calls(text: str, pc: int) -> dict[int, tuple[int, int]]:
    """`{pass: (steps, entry hits)}` from a `sharc-org --trace PC:PC` report."""
    out: dict[int, tuple[int, int]] = {}
    cur = -1
    for line in text.splitlines():
        m = PASS_RX.match(line)
        if m:
            cur = int(m.group(1))
            out[cur] = (int(m.group(2)), 0)
            continue
        m = TRACE_RX.match(line)
        if m and cur >= 0 and int(m.group(1), 16) == pc:
            steps, hits = out[cur]
            out[cur] = (steps, hits + 1)
    return out


def calls_command(sharc_org: str, image: str, script: str, stage: int, passes: int) -> list[str]:
    """Command: the real load stream with every slot set to `stage`, tracing the stage entry."""
    cmd = [sharc_org, image, "--dev", "0x282c0380=1", "--dev", "0x3108d008=5"]
    cmd += ["--pre", "0x3e021=1", "--passes", str(passes), "--msg-base", "0"]
    cmd += ["--msg-script", script, *slot_pokes(stage)]
    cmd += ["--trace", f"{stage:#x}:{stage:#x}:100000"]
    return cmd


def write_csv(path: str | None, header: list[str], rows: list[list[object]]) -> None:
    """Writes `rows` under `header` when `path` is given (derived numbers only)."""
    if not path:
        return
    with open(path, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(header)
        w.writerows(rows)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the firmware image
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("what", choices=("res", "calls"))
    ap.add_argument("--script", default="flat.script")
    ap.add_argument("--passes", type=int, default=1500)
    ap.add_argument("--csv", help="also write the table as CSV")
    ns = ap.parse_args(argv)
    org, image = adr.default_paths()
    if ns.what == "res":
        print("knob  n=59 oracle model | n=1499 oracle model | settled")
        rows: list[list[object]] = []
        for knob in (0, 32, 64, 96, 107, 127):
            t = res_trace(org, image, knob, ns.passes)
            last = ns.passes - 1
            print(
                f"{knob:4d} {t[59]:8.3f} {position_after(knob, 59):8.3f} | "
                f"{t[-1]:8.3f} {position_after(knob, last):8.3f} | {settled_position(knob):8.3f}"
            )
            rows.append(
                [knob, f"{t[59]:.3f}", f"{position_after(knob, 59):.3f}", f"{t[-1]:.3f}"]
                + [f"{position_after(knob, last):.3f}", f"{settled_position(knob):.3f}"]
            )
        head = ["knob", "oracle_n59", "model_n59", f"oracle_n{ns.passes - 1}"]
        write_csv(ns.csv, [*head, f"model_n{ns.passes - 1}", "settled"], rows)
        return 0
    print("stage            steps/pass  calls/pass")
    out: list[list[object]] = []
    for name, pc in STAGES.items():
        text = subprocess.run(
            calls_command(org, image, ns.script, pc, 60), capture_output=True, text=True
        ).stdout
        steps, hits = count_calls(text, pc)[59]
        print(f"{name:18s} {steps:6d} {hits:6d}")
        out.append([name, f"{pc:#x}", steps, hits])
    write_csv(ns.csv, ["stage", "entry", "steps_per_pass", "calls_per_pass"], out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
