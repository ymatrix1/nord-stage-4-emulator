#!/usr/bin/env python3
"""Bit-exact models of the further SHAPE oscillator stages (ORG core 2, 40-bit registers).

* `0x121AB5` ANALOG SHAPE wave 2, Shape Square (19 factory layers): the pulse helper `0x1250C9`
  twice per lane with the amplitude `0.7` and a pulse width taken from the shape words.

Same skeleton as `synth_wave_model.py` (`mix_lanes`); compared with the oracle by
`tools/synth_wave_oracle.py --stage` (`docs/research/synth-stage-models.md`).
"""

from __future__ import annotations

import synth_wave_model as m

# VERIFIED (listing `0x243590..0x2435E4`): constants of the Shape Square width law.
SQUARE_AMP = m.ld(0x3F333333)  # R6 of the helper call (0.7)
SQUARE_FADE = m.ld(0x40B00000)  # the width shrinks as `1 - min(1, 5.5 a)` at high pitch


def square_offset(inc1: int, shape_a: int, shape_b: int) -> int:
    """Pulse width word `2 FIX(w 2^30) - 2^31` of Shape Square (identical for both oscillators)."""
    a = m.flt(inc1, -31)
    room = m.sub(m.ONE, m.clip(m.mul(a, SQUARE_FADE), m.ONE))
    w = m.clip(m.add(m.clip(m.ld(shape_a), room), m.ld(shape_b)), m.ONE)
    return m.u32(2 * m.s32(m.fix(w, 30)) - 0x80000000)


def stage_shape_square(lanes: list[m.Lane]):
    """Stage `0x121AB5` (ANALOG SHAPE wave 2): `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, offs = [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (
            m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31),
            m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31),
        )
        incs.append(inc)
        offs.append(square_offset(inc[0], ln.shape_a, ln.shape_b))
    o1 = [
        m.pulse_helper(ln.phase1, c[0], o, amp=SQUARE_AMP)
        for ln, c, o in zip(lanes, incs, offs, strict=True)
    ]
    o2 = [
        m.pulse_helper(ln.phase2, c[1], o, amp=SQUARE_AMP)
        for ln, c, o in zip(lanes, incs, offs, strict=True)
    ]
    scales = [-m.ld(ln.out_gain) for ln in lanes]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]
