#!/usr/bin/env python3
"""Drives the ORG core 2 synth stages (word-1 filter modules) through `sharc-org` passes.

The pass of `docs/research/org-stage-map.md` calls the B stage of channel 0 as
`I8 = DM(0xC0B8C)`, `I3 = 0xC2942` (frame), `I4 = 0xC1F96` (A record: the 4 input samples sit at
`I4 - 13, -11, -9, -7`, PEy lane at word + 1), `I5 = 0xC28FA` (common constants), SIMD on. This
module installs a stage there, replaces the A stage of slot 0 by a harmless stub, optionally
patches the three control stores that rewrite the frame every pass, feeds samples through the A
record and reads the filtered samples back, 4 per pass.

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

# VERIFIED: pass call set-up, `docs/research/org-stage-map.md` s2, listing 0x127B01..0x127BA8.
B_ENTRY = 0xC0B8C  # DM word that holds the B stage of channel 0 (I8 of the call)
A_ENTRY = 0xC10B8  # A stage of slot 0 (x channel), replaced by the null stage 0x125B79
FRAME = 0xC2942  # I3 of the channel 0 B call; frame fields are I3 + offset
RECORD = 0xC1F96  # I4 of the channel 0 B call; samples at RECORD - 13 + 2 j (PEx), +1 (PEy)
COMMON = 0xC28FA  # I5 of the B calls
IN0 = RECORD - 13
STUB_A = 0x125B79
# Stores that rewrite the frame words -32, -30, -28 of channel 0 in every pass (pc, parcels of 3).
CONTROL_STORES = (0x1262E2, 0x1262E5, 0x12696B)
NOP = 0x0001
SAMPLES_PER_PASS = 4


def f2u(x: float) -> int:
    """float32 bits of a Python float."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def u2f(u: int) -> float:
    """float of float32 bits."""
    return struct.unpack("<f", struct.pack("<I", u & 0xFFFFFFFF))[0]


def patch_controls(image: bytes) -> bytes:
    """Copy of the SRAM image with the three frame-rewriting stores of channel 0 turned into NOPs.

    ASSUMED: the VISA short word 0x0001 is NOP (the `sharc-run` self test uses it). Checked by the
    effect: with the patch the poked frame words survive the passes.
    """
    data = bytearray(image)
    for pc in CONTROL_STORES:
        for k in range(3):
            struct.pack_into("<H", data, 2 * (pc + k), NOP)
    return bytes(data)


def message_script(
    stage: int,
    frame: dict[int, tuple[int, int]],
    xs: list[float],
    ys: list[float] | None = None,
    extra: dict[int, int] | None = None,
) -> tuple[str, int]:
    """Script text (`PASS ADDR VALUE`, `--msg-base 0`) and the number of passes.

    `frame` maps a frame offset to the (PEx, PEy) words; `xs`, `ys` are the input samples (the
    length is rounded up to a multiple of 4 with zeros).
    """
    n = -(-len(xs) // SAMPLES_PER_PASS)
    ys = ys if ys is not None else [0.0] * len(xs)
    lines = [f"0 {B_ENTRY:#x} {stage:#x}", f"0 {A_ENTRY:#x} {STUB_A:#x}"]
    for off, (wx, wy) in sorted(frame.items()):
        lines.append(f"0 {FRAME + off:#x} {wx:#x}")
        lines.append(f"0 {FRAME + off + 1:#x} {wy:#x}")
    for addr, word in (extra or {}).items():
        lines.append(f"0 {addr:#x} {word:#x}")
    for p in range(n):
        for k in range(SAMPLES_PER_PASS):
            j = p * SAMPLES_PER_PASS + k
            x = xs[j] if j < len(xs) else 0.0
            y = ys[j] if j < len(ys) else 0.0
            lines.append(f"{p} {IN0 + 2 * k:#x} {f2u(x):#x}")
            lines.append(f"{p} {IN0 + 2 * k + 1:#x} {f2u(y):#x}")
    return "\n".join(lines) + "\n", n


def run_stage(
    sharc_org: str,
    image: str,
    stage: int,
    frame: dict[int, tuple[int, int]],
    xs: list[float],
    ys: list[float] | None = None,
    extra: dict[int, int] | None = None,
    watch_frame: tuple[int, int] = (-32, 52),
) -> dict:
    """Runs the stage over the samples; returns `x`, `y` (output floats) and the frame words
    `frames[pass][offset] = (PEx, PEy)` after every pass."""
    script, n = message_script(stage, frame, xs, ys, extra)
    with tempfile.TemporaryDirectory() as tmp:
        sp, cp = Path(tmp, "m.txt"), Path(tmp, "o.csv")
        sp.write_text(script)
        lo, cnt = watch_frame
        cmd = [
            sharc_org,
            image,
            "--dev",
            "0x282c0380=1",
            "--dev",
            "0x3108d008=5",
            "--passes",
            str(n),
            "--msg-base",
            "0",
            "--msg-script",
            str(sp),
            "--buf",
            f"{IN0:#x}:{2 * SAMPLES_PER_PASS}:f32",
            "--buf",
            f"{FRAME + lo:#x}:{cnt}:f32",
            "--csv",
            str(cp),
        ]
        r = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if "fault" in r.stdout or not cp.exists():
            raise RuntimeError(r.stdout[-2000:] + r.stderr[-500:])
        rows = list(csv.DictReader(cp.open()))
    out_x: list[float] = []
    out_y: list[float] = []
    frames: list[dict[int, tuple[int, int]]] = [dict() for _ in range(n)]
    buf0: dict[int, dict[int, int]] = {}
    for row in rows:
        p, ch, idx = int(row["pass"]), int(row["channel"]), int(row["index"])
        raw = int(row["raw"], 16)
        if ch == 0:
            buf0.setdefault(p, {})[idx] = raw
        else:
            frames[p][idx] = raw
    for p in range(n):
        for k in range(SAMPLES_PER_PASS):
            out_x.append(u2f(buf0[p][2 * k]))
            out_y.append(u2f(buf0[p][2 * k + 1]))
    pairs = []
    for fr in frames:
        pairs.append({lo + i: (fr[i], fr[i + 1]) for i in range(0, cnt - 1)})
    return {"x": out_x[: len(xs)], "y": out_y[: len(xs)], "frames": pairs}


def default_paths() -> tuple[str, str]:
    """`sharc-org` binary and the ORG core 2 image from NS4_SHARC_ORG / NS4_ORG_IMAGE."""
    return (
        os.environ.get("NS4_SHARC_ORG", "target/release/sharc-org"),
        os.environ.get("NS4_ORG_IMAGE", "SRAM_ORG/core2_sram.bin"),
    )


def main() -> None:
    org, image = default_paths()
    out = run_stage(org, image, 0x12566E, {}, [1.0] + [0.0] * 31)
    print(" ".join(f"{v:.6g}" for v in out["x"][:16]))


if __name__ == "__main__":
    main()
