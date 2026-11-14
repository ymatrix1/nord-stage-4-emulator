#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SHAPE SINE wave 3 stage `0x12377D` (ORG core 2, 40-bit registers).

Listing `0x246EFA..0x24737A` (read word by word, `docs/research/synth-stage-models-5.md`), two
oscillator passes (`LCNTR = 2`, the second one on `inc2` / `phase2`). With `f` = pitch word,
`s5 = DM(-5, I2)` and `x_k = phase + k inc` (k = 1..4), per lane:

    g      = C (B + clip(A - f, B))                       (stored)
    v      = 1 - (1 - s5)^2,  room = 1 - min(1, 5.5 f),  vr = v room       (`DM(-4, I4)`)
    rA, rB = 1 / (1 - vr), 1 / (1 + vr)                   (stored, two Newton steps)
    P0     = pi (rA - rB) (stored),  K = (P0 2/3) f,  r7 = 0.5 / f (stored),  c = (2/pi) vr (stored)
    y_k    = (a+1) rA  if that is < 1 else (a-1) rB,      a = x_k 2^-31   (wave 0 warp)
    word_k = 2 FIX(y_k, 30) + 0x40000000,  z_k = sine(word_k) + c   (stored, sine helper `0x12522E`)
    cube(x) = (1 - min(1, |x 2^-31| r7))^3 K
    out_k  = cube(x_k + 0x80000000) + z_k  (stored),  then  + cube(x_k + FIX(vr, 31))

The mix is the common `mix_lanes` with the scale `g P[16]`; there is no output high pass.
"""

from __future__ import annotations

import synth_stage_shape_sine as ss
import synth_wave_model as m

TWO_OVER_PI = m.ld(0x3F22F983)
PI = m.ld(0x40490FDB)  # VERIFIED (listing 0x246FE0): note 0x40490FDB here, 0x40490FD0 in wave 4
TWO_THIRDS = m.ld(0x3F2AAA3B)


def setup(pitch_word: int, spread: int):
    """`(g, rA, rB, P0, K, r7, c, vr, shift)` of one lane, all as the stored words."""
    f = m.ld(pitch_word)
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, f), m.B_FADE), m.B_FADE))))
    s = m.sub(m.ONE, m.ld(spread))
    v = m.sub(m.ONE, m.mul(s, s))
    room = m.sub(m.ONE, m.clip(m.mul(f, ss.WIDTH_FADE), m.ONE))
    vr = m.mul(v, room)
    c = m.ld(m.st(m.mul(TWO_OVER_PI, vr)))
    vr_mem = m.ld(m.st(vr))
    ra = m.recip_refined(m.sub(m.ONE, vr), m.ONE)
    rb = m.recip_refined(m.add(m.ONE, vr), m.ONE)
    p0 = m.ld(m.st(m.mul(m.sub(ra, rb), PI)))
    k = m.mul(m.mul(p0, TWO_THIRDS), f)
    r7 = m.ld(m.st(m.recip_refined(f, m.HALF)))
    shift = m.fix(vr_mem, 31)
    return g, m.ld(m.st(ra)), m.ld(m.st(rb)), k, r7, c, shift


def warp_y(a: m.X, ra: m.X, rb: m.X) -> m.X:
    """The wave 0 warp of one signed phase `a` (`COMP` / `IF GE` selection)."""
    hi = m.mul(m.add(a, m.ONE), ra)
    return m.mul(m.sub(a, m.ONE), rb) if hi >= m.ONE else hi


def cube(word: int, r7: m.X, k: m.X) -> m.X:
    """`(1 - min(1, |x r7|))^3 K` of a phase word."""
    w = m.sub(m.ONE, m.clip(m.ONE, m.mul(m.flt(word, -31), r7)))
    return m.mul(m.mul(m.mul(w, w), w), k)


def osc(phase: int, inc: int, st_: tuple):
    """`(four samples, new phase)` of one oscillator."""
    _, ra, rb, k, r7, c, shift = st_
    ph = [m.u32(phase + j * inc) for j in (1, 2, 3, 4)]
    out = []
    for x in ph:
        y = warp_y(m.flt(x, -31), ra, rb)
        word = m.u32(2 * m.s32(m.fix(y, 30)) + 0x40000000)
        z = m.ld(m.st(m.add(ss.sine_of_word(word), c)))
        t = m.ld(m.st(m.add(cube(m.u32(x - 0x80000000), r7, k), z)))
        out.append(m.add(cube(m.u32(x + shift), r7, k), t))
    return out, ph[3]


def stage_shape_sine_3(lanes: list[m.Lane]):
    """Stage `0x12377D`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    sts = [setup(ln.pitch, ln.spread) for ln in lanes]
    incs = []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        incs.append(
            (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        )
    o1 = [osc(ln.phase1, c[0], s_) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s_) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    scales = [m.mul(s_[0], m.ld(ln.out_gain)) for s_, ln in zip(sts, lanes, strict=True)]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    return mixed, [o[1] for o in o1], [o[1] for o in o2]
