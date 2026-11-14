#!/usr/bin/env python3
"""Bit-exact model of the unison oscillator stage `0x121D6D` (ANALOG SUPER wave 0) and friends.

Listing `0x243ADA..0x243D38` (ORG core 2). Eight detuned saw voices per lane, mixed with eight
weight groups of the `P` block:

    u      = 0.35 (t + 2 t^4),  t = s5 + 0.06            s5 = DM(-5, I2) (spread word)
    inc_j  = FIX(trunc((rho_j u + (1 - u)) f) 2^31)       rho_j = P[-32 + 2 j + lane], f = pitch
    voice  = phase_j + k inc_j, k = 1..4  (phase_j <- phase_j + 4 inc_j)
    s_k    = copysign(w_k^2, xo_k) - xo_k,  w_k = 1 - min(1, |xf_k r7|),  r7 = 0.5 / f
    acc_k += s_k(own) wown_j + s_k(other lane) wother_j   (accumulated through memory)
    out_k  = trunc(acc_k (P[16] g)),  g = C (B + clip(A - f, B))

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x121D6D`
(`docs/research/synth-stage-models.md`).
"""

from __future__ import annotations

import synth_wave_model as m

# VERIFIED (listing `0x243B48..0x243B68`): spread law constants (0.06, 0.35).
SPREAD_OFFSET = m.ld(0x3D75C28F)
SPREAD_SCALE = m.ld(0x3EB33333)
VOICES = 8


def saw_diff(phases: list[int], r7: m.X) -> list[m.X]:
    """The inline saw of the unison loop (negated sign, no gain): four registers `s_k`."""
    out = []
    for x in phases:
        xf = m.flt(x, -31)
        xo = m.flt(m.u32(x) ^ 0x80000000, -31)
        w = m.sub(m.ONE, m.clip(m.ONE, m.mul(xf, r7)))
        w2 = m.mul(w, w)
        out.append(m.sub(-w2 if xo < 0 else w2, xo))
    return out


def spread_factor(shape: int) -> tuple[m.X, m.X]:
    """`(u, 1 - u)` registers of the detune law."""
    t = m.add(m.ld(shape), SPREAD_OFFSET)
    q = m.mul(t, t)
    q4 = m.mul(q, q)
    u = m.mul(m.add(m.add(t, q4), q4), SPREAD_SCALE)
    return u, m.sub(m.ONE, u)


def lane_unison(lane: m.Lane):
    """Per lane: `(fade g, r7, [voice increments])`."""
    f = m.ld(lane.pitch)
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, f), m.B_FADE), m.B_FADE))))
    r7 = m.ld(m.st(m.recip_refined(f, m.HALF)))
    u, om = spread_factor(lane.spread)
    incs = []
    for rho in lane.ratios:
        tbl = m.ld(m.st(m.mul(m.add(m.mul(m.ld(rho), u), om), f)))
        incs.append(m.fix(tbl, 31))
    return g, r7, incs


def mix_voice(s: list[list[m.X]], lanes: list[m.Lane], j: int, acc: list[list[m.X]]):
    """One voice of the mix: updates `acc[lane][k]` through the 32-bit memory words."""
    for i in (0, 1):
        grp = lanes[0].groups[j]
        w_own = m.ld(grp[i])
        w_other = m.ld(grp[3 - i])
        for k in range(4):
            mixed = m.add(m.mul(s[i][k], w_own), m.mul(s[1 - i][k], w_other))
            acc[i][k] = m.add(mixed, m.ld(m.st(acc[i][k])))


def stage_super(lanes: list[m.Lane]):
    """Stage `0x121D6D`, both lanes: `(out_bits[lane][4], voice phases[lane], 0)`."""
    info = [lane_unison(ln) for ln in lanes]
    acc = [[m.X(0)] * 4 for _ in lanes]
    phases = [list(ln.phases) for ln in lanes]
    for j in range(VOICES):
        s = []
        for i in (0, 1):
            inc = info[i][2][j]
            ph = [m.u32(phases[i][j] + k * inc) for k in (1, 2, 3, 4)]
            phases[i][j] = ph[3]
            s.append(saw_diff(ph, info[i][1]))
        mix_voice(s, lanes, j, acc)
    out = []
    for i, ln in enumerate(lanes):
        scale = m.mul(m.ld(ln.out_gain), info[i][0])
        out.append([m.st(m.mul(a, scale)) for a in acc[i]])
    return out, [p[7] for p in phases], [0, 0]


# VERIFIED (image words `DM(0xC1596..0xC1598)` = `DM(-32..-30, I1)` of channel 0, read by the
# listing `0x2438CA..0x2438F2`): the idle octave multipliers of the three MULTI voices.
MULTI_OCTAVES_ADDR = 0xC1596
MULTI_VOICES = 3


def multi_increments(lane: m.Lane) -> list[int]:
    """The three voice increment words (stored floats `a_v`, listing `0x2438A8..0x2438FC`)."""
    f = m.ld(lane.pitch)
    s5 = m.ld(lane.spread)
    om = m.sub(m.ONE, s5)
    base = m.mul(f, m.ld(lane.ratios[0]))
    out = [m.st(m.mul(m.ld(lane.octaves[0]), base))]
    for v in (1, 2):
        d = m.add(m.mul(m.ld(lane.ratios[v]), s5), om)
        out.append(m.st(m.mul(m.mul(d, m.ld(lane.octaves[v])), base)))
    return out


def stage_multi(lanes: list[m.Lane]):
    """Stage `0x121C54` (ANALOG MULTI, six waves): three octave voices, each with its own fade
    folded into the weights: `(out_bits[lane][4], voice phases[lane], 0)`."""
    words = [multi_increments(ln) for ln in lanes]
    acc = [[m.X(0)] * 4 for _ in lanes]
    phases = [list(ln.phases) for ln in lanes]
    for v in range(MULTI_VOICES):
        s, gs = [], []
        for i in (0, 1):
            a = m.ld(words[i][v])
            gs.append(
                m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, a), m.B_FADE), m.B_FADE))))
            )
            r7 = m.recip_refined(a, m.HALF)
            ph = [m.u32(phases[i][v] + k * m.fix(a, 31)) for k in (1, 2, 3, 4)]
            phases[i][v] = ph[3]
            s.append(saw_diff(ph, r7))
        for i in (0, 1):
            grp = lanes[0].groups[v]
            w_own = m.mul(gs[i], m.ld(grp[i]))
            w_other = m.mul(gs[i], m.ld(grp[3 - i]))
            for k in range(4):
                mixed = m.add(m.mul(s[i][k], w_own), m.mul(s[1 - i][k], w_other))
                acc[i][k] = m.add(mixed, m.ld(m.st(acc[i][k])))
    out = []
    for i, ln in enumerate(lanes):
        gain = m.ld(ln.out_gain)
        out.append([m.st(m.mul(m.ld(m.st(a)), gain)) for a in acc[i]])
    return out, [p[2] for p in phases], [0, 0]
