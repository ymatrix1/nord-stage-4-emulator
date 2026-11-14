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
