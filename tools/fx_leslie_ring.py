#!/usr/bin/env python3
"""Leslie processor with an advancing history ring pointer (the level hand-over, `fx-leslie.md` s9).

The cabinet routine B (`0x1237F8`, the Leslie row routine) writes its input pair into a circular
history at `c[-10] + c[-5]` and reads the FIR taps from `c[-10] + c[-4]`, but it never stores
`c[-10]` back. Every oracle run so far left `DM(c[-10])` at its boot value (`0xE3200` left,
`0xE2E08` right), so the history never moved and the FIR saw zeros: the processor was about
50 dB under its input. This tool advances the two pointers by `stride` words per call (a call is
2 frames, 4 words) with `--poke` and prints the output / input rms in dB.

    python tools/fx_leslie_ring.py PNO_core1_sram.bin pno_core1.script --stride 4 [--calls 1500]

Stride 0 reproduces the old -50.3 dB; stride 2, 4, 8 give -4.7, -5.7, -5.8 dB (odd strides break
the L / R pair alignment). Who advances the pointer in the firmware is open.
"""

from __future__ import annotations

import argparse
import math
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_leslie_check as lc
import fx_leslie_run as lr

#: VERIFIED (boot log words, `fx_leslie_ir.py words`): history pointer word `c[-10]` and ring base
#: of the left context `I1 = 0xE359E` and the right context `0xE31A6`; ring length `c[-0x1C]`.
RINGS = ((0xE3594, 0xE3200), (0xE319C, 0xE2E08))
RING_LEN = 0x32A


def ring_pokes(calls: int, stride: int) -> list[str]:
    """`--poke k:c[-10]=base + (stride k) mod len` for both contexts and every call `k`."""
    out: list[str] = []
    if stride == 0:
        return out
    for k in range(calls):
        for ptr, base in RINGS:
            out += ["--poke", f"{k}:{ptr:#x}={base + (stride * k) % RING_LEN:#x}"]
    return out


def rms_db(out: list[float], inp: list[float]) -> float:
    """Output over input rms in dB."""
    r = lambda v: math.sqrt(sum(x * x for x in v) / len(v))  # noqa: E731
    return 20 * math.log10(r(out) / r(inp))


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("pno_core1", type=Path)
    ap.add_argument("script", type=Path)
    ap.add_argument("--stride", type=int, default=4)
    ap.add_argument("--calls", type=int, default=1500)
    ap.add_argument("--skip", type=int, default=300)
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    extra = [t for a, v in lc.rotor_words().items() for t in ("--poke", f"0:{a:#x}={v:#x}")]
    extra += ring_pokes(ns.calls, ns.stride)
    args = lr.build_args(ns.script, ns.calls, extra, lcg=True)
    with tempfile.NamedTemporaryFile("w", suffix=".args", delete=False) as f:
        f.write("\n".join(args))
    text = subprocess.run(
        [str(ns.binary), str(ns.pno_core1), f"@{f.name}"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    dumps = fb.parse_dumps(text)
    f32 = lambda w: struct.unpack("<f", struct.pack("<I", w))[0]  # noqa: E731
    out = [f32(d[lr.IO + j]) for d in dumps[ns.skip :] for j in range(4)]
    inp = [v for row in lr.lcg_words(ns.calls)[ns.skip :] for v in row]
    print(f"stride {ns.stride}: {rms_db(out, inp):.1f} dB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
