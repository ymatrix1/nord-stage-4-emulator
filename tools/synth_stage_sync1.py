#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SYNC wave 1 stage `0x1226DD` (ORG core 2, 40-bit registers).

Listing `0x244DBA..0x245122`, read word by word (`docs/research/synth-stage-models-7.md`). Two
oscillator passes, the common mix, the one-pole output high pass of `0x122C5C`. Unlike the other
SYNC stages it calls the helpers of the saw family: the parabola helper `0x1251D4`
(`synth_wave_model.helper`) twice on the sync phase and the sine helper `0x12522E` once. With `f` =
pitch word, `h = interp(s5)` (stored), `x_k = phase + k inc`,
`y_k = ASHIFT(SSFR(x_k, FIX(h, 25)), 6)`:

    fh = h f (stored);  r = 0.5 / fh (stored, no floor);  g = fade(h f)  (stored)
    S1  = helper(y, fh, r)  (stored);  S2 = helper(y - 2^31, 4/3 fh, r)  (R6 grows by 4/3)
    D   = S1 - S2  (stored)
    out = D (0.5 - 0.5 sine(x));   scale g P[16], then the output high pass
"""

from __future__ import annotations

import synth_stage_shape_sine as ss
import synth_stage_shape_sine2 as hp
import synth_stage_sync as sy
import synth_stage_sync3 as s3
import synth_wave_model as m


def ssfr(a: int, b: int) -> int:
    """`Rn = Rx * Ry (SSFR)`: signed fractional product, upper word, rounded to nearest with ties
    to even (VERIFIED by the oracle: the ratio `h = 16` at `s5 = 1` makes every fourth product a
    tie, and round-half-up misses 4 of 320 words)."""
    p = m.s32(a) * m.s32(b) * 2
    q, rem = p >> 32, p & 0xFFFFFFFF
    if rem > 1 << 31 or (rem == 1 << 31 and q & 1):
        q += 1
    return m.u32(q)


def ashift6(x: int) -> int:
    return m.u32(x << 6)


def setup(ln: m.Lane, words: list[int]) -> dict:
    """The stored lane constants."""
    ld, st = m.ld, m.st
    f = ld(ln.pitch)
    h_reg = sy.lookup(ln.spread, words)
    fh = m.mul(h_reg, ld(ln.pitch))
    fh_st = ld(st(fh))
    return {
        "h": ld(st(h_reg)),
        "fh": fh_st,
        "r": ld(st(m.recip_refined(fh_st, m.HALF))),
        "g": ld(st(s3.fade(fh))),
        "f": f,
    }


def osc(phase: int, inc: int, s: dict):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    mult = m.fix(s["h"], 25)
    y = [ashift6(ssfr(x, mult)) for x in ph]
    first, r6 = m.helper(y, s["fh"], s["r"])
    first = [m.ld(m.st(v)) for v in first]
    second, _ = m.helper([m.u32(v - 0x80000000) for v in y], r6, s["r"])
    out = []
    for x, a, b in zip(ph, first, second, strict=True):
        d = m.ld(m.st(m.sub(a, b)))
        sine = ss.sine_of_word(x)
        out.append(m.mul(m.sub(m.mul(sine, -m.HALF), -m.HALF), d))
    return out, ph[3]


def stage_sync1(lanes: list[m.Lane], words: list[int] | None = None, high_pass: bool = True):
    """Stage `0x1226DD`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    words = words or sy.table()
    sts = [setup(ln, words) for ln in lanes]
    incs = []
    for ln in lanes:
        f = m.ld(ln.pitch)
        incs.append((m.fix(m.mul(f, m.ld(ln.ratio1)), 31), m.fix(m.mul(f, m.ld(ln.ratio2)), 31)))
    o1 = [osc(ln.phase1, c[0], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s) for ln, c, s in zip(lanes, incs, sts, strict=True)]
    scales = [m.mul(s["g"], m.ld(ln.out_gain)) for s, ln in zip(sts, lanes, strict=True)]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    if not high_pass:
        return mixed, [o[1] for o in o1], [o[1] for o in o2]
    out = [hp.output_high_pass(w, ln.hp or hp.HP_IDLE) for w, ln in zip(mixed, lanes, strict=True)]
    return out, [o[1] for o in o1], [o[1] for o in o2]
