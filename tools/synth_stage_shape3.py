#!/usr/bin/env python3
"""Bit-exact model of the stage `0x120F55` (ANALOG SHAPE wave 3).

Listing `0x241EAA..0x242304` (ORG core 2, 264 instructions, SIMD) and the cosine helper
`0x12522E` (`0x24A45C..0x24A4E8`). Per lane, two oscillators (the loop `LCNTR = 2` swaps the
increment and phase words `DM(-25 / -23)`, `DM(-21 / -19, I2)` between the passes) and the mix
of `m.mix_lanes`; the scale is `-2 g P[16]` (no `1 / D` term).

    inc_k = FIX(f rho_k, 31)        a = inc_1 2^-31 (register)       s = DM(-5, I2) (stored)
    g     = C (B + clip(A - f, B))         (the fade of the saw helper, on f)
    L     = 14 s^2 + 2             w = min(max(K0 / a, 2), L)      K0 = 0.1815 (two Newton steps)
    iw    = 1 / w                  c = (2 / pi) iw                  d = -(pi / 4) w
    ifp   = 0.5 / f                K6 = (d 0.66666) f

and per oscillator, with `x_k = clip(w (p + k inc) 2^-31, 1)` (`p + k inc` as a signed phase):

    h(x)  = u (c12 + q (c14 + q (c16 + c18 q))),   u = |x| - 1,  q = u^2      (helper `0x12522E`,
            a polynomial for `-cos(pi x / 2)`, coefficients are image words)
    A_k   = h(x_k) + c                                     (stored)
    bump(ps) = K6 (1 - min(1, |ps 2^-31 ifp|))^3           ps = p + k inc +- FIX(iw, 31)
    osc_k = bump(+) + A_k (stored),  then + bump(-)        (two sequential adds, stored in between)

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x120F55`
(`docs/research/synth-stage-models-11.md`).
"""

from __future__ import annotations

import synth_wave_model as m

# VERIFIED (image words `DM(0xC1E06 / 0xC1E0E .. 0xC1E14)` read by the helper at `DM(4 / 12 .. 18,
# I0)`, listing `0x24A47C..0x24A4D0`).
HELPER_K = m.ld(0xBF800000)
HELPER_C = tuple(m.ld(w) for w in (0x3FC90FAC, 0xBF25591E, 0x3DA2ACAA, 0xBB8DEBE6))  # c12..c18
# VERIFIED (listing immediates): K0 `0x241F20`, 14 and 2 `0x241F04..0x241F0C`, 2 / pi and
# -pi / 4 `0x241F86 / 0x241F92`, 0.66666 `0x2420B8`, scale -2 `0x2422CE`.
K0 = m.ld(0x3E3A2E7F)
TWO_OVER_PI = m.ld(0x3F22F983)
NEG_PI_4 = m.ld(0xBF490FDB)
BUMP_K = m.ld(0x3F2AAA3B)
TWO, FOURTEEN, NEG_TWO = m.X(2), m.X(14), m.X(-2)


def cos_helper(q: int) -> m.X:
    """Helper `0x12522E` for one lane value `q` (Q30 integer): the register result."""
    u = m.add(abs(m.flt(q, -30)), HELPER_K)
    q2 = m.mul(u, u)
    c12, c14, c16, c18 = HELPER_C
    t = m.add(m.mul(c18, q2), c16)
    t = m.add(m.mul(t, q2), c14)
    t = m.add(m.mul(t, q2), c12)
    return m.mul(t, u)


def shape3_setup(lane: m.Lane):
    """`(inc1, inc2, g, w, c, iw_fix, ifp, k6)` of one lane (listing `0x241EAA..0x241FCA`)."""
    f = m.ld(lane.pitch)
    inc1 = m.fix(m.mul(f, m.ld(lane.ratio1)), 31)
    inc2 = m.fix(m.mul(f, m.ld(lane.ratio2)), 31)
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, f), m.B_FADE), m.B_FADE))))
    s = m.ld(lane.spread)
    limit = m.ld(m.st(m.add(m.mul(m.mul(s, s), FOURTEEN), TWO)))
    a_reg = m.flt(inc1, -31)
    w_reg = min(max(m.recip_refined(a_reg, K0), TWO), limit)
    w = m.ld(m.st(w_reg))
    iw_reg = m.recip_refined(w_reg, m.ONE)
    iw = m.ld(m.st(iw_reg))
    c = m.ld(m.st(m.mul(iw_reg, TWO_OVER_PI)))
    d = m.ld(m.st(m.mul(w, NEG_PI_4)))  # `R0 = DM(20, I4)` is already the stored w
    ifp = m.ld(m.st(m.recip_refined(f, m.HALF)))
    k6 = m.mul(m.mul(d, BUMP_K), f)
    return inc1, inc2, g, w, c, m.fix(iw, 31), ifp, k6, f


def bump(phases: list[int], ifp: m.X, k6: m.X) -> list[m.X]:
    """`K6 (1 - min(1, |x ifp|))^3` for four signed phases."""
    out = []
    for x in phases:
        wf = m.sub(m.ONE, m.clip(m.ONE, m.mul(m.flt(x, -31), ifp)))
        out.append(m.mul(m.mul(m.mul(wf, wf), wf), k6))
    return out


def oscillator3(phase: int, inc: int, setup):
    """One oscillator: `(four sample registers, new phase)`."""
    _, _, _, w, c, iw_fix, ifp, k6, _ = setup
    ph = [m.u32(phase + k * inc) for k in (1, 2, 3, 4)]
    a = []
    for p in ph:
        x = m.clip(m.mul(m.flt(p, -31), w), m.ONE)
        a.append(m.ld(m.st(m.add(cos_helper(m.fix(x, 30)), c))))
    up = bump([m.u32(p + iw_fix) for p in ph], ifp, k6)
    t = [m.ld(m.st(m.add(b, x))) for b, x in zip(up, a, strict=True)]
    dn = bump([m.u32(p - iw_fix) for p in ph], ifp, k6)
    return [m.add(b, x) for b, x in zip(dn, t, strict=True)], ph[3]


def stage_shape3(lanes: list[m.Lane]):
    """Stage `0x120F55`, both lanes: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    sts = [shape3_setup(ln) for ln in lanes]
    o1 = [oscillator3(ln.phase1, s[0], s) for ln, s in zip(lanes, sts)]
    o2 = [oscillator3(ln.phase2, s[1], s) for ln, s in zip(lanes, sts)]
    scales = [m.mul(m.mul(m.ld(ln.out_gain), s[2]), NEG_TWO) for ln, s in zip(lanes, sts)]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]
