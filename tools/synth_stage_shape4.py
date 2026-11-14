#!/usr/bin/env python3
"""Bit-exact model of the stage `0x12145E` (ANALOG SHAPE wave 4).

Listing `0x2428BC..0x242BF4` (ORG core 2, 197 instructions, SIMD): a sibling of the SHAPE wave 0
stage `0x1212D3` (`tools/synth_wave_model.py`): two oscillators per lane, each the sum of two calls
of the parabola helper `0x1251D4` (`m.helper`), the same mix and the same output scale. The
differences are the parameters derived from the frame (`s = DM(-5, I2)`, the shape word) and the
combination of the two helper calls:

    inc_k = FIX(f rho_k, 31)         a = inc_1 2^-31 (stored), g = C (B + clip(A - a, B))
    D     = 1 + (1 - s)^2 - s^2 / 2  (`F2 - F1` of the listing)
    inv_a = 0.5 / a  (two Newton steps, as `0x1212D3`)    inv_d = 2 / D
    off   = 2 FIX(s, 30)  (u32)      c0 = (2/3) (a^2 - 1)  (stored)
    osc   = helper(p + k inc) + helper(p + k inc - off) + c0     (k = 1..4)

(the shape 0 stage subtracts the second call and has no `c0`). Both oscillators of a lane use the
setup of `inc_1`; the second call restarts `R6` from `a` (reloaded from `DM(-6, I4)`). The mix of
the four oscillators is `m.mix_lanes`, the scale `P[16] g inv_d`.

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x12145E`
(`docs/research/synth-stage-models-11.md`).
"""

from __future__ import annotations

import synth_wave_model as m

# VERIFIED (image words, listing `0x2429B6`): 0.66666 for the bias `c0`.
BIAS_K = m.ld(0x3F2AAAAA)
TWO = m.X(2)


def shape4_setup(lane: m.Lane):
    """`(inc1, inc2, a, g, inv_a, inv_d, offset, c0)` of one lane (listing `0x2428BC..0x2429CA`)."""
    pitch = m.ld(lane.pitch)
    inc1 = m.fix(m.mul(pitch, m.ld(lane.ratio1)), 31)
    inc2 = m.fix(m.mul(pitch, m.ld(lane.ratio2)), 31)
    a_reg = m.flt(inc1, -31)
    a = m.ld(m.st(a_reg))
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, a_reg), m.B_FADE), m.B_FADE))))
    s = m.ld(lane.spread)
    t = m.sub(m.ONE, s)
    big = m.add(m.ONE, m.mul(t, t))
    h = m.mul(s, m.HALF)
    d = m.sub(big, m.add(m.mul(h, h), m.mul(h, h)))
    inv_a = m.ld(m.st(m.recip_refined(a_reg, m.HALF)))
    inv_d = m.ld(m.st(m.recip_refined(d, TWO)))
    offset = m.u32(m.s32(m.fix(s, 30)) * 2)
    c0 = m.ld(m.st(m.sub(m.mul(BIAS_K, m.mul(a, a)), BIAS_K)))
    return inc1, inc2, a, g, inv_a, inv_d, offset, c0


def oscillator4(phase: int, inc: int, a, inv_a, offset: int, c0):
    """One oscillator: `(four sample registers, new phase)`."""
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    first, _ = m.helper(ph, a, inv_a)
    second, _ = m.helper([m.u32(p - offset) for p in ph], a, inv_a)
    out = [m.add(m.add(m.ld(m.st(x)), y), c0) for x, y in zip(first, second, strict=True)]
    return out, ph[3]


def stage_shape4(lanes: list[m.Lane]):
    """Stage `0x12145E`, both lanes: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    sts = [shape4_setup(ln) for ln in lanes]
    o1 = [oscillator4(ln.phase1, s[0], s[2], s[4], s[6], s[7]) for ln, s in zip(lanes, sts)]
    o2 = [oscillator4(ln.phase2, s[1], s[2], s[4], s[6], s[7]) for ln, s in zip(lanes, sts)]
    scales = [m.mul(m.mul(m.ld(ln.out_gain), s[3]), s[5]) for ln, s in zip(lanes, sts)]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]
