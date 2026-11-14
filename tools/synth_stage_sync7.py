#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SYNC wave 7 stage `0x123C7C` (ORG core 2, 40-bit registers).

Listing `0x2478F8..0x247FD0`, read word by word (`docs/research/synth-stage-models-7.md`). The
disassembler prints the constant loads `R14 = 25` etc. as `DM(-12, I0)`: `I0 = 0xC1E02` points at
the constant table `0xC1DE2..` (pairs of identical words: 0.125, 0.25, 0.375, 0.5, 0.75, 0.99,
0, 0x7EB851EB, 0xC0000000, 0x80000000, 25, 31, -31, -15, -35, 1, 2, 4, -1, 1.5708), so every
`{..}` is a constant. Frame of `0x1239BE` (`synth_stage_sync6.py`): two oscillator passes, the
common mix, scale `-0.5 g2 P[16]`, no high pass. With `a = inc1 2^-31`, `h = 2 interp(s5)`,
`x_k = phase + k inc`, `wave(|y|, c, q)` as in `0x122C5C`:

    ah = a h;  g1 = fade(ah), q = max(0.5 / ah, 2);  g3 = fade(ah / 4), q3 = max(2 / ah, 2)
    A_k  = wave(|ASHIFT(UUF(x, FIX(h, 25)), 7)|, 0.5, q) g1 + 1                (stored)
    P_k  = A_k (wave(|ASHIFT(UUF(x, FIX(h / 4, 25)), 7) + 2^30|, 0.5, q3) g3 + 1)   (stored)
    Q_k  = P_k (0.25 wave(|x|, c2, q2) + 0.25)                                 (stored)
    out  = Q_k (g2 FLOAT(x ^ 2^31) - 1) + (1 - c2 / (2 (1 - c2)))
"""

from __future__ import annotations

import synth_stage_sync as sy
import synth_stage_sync3 as s3
import synth_stage_sync6 as s6
import synth_wave_model as m

QUARTER = m.X(1, 4)


def setup(ln: m.Lane, words: list[int]) -> dict:
    """The stored lane constants (every `DM(.., I4)` word reloaded as a float)."""
    ld, st = m.ld, m.st
    f = ld(ln.pitch)
    inc1 = m.fix(m.mul(f, ld(ln.ratio1)), 31)
    a = m.flt(inc1, -31)
    a_st = ld(st(a))
    h_reg = sy.lookup(ln.spread, words)
    h_reg = m.add(h_reg, h_reg)
    ah = m.mul(a, h_reg)
    ah_st = ld(st(ah))
    ah4_st = ld(st(m.mul(QUARTER, ah)))
    e2 = ld(st(m.sub(m.ONE, m.clip(m.mul(a_st, s6.FOUR), m.ONE))))
    c2_reg = m.add(m.mul(m.clip(m.X(-1), e2), m.HALF), m.HALF)
    k2 = m.recip_refined(m.add(m.sub(m.ONE, c2_reg), m.sub(m.ONE, c2_reg)), c2_reg)
    return {
        "h": ld(st(h_reg)),
        "h4": ld(st(m.mul(QUARTER, h_reg))),
        "g1": ld(st(s3.fade(ah_st))),
        "g2": ld(st(s3.fade(a_st))),
        "g3": ld(st(s3.fade(ah4_st))),
        "q": ld(st(max(m.recip_refined(ah_st, m.HALF), s6.TWO))),
        "q2": ld(st(max(m.recip_refined(a_st, m.HALF), s6.TWO))),
        "q3": ld(st(max(m.recip_refined(ah4_st, m.HALF), s6.TWO))),
        "c2": ld(st(c2_reg)),
        "cc": ld(st(m.sub(m.ONE, k2))),
    }


def osc(phase: int, inc: int, s: dict):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    mult1, mult2 = m.fix(s["h"], 25), m.fix(s["h4"], 25)
    out = []
    for x in ph:
        ta = s3.wave(abs(m.flt(sy.ashift7(sy.uuf(x, mult1)), -31)), m.HALF, s["q"])
        a_k = m.ld(m.st(m.add(m.mul(ta, s["g1"]), m.ONE)))
        y2 = m.u32(sy.ashift7(sy.uuf(x, mult2)) - 0xC0000000)
        tb = s3.wave(abs(m.flt(y2, -31)), m.HALF, s["q3"])
        p = m.ld(m.st(m.mul(m.add(m.mul(tb, s["g3"]), m.ONE), a_k)))
        tc = s3.wave(abs(m.flt(x, -31)), s["c2"], s["q2"])
        q = m.ld(m.st(m.mul(p, m.add(m.mul(tc, QUARTER), QUARTER))))
        z = m.flt(x ^ 0x80000000, -31)
        out.append(m.add(m.mul(m.sub(m.mul(z, s["g2"]), m.ONE), q), s["cc"]))
    return out, ph[3]


def stage_sync7(lanes: list[m.Lane], words: list[int] | None = None):
    """Stage `0x123C7C`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    words = words or sy.table()
    sts = [setup(ln, words) for ln in lanes]
    incs = []
    for ln in lanes:
        f = m.ld(ln.pitch)
        incs.append((m.fix(m.mul(f, m.ld(ln.ratio1)), 31), m.fix(m.mul(f, m.ld(ln.ratio2)), 31)))
    o1 = [osc(ln.phase1, c[0], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    scales = [
        m.mul(m.mul(s["g2"], -m.HALF), m.ld(ln.out_gain)) for s, ln in zip(sts, lanes, strict=True)
    ]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    return mixed, [o[1] for o in o1], [o[1] for o in o2]
