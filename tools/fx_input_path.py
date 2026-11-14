#!/usr/bin/env python3
"""Measure the gain from the FX Core1 input buffers to the master output words in `sharc-org`.

Reproduces `docs/research/piano-receiver-scale.md`: the SMP serial stream (SPORT3B, 10-word frames
at `DM(0xC0234)`, `DM(0xC023E)` in the `0xC01BC` buffer set) and the PNO 2D buffers (`DM(0xC01BC)`
..) reach the master output words `DM(0xC04D8)` with the same gain, `DM(0xB651A)` = 0.1866 once the
master level word `DM(0xB6518)` is 1.0 (it is 0 in the image and no writer was found).

    python tools/fx_input_path.py FX/core1_sram.bin smp          # SMP words -> output
    python tools/fx_input_path.py FX/core1_sram.bin pno          # PNO 2D words -> output
    python tools/fx_input_path.py FX/core1_sram.bin dac          # output words -> DAC word

`FX/core1_sram.bin` comes from `tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --sram FX`. Needs the built
`sharc-org` (`cargo build --release -p nord-sharc`). Nothing here is committed data.
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

BOOT = [
    "--entry", "0x1211E3", "--trigger", "0xC02C1", "--idle-pc", "0x12128B",
    "--rx-irq", "P4I:0x41", "--dev", "0x3108c06c=0x400", "--dev", "0x3108c00c=4",
    "--dev", "0x28b00b00=1", "--dev", "0x3108d008=5", "--flag", "1=1", "--boot", "3000000",
]  # fmt: skip
MASTER_LEVEL = 0xB6518
SMP_WORDS = (0xC0234, 0xC0235, 0xC023E, 0xC023F)
PNO_WORDS = (0xC01BC, 0xC01BD)
OUT_WORD = 0xC04D8
TX_WORD = 0xC0430


def f32_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def bits_f32(b: int) -> float:
    return struct.unpack("<f", struct.pack("<I", b))[0]


def parse_dump(text: str) -> dict[int, list[int]]:
    """`DM(0xADDR..): w0 w1 ...` lines of sharc-org as {address: [words]}."""
    out: dict[int, list[int]] = {}
    for line in text.splitlines():
        if line.startswith("DM(") and ": " in line:
            head, words = line.split(": ", 1)
            out[int(head[3:].split("..")[0], 16)] = [int(w, 16) for w in words.split()]
    return out


def script_lines(passes: int, pokes: list[tuple[int, float]]) -> str:
    """`PASS ADDR WORD` message lines poking every pass (`--msg-base 0` makes ADDR absolute)."""
    return "".join(f"{p} {a} {f32_bits(v)}\n" for p in range(passes) for a, v in pokes)


def gain(out: float, inp: float) -> float:
    return out / inp


def run(exe: str, image: str, passes: int, pokes, dumps) -> dict[int, list[int]]:
    with tempfile.TemporaryDirectory() as d:
        script = Path(d) / "msgs.txt"
        script.write_text(script_lines(passes, pokes))
        cmd = [exe, image, *BOOT, "--passes", str(passes), "--max-pass-steps", "50000"]
        cmd += ["--msg-base", "0", "--msg-script", str(script)]
        for a, n in dumps:
            cmd += ["--dump", f"0x{a:X}:{n}"]
        res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return parse_dump(res.stdout)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("image")
    ap.add_argument("what", choices=["smp", "pno", "dac"])
    ap.add_argument("--exe", default="target/release/sharc-org")
    ap.add_argument("--passes", type=int, default=1500)
    ap.add_argument("--level", type=float, default=0.25)
    a = ap.parse_args(argv)
    master = [(MASTER_LEVEL, 1.0)]
    if a.what == "dac":
        pokes = [(OUT_WORD + k, a.level) for k in (0, 1)] + [(OUT_WORD + 8 + k, a.level) for k in (0, 1)]
        dumps = [(TX_WORD, 4)]
        got = run(a.exe, a.image, 1, pokes, dumps)[TX_WORD][0]
        print(f"DAC word for {a.level}: 0x{got:08X} = {struct.unpack('<i', struct.pack('<I', got))[0] / 2**31:.6f} of 2^31")
        return 0
    words = SMP_WORDS if a.what == "smp" else PNO_WORDS
    pokes = [(w, a.level) for w in words] + master
    out = run(a.exe, a.image, a.passes, pokes, [(OUT_WORD, 2)])[OUT_WORD]
    g = gain(bits_f32(out[0]), a.level)
    print(f"{a.what}: input {a.level} -> output {bits_f32(out[0]):.6f}, gain {g:.5f} after {a.passes} passes")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
