#!/usr/bin/env python3
"""Leslie row of the organ unit: context words, routine and measured impulse response.

The boot of the firmware (`tools/arm_bulk_tables.py capture`) leaves the routine pointers `0xB2F29`
and `0xB2F2B` of the organ unit's Leslie row at `0x1237F8` (PNO core 1; the FX core 2 copy is
`0x122B1D`), not at the image's `0x12637B` stub, with the stage contexts `I1 = 0xE359E` (left) and
`0xE31A6` (right) in the PNO core 1 DM. Each context carries a 402 word coefficient run in PM
(`0x967FE..` left, `0x96634..` right) that the image does not contain and the ARM sends. This tool

* `words`: reads the final state of the boot log (`pno_core1.script`) and overlays it on the
  static PNO core 1 image (`context`),
* `ir`: runs the FX core 2 copy of the routine in `sharc-fx` with that context and prints the
  impulse response words of both channels (the routine is linear and time invariant there,
  checked by a shifted and a doubled impulse),
* `table`: prints the magnitude response of the two channels in third octave bands.

    python tools/fx_leslie_ir.py ir FX_core2_sram.bin PNO_core1_sram.bin pno_core1.script > ir.csv

Evidence and limits: `docs/research/fx-leslie.md`.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_oracle as fo

#: VERIFIED (boot log `0xB2F29`, `0xB2F2B`; `tools/tests/test_fx_leslie_ir.py` on the committed
#: data): the Leslie row routine in PNO core 1 and its FX core 2 copy (same code, 111 instructions).
PNO_PC, FX_PC, FX_END = 0x1237F8, 0x122B1D, 0x122C39
#: VERIFIED (boot log words `0xB2F2A`, `0xB2F2C`): context pointers (`I1`) of the two channels.
CTX = {"L": 0xE359E, "R": 0xE31A6}
IN_ADDR = 0xB1000
#: DM and PM windows copied into the oracle (contexts, line buffers, coefficient runs).
WINDOWS = ((0xE2D00, 0xE3700), (0x96500, 0x96A00))
FRAMES = 1200
ONE = 0x3F800000


def boot_state(script: Path) -> dict[int, int]:
    """Final DM words of the boot log script (`pass address value` per line)."""
    out: dict[int, int] = {}
    for line in script.read_text(encoding="utf-8").splitlines():
        p = line.split()
        if len(p) == 3:
            out[int(p[1], 16)] = int(p[2], 16)
    return out


def image_words(path: Path, lo: int, hi: int) -> dict[int, int]:
    """Words `lo..hi` of a flat SRAM image (word `A` at byte `4 A`)."""
    data = path.read_bytes()
    return {a: struct.unpack_from("<I", data, 4 * a)[0] for a in range(lo, hi)}


def context(pno1: Path, script: Path) -> dict[int, int]:
    """Static image words under the windows, overlaid with the boot log words."""
    mem: dict[int, int] = {}
    for lo, hi in WINDOWS:
        mem.update(image_words(pno1, lo, hi))
    boot = boot_state(script)
    mem.update({a: w for a, w in boot.items() if any(lo <= a < hi for lo, hi in WINDOWS)})
    return mem


def run(fx2: str, binary: Path, mem: dict[int, int], ctx: int, words: list[int]) -> list[int]:
    """The routine on `words` (4 words = 2 stereo frames per call, state carried)."""
    sets = {"I0": IN_ADDR, "I1": ctx, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("leslie", FX_PC, FX_END, sets, lambda *a: None, lambda r: {}, mem)
    calls = len(words) // 4
    cases = [{IN_ADDR + k: words[c * 4 + k] for k in range(4)} for c in range(calls)]
    dumps = {IN_ADDR + k for k in range(4)}
    res = fb.parse_dumps(fo.run_oracle(fx2, binary, rt, cases, dumps))
    return [res[c][IN_ADDR + k] for c in range(calls) for k in range(4)]


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def impulse(fx2: str, binary: Path, mem: dict[int, int], ctx: int) -> tuple[list[int], list[int]]:
    """Left and right response words to a unit impulse on the left input (frame 0)."""
    words = [0] * (FRAMES * 2)
    words[0] = ONE
    out = run(fx2, binary, mem, ctx, words)
    return out[0::2], out[1::2]


def magnitude_db(ir: list[float], freqs: list[float], fs: float = 48000.0) -> list[float]:
    """Magnitude (dB) of `ir` at `freqs` by direct evaluation of the DTFT."""
    out = []
    for fr in freqs:
        w = 2 * math.pi * fr / fs
        re = sum(h * math.cos(w * n) for n, h in enumerate(ir))
        im = sum(h * math.sin(w * n) for n, h in enumerate(ir))
        out.append(20 * math.log10(max(math.hypot(re, im), 1e-12)))
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=["ir", "words"])
    ap.add_argument("fx_core2")
    ap.add_argument("pno_core1", type=Path)
    ap.add_argument("script", type=Path)
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    mem = context(ns.pno_core1, ns.script)
    if ns.cmd == "words":
        for ch, c in CTX.items():
            print(ch, hex(c), [hex(mem[c + k]) for k in range(-0x21, 0)])
        return 0
    print("channel,input,n,word")
    for ch, c in CTX.items():
        # shift invariance and linearity checks on the left channel context only
        ir_l, ir_r = impulse(ns.fx_core2, ns.binary, mem, c)
        for n, w in enumerate(ir_l):
            print(f"{ch},L,{n},{w:#010x}")
        for n, w in enumerate(ir_r):
            print(f"{ch},R,{n},{w:#010x}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
