#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SYNC wave 3..5 stage `0x122C5C` (ORG core 2, 40-bit registers).

Listing `0x2458B8..0x245E3C` (read word by word, `docs/research/synth-stage-models-6.md`): not the
code of `0x122A32`. Two oscillator passes (`LCNTR = 2`, slots swapped at the end of the loop), the
common mix, a scale and the one-pole output high pass of SHAPE SINE 4. With `f` = pitch word,
`a = inc1 2^-31` (oscillator 1 only), `h` = table interpolation at `s5` (as `0x122A32`), `p` = pulse
word `DM(-8, I1)` as a signed fraction, `x_k = phase + k inc` (k = 1..4):

    e   = 1 - clip(4 a h, 1);  r = 2 (p - 0.5);  w1 = clip(r, e)        (stored)
    c1  = 0.5 + w1 / 2   (stored);  q = max(0.5 / (a h), 2)   (stored);  rt = (1 - |r|) / (1 - |w1|)
    e2  = 1 - clip(4 a, 1);  c2 = 0.5 - e2 / 2   (stored);  q2 = max(0.5 / a, 2)   (stored)
    g1  = C (B + clip(A - a h, B));  g2 = C (B + clip(A - a, B))        (stored)
    tA  = sg - copysign(u^2, sg),  sg = sgn(d), u = 1 - min(1, |d q|), d = |y_k| - c1
    y_k = ASHIFT(UUF(x_k, FIX(h, 25)), 7) + pulse
    tB  = the same on d = |x_k| - c2 with q2
    out = (0.5 tB + 0.5) (tA - 1) + 1 + w1

The mix is the common `mix_lanes` with the scale `g2 P[16] g1 rt`, then the output high pass.
"""

from __future__ import annotations

import synth_stage_shape_sine2 as hp
import synth_stage_sync as sy
import synth_wave_model as m

TWO = m.X(2)
FOUR = m.X(4)


def fade(x: m.X) -> m.X:
    """`C (B + clip(A - x, B))`, the saw fade of the pitch-like quantity `x`."""
    return m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, x), m.B_FADE), m.B_FADE))


def setup(ln: m.Lane, words: list[int]) -> dict:
    """The stored lane constants (every `DM(.., I4)` word reloaded as a float)."""
    ld, st = m.ld, m.st
    f = ld(ln.pitch)
    inc1 = m.fix(m.mul(f, ld(ln.ratio1)), 31)
    a = m.flt(inc1, -31)
    a_st = ld(st(a))
    h_reg = sy.lookup(ln.spread, words)  # the product `a h` reads the register, not the stored word
    h = ld(st(h_reg))
    ah = m.mul(a, h_reg)
    ah_st = ld(st(ah))
    e = m.sub(m.ONE, m.clip(m.mul(ah, FOUR), m.ONE))
    p = m.flt(ln.pulse, -31)
    r = m.sub(p, m.HALF)
    r = m.add(r, r)
    w1 = m.clip(r, e)
    c1 = ld(st(m.add(m.mul(w1, m.HALF), m.HALF)))
    ratio = m.recip_refined(m.sub(m.ONE, abs(w1)), m.sub(m.ONE, abs(r)))
    q = ld(st(max(m.recip_refined(ah_st, m.HALF), TWO)))
    e2 = m.sub(m.ONE, m.clip(m.mul(a_st, FOUR), m.ONE))
    c2 = ld(st(m.add(m.mul(m.clip(m.X(-1), e2), m.HALF), m.HALF)))
    q2 = ld(st(max(m.recip_refined(a_st, m.HALF), TWO)))
    return {
        "h": h,
        "w1": ld(st(w1)),
        "c1": c1,
        "c2": c2,
        "q": q,
        "q2": q2,
        "ratio": ld(st(ratio)),
        "g1": ld(st(fade(ah_st))),
        "g2": ld(st(fade(a_st))),
        "pulse": ln.pulse,
    }


def wave(absx: m.X, c: m.X, q: m.X) -> m.X:
    """`sg - copysign(u^2, sg)` with `sg = sgn(d)`, `u = 1 - min(1, |d q|)`, `d = |x| - c`.

    Listing `0x245B5E..0x245BD4`: the transfer `R8 = R12` reads the sign register (`R12` holds
    `copysign(1, d)` after `F12 = F11 COPYSIGN F12`), not `d`: the run of seed 2 showed 1.0.
    """
    d = m.sub(absx, c)
    u = m.sub(m.ONE, m.clip(m.ONE, m.mul(d, q)))
    u2 = m.mul(u, u)
    sg = m.X(-1) if d < 0 else m.X(1)
    return m.sub(sg, -u2 if d < 0 else u2)


def osc(phase: int, inc: int, s: dict):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    mult = m.fix(s["h"], 25)
    out = []
    for x in ph:
        y = m.u32(sy.ashift7(sy.uuf(x, mult)) + s["pulse"])
        ta = m.ld(m.st(wave(abs(m.flt(y, -31)), s["c1"], s["q"])))
        tb = wave(abs(m.flt(x, -31)), s["c2"], s["q2"])
        v = m.add(m.mul(tb, m.HALF), m.HALF)
        z = m.add(m.mul(v, m.sub(ta, m.ONE)), m.ONE)
        out.append(m.add(z, s["w1"]))
    return out, ph[3]


def stage_sync3(lanes: list[m.Lane], words: list[int] | None = None, high_pass: bool = True):
    """Stage `0x122C5C`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    words = words or sy.table()
    sts = [setup(ln, words) for ln in lanes]
    incs = []
    for ln in lanes:
        f = m.ld(ln.pitch)
        incs.append((m.fix(m.mul(f, m.ld(ln.ratio1)), 31), m.fix(m.mul(f, m.ld(ln.ratio2)), 31)))
    o1 = [osc(ln.phase1, c[0], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    scales = [
        m.mul(m.mul(m.mul(s["g2"], m.ld(ln.out_gain)), s["g1"]), s["ratio"])
        for s, ln in zip(sts, lanes, strict=True)
    ]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    if not high_pass:
        return mixed, [o[1] for o in o1], [o[1] for o in o2]
    out = [hp.output_high_pass(w, ln.hp or hp.HP_IDLE) for w, ln in zip(mixed, lanes, strict=True)]
    return out, [o[1] for o in o1], [o[1] for o in o2]
