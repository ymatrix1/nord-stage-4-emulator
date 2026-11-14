import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_adr_model as m  # noqa: E402

ATTACK, DECAY, RELEASE, FAST_NEG = 0xC0DC8, 0xC0DCC, 0xC0DD0, 0xC0DD4
RECS = {
    ATTACK: m.Record.of(1.0, 0.1, 0.0, DECAY),
    DECAY: m.Record.of(0.8, 0.0, 0.5, DECAY),
    RELEASE: m.Record.of(0.9, 0.0, 0.0, RELEASE),
    FAST_NEG: m.Record.of(0.5, -0.03, -0.02, FAST_NEG),
}

# Golden: slot-0 frame word `DM(F - 20)` (level) and `DM(F - 18)` (record pointer) after oracle
# passes 1..15 (`python tools/synth_adr_oracle.py`, scenarios of section 3 of the research note).
# Scenario A: controller word 0x01000100 from pass 0, records poked every pass.
A_Y = [
    0x3DCCCCCD, 0x3E4CCCCD, 0x3E999999, 0x3ECCCCCC, 0x3EFFFFFF, 0x3F199999, 0x3F333332,
    0x3F4CCCCB, 0x3F666664, 0x3F7FFFFD, 0x3F800000, 0x3F4CCCCD, 0x3F23D70A, 0x3F03126E, 0x3F000000,
]  # fmt: skip
A_PTR = [ATTACK] * 10 + [DECAY] * 5
# Scenario B: controller word 0x00000200 before pass 8 (sign 0: start at the release record).
B_Y = [
    0x3DCCCCCD, 0x3E4CCCCD, 0x3E999999, 0x3ECCCCCC, 0x3EFFFFFF, 0x3F199999, 0x3F333332,
    0x3F2147AC, 0x3F1126E7, 0x3F02A302, 0x3EEB2569, 0x3ED3A1AA, 0x3EBE77E5, 0x3EAB6BE7, 0x3E9A4782,
]  # fmt: skip
B_PTR = [ATTACK] * 7 + [RELEASE] * 8
# Scenario D: controller word 0x04000200 before pass 6 (sign < 0, record with thr = -0.02).
D_Y = [
    0x3DCCCCCD, 0x3E4CCCCD, 0x3E999999, 0x3ECCCCCC, 0x3EFFFFFF, 0x3E6147AD, 0x3DA3D709,
    0x3C23D706, 0x00000000, 0x3DCCCCCD, 0x3E4CCCCD, 0x3E999999, 0x3ECCCCCC, 0x3EFFFFFF, 0x3F199999,
]  # fmt: skip
D_PTR = [ATTACK] * 5 + [FAST_NEG] * 3 + [ATTACK] * 7


def run(start):
    shifted = {p - 1: v for p, v in start.items()}  # oracle pass p = model pass p - 1
    return m.run_chain(RECS, ATTACK, 0, 15, shifted, attack=ATTACK)


def test_attack_decay_sustain_matches_the_oracle_words():
    out = run({})
    assert [y for y, _ in out] == A_Y
    assert [p for _, p in out] == A_PTR


def test_release_start_matches_the_oracle_words():
    out = run({8: RELEASE})
    assert [y for y, _ in out] == B_Y
    assert [p for _, p in out] == B_PTR


def test_negative_threshold_fires_a_restart_on_the_attack_record():
    out = run({6: FAST_NEG})
    assert [y for y, _ in out] == D_Y
    assert [p for _, p in out] == D_PTR


def test_decay_stops_at_the_sustain_level_and_attack_is_clamped_to_one():
    y, act = m.adr_step(m.f2u(0.95), m.Record.of(1.0, 0.1, 0.0, DECAY))
    assert (y, act) == (m.ONE, "link")
    y, act = m.adr_step(m.f2u(0.55), m.Record.of(0.8, 0.0, 0.5, DECAY))
    assert (m.u2f(y), act) == (0.5, "")


def test_rate_index_is_the_top_byte_of_the_clipped_sum():
    assert m.rate_index(0xC0000000) == -64  # the ARM idle word -2.0
    assert m.rate_index(0) == 0
    assert m.rate_index(0x7FFFFFFF) == 64  # clipped to +0.5
    assert m.rate_index(0x3F000000, 0x01000000) == 64
    assert m.rate_index(0x80000000, 0x80000000) == -64


# Golden: strip word 3 as the pass sees it (after the pass smoother) -> R of channel 0, from the
# oracle (`sweep_cutoff` in tools/synth_adr_oracle.py).
CUTOFF = [
    (0x9A7F8000, 0x00000000), (0xB0640000, 0x00000000), (0xC944C000, 0x00000000),
    (0xD03C0000, 0x00000000), (0xE36950C7, 0x3E0955BE), (0xF135D950, 0x3E79636E),
    (0xFA078000, 0x3EA08043), (0x00000000, 0x3EB8BE4D), (0x007F6000, 0x3EBAC378),
    (0x0121D950, 0x3EBD5726), (0x02FC4000, 0x3EC4DD52), (0x121D950B, 0x3F0125F0),
    (0x2FC40000, 0x3F3D574D), (0x3FB00000, 0x3F5DAA05), (0x4F9C0000, 0x3F7DFCBD),
    (0x598F8000, 0x3F816BB1), (0x65808000, 0x3F816BB1), (0x7F5FFFFF, 0x3F816BB1),
    (0x7EC0C7FF, 0x3F816BB1),
]  # fmt: skip


def test_cutoff_law_matches_the_oracle_within_float32_interpolation_error():
    worst = 0.0
    for word, bits in CUTOFF:
        worst = max(worst, abs(m.cutoff_r(word) - m.u2f(bits)))
    assert worst < 5e-6


def test_cutoff_law_limits_and_warp():
    assert m.cutoff_r(0xD03C0000) == 0.0  # below the table start R is 0
    assert abs(m.cutoff_r(0x7FFFFFFF) - 1.0111) < 1e-9  # flat top
    assert abs(m.cutoff_r(0) - 0.36083) < 1e-9
    assert abs(m.cutoff_fc_over_fs(1.0) - 0.25) < 1e-12


def test_osc_ratio_is_24_steps_per_octave():
    assert abs(m.osc_ratio(24) / m.osc_ratio(0) - 2.0) < 1e-12
    assert abs(m.osc_ratio(0) - 0.0060279) < 1e-7
