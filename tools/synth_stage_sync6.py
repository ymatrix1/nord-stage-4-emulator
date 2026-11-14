#!/usr/bin/env python3
"""Bit-exact models of the ANALOG SYNC wave 6 and 8 stages `0x1239BE`, `0x123FE9` (ORG core 2).

Listings `0x24737C..0x2478F6` (SYNC 6) and `0x247FD2..0x2484D0` (SYNC 8), read word by word
(`docs/research/synth-stage-models-7.md`). Same frame as `0x122C5C` (`synth_stage_sync3.py`): two
oscillator passes (`LCNTR = 2`), the common mix, the table interpolation `h` at `s5`, but no pulse
word, a fixed edge width 0.5 for the first wave and no output high pass. With `f` = pitch word,
`a = inc1 2^-31` (oscillator 1 only), `x_k = phase + k inc` (k = 1..4):

    SYNC 6  h = interp(s5)        SYNC 8  h = 2 interp(0.75 s5)           (stored)
    g1 = fade(a h);  g2 = fade(a);  q = max(0.5 / (a h), 2);  q2 = max(0.5 / a, 2)   (stored)
    e2 = 1 - clip(4 a, 1);  c2 = 0.5 - e2 / 2 (SYNC 6) or 0.5 (SYNC 8) (stored);  k = c2 / (1 - c2)
    tA = wave(|y_k|, 0.5, q),  y_k = ASHIFT(UUF(x_k, FIX(h, 25)), 7);   A_k = tA g1 + 1  (stored)
    tB = wave(|x'_k|, c2, q2),  x' = x (SYNC 6) or x - 0xC0000000 - 2 inc (SYNC 8);  v = 0.5 tB + 0.5
    SYNC 6  out = (A v)(g2 FLOAT(x ^ 2^31) - 1) + (1 - k / 2)          scale -0.5 g2 P[16]
    SYNC 8  out = A v - (1 - k / 2)                                    scale g2 P[16]
"""

from __future__ import annotations

import synth_stage_sync as sy
import synth_stage_sync3 as s3
import synth_wave_model as m

TWO = m.X(2)
FOUR = m.X(4)
THREE_QUARTERS = m.X(3, 4)


def lookup_at(s5: m.X, words: list[int]) -> m.X:
    """Table interpolation at a spread value that is already a register (SYNC 8: `0.75 s5`)."""
    n = m.fix(s5, sy.SPREAD_SHIFT)
    i = (n >> 23) & 0xFF
    frac = m.flt(((n & 0x7FFFFF) << 8) & 0xFFFFFFFF, -31)
    lo, hi = m.ld(words[i]), m.ld(words[i + 1])
    return m.add(m.mul(m.sub(hi, lo), frac), lo)


def setup(ln: m.Lane, words: list[int], v: int) -> dict:
    """The stored lane constants of variant `v` (6 or 8); every `DM(.., I4)` word is reloaded."""
    ld, st = m.ld, m.st
    f = ld(ln.pitch)
    inc1 = m.fix(m.mul(f, ld(ln.ratio1)), 31)
    a_st = ld(st(m.flt(inc1, -31)))
    a = m.flt(inc1, -31)
    s5 = ld(ln.spread)
    if v == 8:
        h_reg = lookup_at(m.mul(s5, THREE_QUARTERS), words)
        h_reg = m.add(h_reg, h_reg)
    else:
        h_reg = sy.lookup(ln.spread, words)
    ah_st = ld(st(m.mul(a, h_reg)))
    # `DM(0, I4) = R2` then `R5 = DM(0, I4)`: `e2` goes through memory (truncated to 24 bits)
    e2 = ld(st(m.sub(m.ONE, m.clip(m.mul(a_st, FOUR), m.ONE))))
    c2_reg = m.HALF if v == 8 else m.add(m.mul(m.clip(m.X(-1), e2), m.HALF), m.HALF)
    k = m.recip_refined(m.sub(m.ONE, c2_reg), c2_reg)
    return {
        "h": ld(st(h_reg)),
        "g1": ld(st(s3.fade(ah_st))),
        "g2": ld(st(s3.fade(a_st))),
        "q": ld(st(max(m.recip_refined(ah_st, m.HALF), TWO))),
        "q2": ld(st(max(m.recip_refined(a_st, m.HALF), TWO))),
        "c2": ld(st(c2_reg)),
        "k": ld(st(k)),
        "v": v,
        "a": a,
    }


def osc(phase: int, inc: int, s: dict):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    mult = m.fix(s["h"], 25)
    cc = m.sub(m.ONE, m.mul(m.HALF, s["k"]))
    out = []
    for x in ph:
        y = sy.ashift7(sy.uuf(x, mult))
        ta = s3.wave(abs(m.flt(y, -31)), m.HALF, s["q"])
        a_k = m.ld(m.st(m.add(m.mul(ta, s["g1"]), m.ONE)))
        xb = m.u32(x - m.u32(0xC0000000 + 2 * inc)) if s["v"] == 8 else x
        tb = s3.wave(abs(m.flt(xb, -31)), s["c2"], s["q2"])
        w = m.add(m.mul(tb, m.HALF), m.HALF)
        p = m.mul(a_k, w)
        if s["v"] == 8:
            out.append(m.sub(p, cc))
            continue
        z = m.flt(x ^ 0x80000000, -31)
        p = m.ld(m.st(p))
        out.append(m.add(m.mul(p, m.sub(m.mul(z, s["g2"]), m.ONE)), cc))
    return out, ph[3]


def stage_sync_v(lanes: list[m.Lane], v: int, words: list[int] | None = None):
    """`(out_bits[lane][4], phase1[lane], phase2[lane])` of variant `v`."""
    words = words or sy.table()
    sts = [setup(ln, words, v) for ln in lanes]
    incs = []
    for ln in lanes:
        f = m.ld(ln.pitch)
        incs.append((m.fix(m.mul(f, m.ld(ln.ratio1)), 31), m.fix(m.mul(f, m.ld(ln.ratio2)), 31)))
    o1 = [osc(ln.phase1, c[0], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    scales = []
    for s, ln in zip(sts, lanes, strict=True):
        g = m.mul(s["g2"], -m.HALF) if v == 6 else s["g2"]
        scales.append(m.mul(g, m.ld(ln.out_gain)))
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    return mixed, [o[1] for o in o1], [o[1] for o in o2]


def stage_sync6(lanes: list[m.Lane], words: list[int] | None = None):
    """Stage `0x1239BE`."""
    return stage_sync_v(lanes, 6, words)


def stage_sync8(lanes: list[m.Lane], words: list[int] | None = None):
    """Stage `0x123FE9`."""
    return stage_sync_v(lanes, 8, words)
