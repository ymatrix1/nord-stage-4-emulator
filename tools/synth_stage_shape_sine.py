#!/usr/bin/env python3
"""Bit-exact models of the ANALOG SHAPE SINE stages (ORG core 2, 40-bit registers).

* `0x123427` SHAPE SINE wave 0 (listing `0x24684E..0x246B90`): a phase-distorted sine. The phase
  `x = ph 2^-31` (-1..1) is warped by two linear slopes around the knee `-w`,

      y = (x + 1) / (1 - w)   if that is below 1,   else (x - 1) / (1 + w)

  with the width `w = (1 - (1 - s5)^2) (1 - min(1, 5.5 a))` (`s5 = DM(-5, I2)`, `a = inc 2^-31`),
  and the warped phase word `2 FIX(y 2^30)` is fed to the sine polynomial of the helper `0x12522E`
  (`t (c12 + q (c14 + q (c16 + q c18)))`, `t = |x| - 1`). The two oscillators and lanes are mixed
  by `mix_lanes`; the scale is `g P[16]` with the sine fade `g` of the increment.

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x123427`
(`docs/research/synth-stage-models-2.md`).
"""

from __future__ import annotations

import synth_wave_model as m

WIDTH_FADE = m.ld(0x40B00000)  # 5.5: the width closes as `1 - min(1, 5.5 a)` (VERIFIED, listing)


def sine_of_word(word: int) -> m.X:
    """The sine polynomial on the warped phase word (`x = word 2^-30`, no offset subtraction)."""
    t = m.sub(abs(m.flt(word, -30)), m.ONE)
    q = m.mul(t, t)
    h = m.add(m.mul(m.SINE_C18, q), m.SINE_C16)
    h = m.add(m.mul(h, q), m.SINE_C14)
    h = m.add(m.mul(h, q), m.SINE_C12)
    return m.mul(h, t)


def width_setup(inc1: int, spread: int):
    """`(a stored, width w, 1/(1 - w), 1/(1 + w))` as the registers of the listing."""
    a_mem = m.ld(m.st(m.flt(inc1, -31)))
    s = m.sub(m.ONE, m.ld(spread))
    s = m.sub(m.ONE, m.mul(s, s))
    room = m.sub(m.ONE, m.clip(m.mul(a_mem, WIDTH_FADE), m.ONE))
    w = m.mul(s, room)
    inv_m = m.ld(m.st(m.recip_refined(m.sub(m.ONE, w), m.ONE)))
    inv_p = m.ld(m.st(m.recip_refined(m.add(m.ONE, w), m.ONE)))
    return w, inv_m, inv_p


def warp_word(ph: int, inv_m: m.X, inv_p: m.X) -> int:
    """Warped phase word of one phase word (wave 0 law)."""
    x = m.flt(ph, -31)
    hi = m.mul(m.add(x, m.ONE), inv_m)
    y = m.mul(m.sub(x, m.ONE), inv_p) if hi >= m.ONE else hi
    return m.u32(m.s32(m.fix(y, 30)) * 2)


def shape_sine_osc(phase: int, inc: int, inv_m: m.X, inv_p: m.X, warp=warp_word):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    return [sine_of_word(warp(p, inv_m, inv_p)) for p in ph], ph[3]


def stage_shape_sine(lanes: list[m.Lane]):
    """Stage `0x123427`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, setups, gs = [], [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        incs.append(inc)
        a = m.flt(inc[0], -31)
        gs.append(
            m.ld(
                m.st(
                    m.mul(
                        m.SINE_FADE_C,
                        m.add(m.clip(m.sub(m.SINE_FADE_A, a), m.SINE_FADE_B), m.SINE_FADE_B),
                    )
                )
            )
        )
        setups.append(width_setup(inc[0], ln.spread))
    o1 = [
        shape_sine_osc(ln.phase1, c[0], s_[1], s_[2])
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    o2 = [
        shape_sine_osc(ln.phase2, c[1], s_[1], s_[2])
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    scales = [m.mul(g, m.ld(ln.out_gain)) for g, ln in zip(gs, lanes, strict=True)]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


QUARTER = 0x40000000  # +90 degrees in phase words (`R5 = 0x40000000` of the listing)
K_TOP = m.ld(0x3F7CAC08)  # VERIFIED (listing `0x245EAE`): the resonance falls as `K_TOP - 4 a`
K_FOUR = m.ld(0x40800000)
K_DAMP = m.ld(0x3F59999A)  # 0.85, the output level falls as `1 - 0.85 k`


def resonance_setup(inc1: int, spread: int):
    """`(k, 2k, 1 + k^2, 1 - 0.85 k)` as the stored words of the listing `0x245EA6..0x245EF6`."""
    a = m.flt(inc1, -31)
    f4 = m.sub(K_TOP, m.mul(a, K_FOUR))
    if f4 < 0:  # `IF LT R4 = M0`, M0 = 0
        f4 = m.X(0)
    k_reg = m.mul(f4, m.ld(spread))
    k = m.ld(m.st(k_reg))
    c16 = m.ld(m.st(m.sub(m.ONE, m.mul(k_reg, K_DAMP))))  # uses the register, not the stored k
    c18 = m.ld(m.st(m.add(m.mul(k, k), m.ONE)))
    two_k = m.ld(m.st(m.add(k, k)))
    return two_k, c18, c16


def resonant_osc(phase: int, inc: int, two_k: m.X, c18: m.X):
    """`(four samples, new phase)`: `cos / (1 + k^2 + 2 k sin)` of the phase."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    out = []
    for p in ph:
        c = m.ld(m.st(sine_of_word(m.u32(p + QUARTER))))
        s = sine_of_word(p)
        out.append(m.recip_refined(m.add(c18, m.mul(s, two_k)), c))
    return out, ph[3]


def stage_shape_sine_5(lanes: list[m.Lane]):
    """Stage `0x122F1F` (SHAPE SINE wave 5): `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, setups, gs = [], [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        incs.append(inc)
        a = m.flt(inc[0], -31)
        fade = m.add(m.clip(m.sub(m.SINE_FADE_A, a), m.SINE_FADE_B), m.SINE_FADE_B)
        gs.append(m.ld(m.st(m.mul(m.SINE_FADE_C, fade))))
        setups.append(resonance_setup(inc[0], ln.spread))
    o1 = [
        resonant_osc(ln.phase1, c[0], s_[0], s_[1])
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    o2 = [
        resonant_osc(ln.phase2, c[1], s_[0], s_[1])
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    scales = [
        m.mul(m.mul(g, s_[2]), m.ld(ln.out_gain))
        for g, s_, ln in zip(gs, setups, lanes, strict=True)
    ]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


GAIN_MAX = m.ld(0x41700000)  # 15.0: the gain ceiling grows as `1 + 15 s5^2`
GAIN_NUM = m.ld(0x3E3A2E7F)  # numerator of the pitch law `0.1818 / a`


def gain_setup(inc1: int, spread: int):
    """`(rho stored, 1 / rho stored)` of the listing `0x2464DA..0x246632` (SHAPE SINE wave 2)."""
    s5 = m.ld(spread)
    ceiling = m.ld(m.st(m.add(m.mul(m.mul(s5, s5), GAIN_MAX), m.ONE)))
    a = m.flt(inc1, -31)
    rho = min(max(m.recip_refined(a, GAIN_NUM), m.ONE), ceiling)
    return m.ld(m.st(rho)), m.ld(m.st(m.recip_refined(rho, m.ONE)))


def clipped_gain_osc(phase: int, inc: int, rho: m.X, inv: m.X):
    """`(four samples, new phase)`: sine of the clipped, amplified phase minus `1 - 1/rho`."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    out = []
    offset = m.sub(m.ONE, inv)
    for p in ph:
        y = m.clip(m.mul(m.flt(p, -31), rho), m.ONE)
        word = m.u32(m.s32(m.fix(y, 30)) * 2)
        out.append(m.sub(sine_of_word(word), offset))
    return out, ph[3]


def stage_shape_sine_2(lanes: list[m.Lane]):
    """Stage `0x1232A5` (SHAPE SINE wave 2): `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, setups, gs = [], [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        incs.append(inc)
        a = m.flt(inc[0], -31)
        fade = m.add(m.clip(m.sub(m.SINE_FADE_A, a), m.SINE_FADE_B), m.SINE_FADE_B)
        gs.append(m.ld(m.st(m.mul(m.SINE_FADE_C, fade))))
        setups.append(gain_setup(inc[0], ln.spread))
    o1 = [
        clipped_gain_osc(ln.phase1, c[0], *s_)
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    o2 = [
        clipped_gain_osc(ln.phase2, c[1], *s_)
        for ln, c, s_ in zip(lanes, incs, setups, strict=True)
    ]
    scales = [m.mul(-g, m.ld(ln.out_gain)) for g, ln in zip(gs, lanes, strict=True)]  # `F0 = -F0`
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


def folded_osc(phase: int, inc: int, rho: m.X):
    """`(four samples, new phase)` of SHAPE SINE wave 1 (`0x1235C9`): the half-cycle magnitude
    `x = (ph & 0x7FFFFFFF) 2^-31` is stretched by `rho = 1 / (1 - w)` and clipped at 1, and the
    sign half is restored by subtracting `2^31` from the warped word."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    out = []
    for p in ph:
        x = m.flt(p & 0x7FFFFFFF, -31)
        y = m.clip(m.mul(x, rho), m.ONE)
        word = m.u32(m.fix(y, 30) << 1)
        if p >> 31:
            word = m.u32(word - 0x80000000)
        out.append(sine_of_word(word))
    return out, ph[3]


def stage_shape_sine_1(lanes: list[m.Lane]):
    """Stage `0x1235C9` (SHAPE SINE wave 1): `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, setups, gs = [], [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        incs.append(inc)
        a = m.flt(inc[0], -31)
        fade = m.add(m.clip(m.sub(m.SINE_FADE_A, a), m.SINE_FADE_B), m.SINE_FADE_B)
        gs.append(m.ld(m.st(m.mul(m.SINE_FADE_C, fade))))
        setups.append(width_setup(inc[0], ln.spread)[1])
    o1 = [folded_osc(ln.phase1, c[0], r) for ln, c, r in zip(lanes, incs, setups, strict=True)]
    o2 = [folded_osc(ln.phase2, c[1], r) for ln, c, r in zip(lanes, incs, setups, strict=True)]
    scales = [m.mul(g, m.ld(ln.out_gain)) for g, ln in zip(gs, lanes, strict=True)]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]
