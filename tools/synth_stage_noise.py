#!/usr/bin/env python3
"""Bit-exact model of the Core2 noise stage `0x121FD8` (ANALOG PURE wave 6).

Listing `0x243FB0..0x24405E` (ORG core 2). Per lane (SIMD, lane y one address above lane x) the
stage runs four steps of a linear congruential generator on the lane state word (`DM(0xB2EF9)`,
`DM(0xB2EFA)`), `s <- s M + C` with 32-bit wrap (the same constants for both lanes), converts each
state to a float `s 2^-31` (stored to the frame, so truncated to 24 bits), and mixes the two lanes
with the same four `P` weights as the oscillator stages:

    out_i[k] = trunc((n_i[k] P_own_i + n_(1-i)[k] P_partner) P[16 + i])

`P_own` for lane x is `P[-16]` (lane y `P[-15]`), `P_partner` the other lane word `P[-14 + i]`.
Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x121FD8`
(`docs/research/synth-stage-models.md`).
"""

from __future__ import annotations

import synth_wave_model as m

# VERIFIED (image words `DM(0xE08C8)`, `DM(0xE08C9)` of ORG core 2, read by the stage listing;
# oracle: the final state of both lanes after four steps equals `s M + C`).
MULT = 0x0BB38435
INC = 0x3619636B


def noise_states(state: int) -> list[int]:
    """Four LCG steps: the integer words of one lane (32-bit wrap)."""
    out = []
    for _ in range(4):
        state = m.u32(m.s32(state) * MULT + INC)
        out.append(state)
    return out


def stage_noise(lanes: list[m.Lane]):
    """Stage `0x121FD8`, both lanes: `(out_bits[lane][4], final state[lane], 0)`."""
    words = [noise_states(ln.state) for ln in lanes]
    n = [[m.ld(m.st(m.flt(w, -31))) for w in ws] for ws in words]
    out = []
    for i in (0, 1):
        me, other = lanes[i], lanes[1 - i]
        gain = m.ld(me.out_gain)
        row = []
        for k in range(4):
            mix = m.add(
                m.mul(n[1 - i][k], m.ld(other.weights[0])), m.mul(n[i][k], m.ld(me.weights[1]))
            )
            row.append(m.st(m.mul(mix, gain)))
        out.append(row)
    return out, [ws[3] for ws in words], [0, 0]
