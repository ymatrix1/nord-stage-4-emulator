#!/usr/bin/env python3
"""The reverb core `0x123F61` on the **real instance** of the FX image (types 0 and 1).

The FX DSP image carries the reverb instance in its initial DM (`docs/research/fx-reverb-leslie.md`
s8): context `RCTX = 0xB27E8`, delay lines of 5000 words in SDRAM (`0x0802F000`, `0x08030388` in
core 2), the all-pass buffer `0xE24E7` (3140 words), the loop state array `0xB2849`, the smoothing
pole `c[0x11] = 0x3F7DC91D`. The ARM adds the 39 type words (`fx-type-words.csv`: taps `-2658` /
`-4068` words for types 0, 1). The PM list `c[-12] = 0x9454B` is read only by dead loads (two
registers that the next pass overwrites): zero or random, the outputs are identical.

    python tools/fx_rvbreal.py check SRAM/core2_sram.bin --type 0 --calls 300
    python tools/fx_rvbreal.py impulse SRAM/core2_sram.bin --type 0 --calls 6000
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_model_rvbcore as core
import fx_oracle as fo
import fx_reverb_model as rm
import fx_rvbcore_check as chk
from fx_pe import M32, Machine, Missing

RCTX = 0xB27E8  # VERIFIED: the one instance whose pointer words are in the image (both cores)
CSV = chk.CSV
IN_ADDR = chk.IN_ADDR
LINE_A_OFF, LINE_B_OFF = 39, 44  # `c[39]`, `c[44]`: base words of the delay lines


def image_words(image: str, lo: int, n: int) -> dict[int, int]:
    """Words `lo .. lo + n` of the flat SRAM image (word index = DM address)."""
    raw = Path(image).read_bytes()
    return {a: struct.unpack_from("<I", raw, 4 * a)[0] for a in range(lo, lo + n)}


def real_layout(image: str, rvb_type: int = 0, amount: int = 127) -> dict[int, int]:
    """Initial DM of the real instance: image words, the ARM type words of `rvb_type` and the
    `kRvbAmt` words (send `c[0x12]`, dry `c[0x13]`, [fx-reverb-leslie](s2) law) with the smoothed
    copies `c[0x14]`, `c[0x15]` already settled."""
    x = amount / 128
    send = 1.0 if amount >= 127 else x * x * (2 - x)  # VERIFIED: (1, 0) at 127 exactly
    dry = (1 - send * send) ** 0.5 if amount < 127 else 0.0
    words = {0x12: fb.bits(send), 0x13: fb.bits(dry), 0x14: fb.bits(send), 0x15: fb.bits(dry)}
    mem = {RCTX + o: w for o, w in words.items()}
    mem |= {a: w for a, w in image_words(image, RCTX - 0x20, 0x60).items() if w}
    with open(CSV, newline="") as f:
        for r in csv.DictReader(f):
            if (r["slot"], r["type"], r["kinds"]) == ("Rvb", str(rvb_type), "organ"):
                mem[RCTX + int(r["ctx"])] = int(r["word"], 16)
    return mem


def run_oracle(image: str, binary: Path, frames, mem0, trace: bool = False):
    """Oracle text for the 4-frame wrapper `0x12417E` on `frames` (8 words per call); dumps the
    output frames and `c[0x15]`."""
    sets = {"I0": IN_ADDR, "I1": RCTX, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("rvbreal", rm.PC4, rm.END, sets, lambda *_: None, lambda *_: {}, mem0)
    dumps = {IN_ADDR + k for k in range(8)} | {RCTX + 0x15}
    cases = [{IN_ADDR + k: w for k, w in enumerate(f)} for f in frames]
    return fo.run_oracle(image, binary, rt, cases, dumps, trace=trace)


def model_machine(mem: dict[int, int]) -> Machine:
    ir = {f"{k}{n}": 0 for k in "imlb" for n in range(16)}
    ir.update({"i0": IN_ADDR, "i1": RCTX, "m1": 1, "m2": 2, "m7": M32, "m9": 1, "m10": 2,
               "m15": M32})  # fmt: skip
    return Machine(mem, ir)


def core_call(mem: dict[int, int], f0, f2):
    """The core model on two frames (pairs of values): returns the two frames it returns."""
    m = model_machine(mem)
    m.x[0], m.y[0] = (fb.word(v) for v in f0)
    m.x[4], m.y[4] = (fb.word(v) for v in f2)
    core.run(m)
    return tuple(rm.pairs([core.wd(m.x[r]), core.wd(m.y[r])])[0] for r in (12, 14))


def wrapper_call(mem, st1, st2, frames):
    """`0x12417E` with the core model: bank 1, core, hold, bank 2, mix; returns output words."""
    y = rm.bank(frames, st1)
    w0, w2 = (tuple(rm.store(v) for v in w) for w in core_call(mem, y[0], y[2]))
    z = rm.bank([w0, w0, w2, w2], st2)
    gain = fb.from_bits(mem[RCTX + 0x15])
    return rm.flat([rm.mix(x, gain, zz) for x, zz in zip(frames, z)])


def check(image: str, binary: Path, rvb_type: int, calls: int) -> tuple[int, int, list]:
    """Model against the oracle on the real instance: the output words of every call."""
    import random

    rng = random.Random(7)
    mem0 = real_layout(image, rvb_type)
    frames = [[fb.bits(rng.uniform(-1, 1)) for _ in range(8)] for _ in range(calls)]
    got = fb.parse_dumps(run_oracle(image, binary, frames, mem0))
    mem = Missing(mem0, default=0)
    st1, st2 = [Fraction(0)] * 12, [Fraction(0)] * 12
    bad = total = 0
    first: list = []
    for k, fr in enumerate(frames):
        mem.update({IN_ADDR + j: w for j, w in enumerate(fr)})
        want = wrapper_call(mem, st1, st2, rm.pairs(fr))
        for j in range(8):
            total += 1
            g = got[k][IN_ADDR + j]
            if g != want[j] and (g | want[j]) & 0x7FFFFFFF:
                bad += 1
                if len(first) < 4:
                    first.append((k, j, hex(g), hex(want[j])))
    return bad, total, first


def response(image: str, binary: Path, rvb_type: int, calls: int, kind: str, overrides=None):
    """Oracle output `(L, R)` samples at 48 kHz for an impulse or white noise (amplitude 0.25)."""
    import random

    import numpy as np

    rng = random.Random(11)
    mem0 = real_layout(image, rvb_type)
    mem0.update({RCTX + o: w for o, w in (overrides or {}).items()})
    if kind == "impulse":
        frames = [[0] * 8 for _ in range(calls)]
        frames[0][0] = frames[0][1] = fb.bits(1.0)
    else:
        frames = [[fb.bits(rng.uniform(-0.25, 0.25)) for _ in range(8)] for _ in range(calls)]
    got = fb.parse_dumps(run_oracle(image, binary, frames, mem0))
    words = np.array(
        [[got[k][IN_ADDR + j] for j in range(8)] for k in range(calls)], dtype=np.uint32
    )
    out = words.view(np.float32).reshape(-1, 2).astype(np.float64)
    inp = np.array(frames, dtype=np.uint32).view(np.float32).reshape(-1, 2).astype(np.float64)
    return inp, out


def t60_s(h, fs: float) -> tuple[float, float]:
    """Schroeder backward integration of `h`: `(T60 from the -5..-35 dB slope, EDT 0..-10 dB)`."""
    import numpy as np

    e = np.cumsum((h**2)[::-1])[::-1]
    db = 10 * np.log10(np.maximum(e / e[0], 1e-30))
    t = np.arange(len(h)) / fs

    def slope(lo: float, hi: float) -> float:
        m = (db <= lo) & (db >= hi)
        c = np.polyfit(t[m], db[m], 1)
        return -60.0 / c[0]

    return slope(-5, -35), slope(0, -10)


def echo_density(h, fs: float, win_s: float = 0.02) -> list[tuple[float, float]]:
    """Normalised echo density (Abel and Huang): fraction of samples beyond one standard
    deviation in a sliding window, divided by 0.3173 (gaussian), at 0.1, 0.2, 0.4, 0.8 s."""
    import numpy as np

    n = int(win_s * fs)
    out = []
    for t0 in (0.1, 0.2, 0.4, 0.8):
        i = int(t0 * fs)
        w = h[i : i + n]
        out.append((t0, float(np.mean(np.abs(w) > w.std()) / 0.3173)))
    return out


def band_db(inp, out, fs: float, bands) -> list[float]:
    """Mean wet-to-input power ratio in dB per band, from the noise response."""
    import numpy as np

    n = 4096
    num = den = 0
    for i in range(0, len(inp) - n, n):
        win = np.hanning(n)
        num = num + np.abs(np.fft.rfft(out[i : i + n] * win)) ** 2
        den = den + np.abs(np.fft.rfft(inp[i : i + n] * win)) ** 2
    f = np.fft.rfftfreq(n, 1 / fs)
    return [
        float(10 * np.log10(num[(f >= lo) & (f < hi)].sum() / den[(f >= lo) & (f < hi)].sum()))
        for lo, hi in bands
    ]


BANDS = (
    (60, 250),
    (250, 500),
    (500, 1000),
    (1000, 2000),
    (2000, 4000),
    (4000, 8000),
    (8000, 16000),
)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["check", "responses"])
    p.add_argument("image")
    p.add_argument("--type", type=int, default=0)
    p.add_argument("--calls", type=int, default=100)
    p.add_argument("--bin")
    a = p.parse_args(argv)
    if a.cmd == "responses":
        import numpy as np

        binary = fb.find_binary(a.bin)
        for t in (0, 1):
            _, imp = response(a.image, binary, t, a.calls, "impulse")
            h = imp[:, 0] + imp[:, 1]
            h[0:8] = 0  # the direct (dry) part
            t60, edt = t60_s(h, 48000)
            print(f"type {t}: T60 {t60:.2f} s, EDT {edt:.2f} s, peak {np.abs(imp).max():.4f}")
            print("  echo density", [(a_, round(b_, 2)) for a_, b_ in echo_density(h, 48000)])
            ni, no = response(a.image, binary, t, a.calls, "noise")
            print(
                "  wet/in dB per band",
                [round(v, 1) for v in band_db(ni[:, 0], no[:, 0], 48000, BANDS)],
            )
        return 0
    bad, total, first = check(a.image, fb.find_binary(a.bin), a.type, a.calls)
    print(f"check: {total - bad}/{total} words identical {first}")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
