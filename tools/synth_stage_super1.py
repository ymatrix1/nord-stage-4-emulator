#!/usr/bin/env python3
"""Bit-exact model of the unison stage `0x121E9D` (ANALOG SUPER wave 1).

Listing `0x243D3A..0x243FAE` (ORG core 2): the seven-voice sibling of `0x121D6D`
(`tools/synth_stage_unison.py`). Same fade `g`, reciprocal `r7 = 0.5 / f`, detune law `u` and
voice increments; only the loop (`LCNTR = 7`, voices 0..6) and the per-sample shape differ. Per
voice and phase word `x_k = phase + k inc` (k = 1..4):

    a = |x_k 2^-31|,  d = a - 0.5,  w = 1 - min(1, |d r7|)
    sg = copysign(1, d),  s_k = sg - copysign(w^2, sg)

(the sign of `d` comes from `R12 = R0` and `F12 = F11 COPYSIGN F12`; `R8 = R12` then moves `sg`
into the register that held `w^2`). The mix, the memory-held accumulators and the scale `P[16] g`
are those of `stage_super`.

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x121E9D`
(`docs/research/synth-stage-models-5.md`).
"""

from __future__ import annotations

import synth_stage_unison as un
import synth_wave_model as m

VOICES = 7  # VERIFIED (listing `0x243E52`, `LCNTR = 7`)


def fold_diff(phases: list[int], r7: m.X) -> list[m.X]:
    """The inline triangle fold of the loop: four registers `s_k`."""
    out = []
    for x in phases:
        a = abs(m.flt(x, -31))
        d = m.sub(a, m.HALF)
        w = m.sub(m.ONE, m.clip(m.ONE, m.mul(d, r7)))
        w2 = m.mul(w, w)
        sg = m.ONE if d >= 0 else -m.ONE
        out.append(m.sub(sg, w2 if d >= 0 else -w2))
    return out


def stage_super1(lanes: list[m.Lane]):
    """Stage `0x121E9D`, both lanes: `(out_bits[lane][4], voice phases[lane], 0)`."""
    info = [un.lane_unison(ln) for ln in lanes]
    acc = [[m.X(0)] * 4 for _ in lanes]
    phases = [list(ln.phases) for ln in lanes]
    for j in range(VOICES):
        s = []
        for i in (0, 1):
            inc = info[i][2][j]
            ph = [m.u32(phases[i][j] + k * inc) for k in (1, 2, 3, 4)]
            phases[i][j] = ph[3]
            s.append(fold_diff(ph, info[i][1]))
        un.mix_voice(s, lanes, j, acc)
    out = []
    for i, ln in enumerate(lanes):
        scale = m.mul(m.ld(ln.out_gain), info[i][0])
        out.append([m.st(m.mul(a, scale)) for a in acc[i]])
    return out, [p[VOICES - 1] for p in phases], [0, 0]
