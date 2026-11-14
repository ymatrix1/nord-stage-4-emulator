#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SUB OSC stage `0x12179A` (ORG core 2, `0x242F34..0x2430CC`).

Two oscillator passes (phase `DM(-21, I2)` / `DM(-19, I2)`, increments `inc = FIX(f rho, 31)`),
each the sum of a square one octave down and a saw:

    pulse_k = pulse_helper(phase, inc / 2, 0x80000000, amp = s5)     (pulse helper `0x1250C9`)
    saw_k   = saw_helper(2 phase, inc, amp = 1 - s5)                 (saw helper `0x12501F`)
    osc_k   = saw_k + stored(pulse_k)

the new phase is the one returned by the pulse helper (`phase + 4 inc / 2`), `inc / 2 = SSFR(inc,
0x40000000)`. The two passes are mixed with `mix_lanes` and scaled by `P[16] (1 - 0.3 s5)`.
"""

from __future__ import annotations

import synth_stage_misc as misc
import synth_stage_sync1 as sync1
import synth_wave_model as m


def sub_osc(phase: int, inc: int, s5: m.X):
    """`(four samples, new phase)` of one oscillator pass."""
    half = sync1.ssfr(inc, 0x40000000)
    pulse, new_phase = m.pulse_helper(phase, half, misc.SUB_OFFSET, amp=s5)
    saw, _ = m.saw_helper(m.u32(2 * phase), inc, amp=m.sub(m.ONE, s5))
    return [m.add(sw, pu) for sw, pu in zip(saw, pulse, strict=True)], new_phase


def stage_sub_osc(lanes: list[m.Lane]):
    """Stage `0x12179A`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs = []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        incs.append(
            (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        )
    s5 = [m.ld(ln.spread) for ln in lanes]
    o1 = [sub_osc(ln.phase1, c[0], s) for ln, c, s in zip(lanes, incs, s5, strict=True)]
    o2 = [sub_osc(ln.phase2, c[1], s) for ln, c, s in zip(lanes, incs, s5, strict=True)]
    scale_f = m.ld(misc.SUB_MIX)
    scales = [
        m.mul(m.ld(ln.out_gain), m.sub(m.ONE, m.mul(scale_f, s)))
        for ln, s in zip(lanes, s5, strict=True)
    ]
    return m.mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]
