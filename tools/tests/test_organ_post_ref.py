"""Unit tests for tools/organ_post_ref.py (synthetic scenarios; no firmware, no oracle binary).

The golden digests were produced by the Python model on seeds that `organ_post_oracle.py verify`
proved bit-exact against the SHARC oracle (seeds 0..5, 0 differing words).
"""

from __future__ import annotations

import hashlib
import math

import organ_post_ref as pr


def digest(m: pr.Mem, lo: int, hi: int) -> str:
    h = hashlib.sha256()
    for a in range(lo, hi):
        if a not in (0xE210E, 0xE210F):
            h.update(a.to_bytes(4, "little") + m[a].to_bytes(4, "little"))
    return h.hexdigest()[:16]


def test_post_stage_matches_the_oracle_verified_digests():
    for seed, want in ((0, "86d94a56d077029a"), (1, "15481af8f865e514")):
        m = pr.Mem(pr.synthetic_inputs(seed))
        pr.post_stage(m)
        assert digest(m, 0xE0000, 0xE3000) == want


def test_output_stage_matches_the_oracle_verified_digest():
    m = pr.Mem(pr.output_inputs(0))
    pr.output_stage(m, 0xB0200, pr.OUT_RECORD)
    assert digest(m, 0xB0000, 0xB2000) == "3c1b7343797da8f6"
    assert [m[0xB0200 + i] for i in range(2)] == [0x3CC2C31B] * 2  # both lanes carry one value


def quiet(seed: int = 3) -> pr.Mem:
    """A scenario with the LFO depth and offset at zero modulation and frozen oscillators."""
    m = pr.Mem(pr.synthetic_inputs(seed))
    m[pr.DEPTH], m[pr.OFFSET] = 0, 0x40000000
    for k in range(pr.NOTES):
        m[pr.RATE + k] = 0
    return m


def test_lfo_advances_by_the_step_per_frame_and_mod_is_zero_without_depth():
    m = quiet()
    start = m[pr.LFO]
    pr.lfo_mod(m)
    assert m[pr.LFO] == (start + pr.LFO_STEP) & 0xFFFFFFFF
    assert m[pr.MODW] == 0 and m[pr.MODW + 1] == 0
    pitch = pr.lfo_mod(m)
    assert pitch == [0x40000000, 0x40000000]  # m = 1/2 means no detune
    assert m[pr.LFO] == (start + 2 * pr.LFO_STEP) & 0xFFFFFFFF


def test_fine_tune_offset_is_a_pitch_fraction():
    m = quiet()
    m[pr.OFFSET] = 0x40000000 + 0x01000000  # +2^24 / 2^31 * 2 = +2^-6
    pr.lfo_mod(m)
    assert m[pr.MODW] == 0x02000000  # mod = 2 (fine - 1/2)
    pitch = pr.lfo_mod(m)
    assert pitch[0] == 0x40000000 + 0x01000000  # m = 1/2 + mod / 2


def test_scanner_reads_the_waveshaper_table_at_the_quantised_triangle():
    m = quiet()
    for k in range(pr.NOTES):
        m[pr.STATE + k] = 0x80000000  # |ph| = 1 -> (1 - 0.5) * coef, clipped to 1
        m[pr.COEF + k] = pr.f2u(4.0)
    for i in range(-512, 513):
        m[pr.TABLE + i] = i + 1000  # an index marker per table slot
    pr.scanner(m, pr.OUT_A)
    assert m[pr.OUT_A] == 512 + 1000  # clip(0.5 * 4, 1) = 1 -> fix(1 * 2^9)
    assert m[pr.OUT_A + 60] == 512 + 1000  # notes 48.. skip the asymmetry stage


def test_asymmetry_stage_scales_only_negative_samples_of_notes_12_to_47():
    m = quiet()
    for k in range(pr.NOTES):
        m[pr.STATE + k] = 0x40000000  # ph = 0.5 -> |ph| - 0.5 = 0
    for i in range(-512, 513):
        m[pr.TABLE + i] = pr.f2u(-0.5)
    pr.scanner(m, pr.OUT_A)
    assert m[pr.OUT_A + 5] == pr.f2u(-0.5)  # outside 12..47: untouched
    gain = 0.9 + 0.2 * 0.5  # 0.9 + 0.2 ph
    got = pr.ld(m[pr.OUT_A + 20])
    assert abs(float(got) - (-0.5 * gain)) < 1e-6
    for i in range(-512, 513):
        m[pr.TABLE + i] = pr.f2u(0.5)
    pr.scanner(m, pr.OUT_B)
    assert m[pr.OUT_B + 20] == pr.f2u(0.5)  # positive samples pass unchanged


def test_smooth_is_a_two_tap_fir_plus_one_pole():
    m = pr.Mem()
    one = pr.f2u(1.0)
    for k in range(pr.NOTES):
        m[pr.OUT_A + k], m[pr.OUT_B + k], m[pr.Y1 + k] = one, pr.f2u(2.0), pr.f2u(4.0)
    for i in range(0, 252, 12):
        for j in range(4):
            m[pr.FILT + i + j] = pr.f2u(0.5)  # cB
            m[pr.FILT + i + 4 + j] = pr.f2u(0.25)  # cA
            m[pr.FILT + i + 8 + j] = pr.f2u(0.125)  # cC
    pr.smooth(m, pr.OUT_A, pr.OUT_B, pr.Y1, pr.Y0)
    assert pr.ld(m[pr.Y0]) == 1 * 0.25 + 2 * 0.5 + 4 * 0.125  # 1.75
    assert pr.ld(m[pr.Y0 + 83]) == 1.75


def test_sine_polynomial_is_a_sine_of_the_folded_phase():
    m = pr.Mem()
    for i, v in enumerate((-30 & 0xFFFFFFFF, -30 & 0xFFFFFFFF, pr.f2u(-1.0), pr.f2u(-1.0))):
        m[pr.CONSTS + i] = v
    # firmware polynomial: u (pi/2 - 0.6459 u^2 + 0.0795 u^4 - 0.004339 u^6), u = |ph 2^-30| - 1
    for t, c in enumerate((-0.0043311, 0.0794309, -0.645891, 1.57079)):
        m[pr.CONSTS + 4 + 2 * t] = m[pr.CONSTS + 5 + 2 * t] = pr.f2u(c)
    for j in range(pr.SINES):
        m[pr.GAINS + j] = pr.f2u(1.0)
        m[pr.SINE_PH + j] = 0xA0000000  # ph = -1.5 * 2^30: u = 0.5 (the ladder is zero: frozen)
    for q, v in enumerate(pr.BUCKET_PAIRS):
        m[pr.BUCKET_INDEX + 2 * q] = v
    pr.sines(m)
    # pairs 0, 6, 12 and 16 add into bucket pair 0: four oscillators per lane
    for lane in (0, 1):
        got = float(pr.ld(m[pr.Y0 + pr.NOTES + lane]))
        assert abs(got - 4 * math.sin(math.pi / 4)) < 1e-3
    # buckets of the second frame are separate (0x60 words further) and equal here
    assert m[pr.Y0 + pr.NOTES + pr.BUCKET_STRIDE] == m[pr.Y0 + pr.NOTES]
    # pair 2 (oscillators 4, 5) only reaches bucket pair 4 through its single octave-0 entry
    assert float(pr.ld(m[pr.Y0 + pr.NOTES + 4])) > 2.0


def test_output_stage_with_identity_sections_weights_the_96_words():
    m = pr.Mem()
    w, g, i1 = pr.OUT_WEIGHTS, pr.OUT_PARAMS, pr.OUT_RECORD
    m[i1 - 0x1F], m[i1 - 0x1E] = w, g
    for i in range(96):
        m[w + i] = pr.f2u(0.0)
    m[w + 5] = pr.f2u(1.0)  # only note 5, lane y
    m[w + 6] = pr.f2u(2.0)  # only note 6, lane x
    for i in range(192):
        m[pr.Y0 + i] = pr.f2u(1.0)
    m[g], m[g + 1] = pr.f2u(1.0), 0
    m[g + 4] = m[g + 5] = pr.f2u(1.0)
    m[g + 2], m[g + 3], m[g + 8] = pr.OUT_STATES[1], pr.OUT_STATES[2], pr.OUT_STATES[0]
    m[g + 0xB] = pr.f2u(1.0)
    for s in pr.OUT_STATES:
        m[s + 6] = pr.f2u(1.0)  # b0 = 1, everything else zero: identity
    pr.output_stage(m, 0xB0200, i1)
    want = 3.0 * 0.090075  # (1 + 2) summed over both lanes, gain 1, scaled by 0x3DB8793E
    for i in range(4):
        assert abs(float(pr.ld(m[0xB0200 + i])) - want) < 1e-5
    assert m[0xB0200] == m[0xB0201]


def test_inputs_are_deterministic_and_cover_the_tables():
    a, b = pr.synthetic_inputs(5), pr.synthetic_inputs(5)
    assert a == b and a != pr.synthetic_inputs(6)
    assert len(a) > 1900  # 1025 table words, 252 filter words, 84 x 8 per-note words, ...
    assert all(0 <= v < 2**32 for v in a.values())
    o = pr.output_inputs(2)
    assert o[pr.OUT_RECORD - 0x1F] == pr.OUT_WEIGHTS
