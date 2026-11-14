import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import dsp_frame_rate as d  # noqa: E402


def test_arm_literal_words_are_the_float32_of_the_rate():
    assert d.f32(0x47298A58) == 43402.34375 == d.FS
    assert d.f32(0x46A98A58) == d.FS / 2
    assert d.f32(0x47A98A58) == d.FS * 2
    assert d.FS * 1024 == 44_444_000


def test_find_literals_reports_pool_offsets():
    blob = struct.pack("<4I", 0, 0x47298A58, 7, 0x47A98A58)
    assert d.find_literals(blob) == [(4, "fs"), (12, "2 fs")]


def test_delay_words_are_truncated_ms_times_the_rate():
    rows = d.delay_check()
    assert all(words == model for _, words, model in rows)
    # 43,400 Hz would miss three of the six words
    miss = [1 for ms, words in d.DELAY_ROWS if d.delay_samples(ms, 43_400.0) != words]
    assert len(miss) == 3


def test_sy_fade_words_give_five_and_fifteen_khz():
    r = d.sy_implied()
    for v in r.values():
        assert abs(v - 43_402.0) < 0.05
    assert abs(d.f32(d.SY_WORDS[1]) * d.FS - 5000.0) < 0.1
    assert abs(d.f32(d.SY_WORDS[0]) * d.FS - 15000.0) < 0.2


def test_b3_fine_tune_words_explain_both_old_rates():
    r = d.b3_check()
    assert abs(r["image"]["implied_fs"] - 43_829.0) < 1.0
    assert abs(r["arm"]["cents_at_FS"] - 0.18) < 0.01
    assert d.b3_step(60, d.ARM_TUNE) == 51_784_672
    assert d.b3_step(60, d.B3_IMAGE_TUNE) == 51_275_456
    assert d.b3_step(72, d.ARM_TUNE) == 2 * d.b3_step(60, d.ARM_TUNE)


def test_bank_ladder_of_a_synthetic_12tet_table_sits_on_c1_at_the_arm_tune():
    # table of v words generated for exactly C1 at FS and the ARM tune: the ladder deviation is ~0
    cyc0 = d.C1 / d.FS
    v0 = cyc0 * 2**32 / (256 * d.ARM_TUNE / 2**31)
    table = []
    for k in range(92):
        v = round(v0 * 2 ** (k / 12))
        table += [v, v]
    r = d.bank_ladder(table)
    assert abs(r["mean"]) < 0.15 and r["sd"] < 0.15  # integer rounding of v: 0.07 cent
    # at unity tune the same table is 10.16 cent sharp
    assert math.isclose(d.bank_ladder(table, d.UNITY_TUNE)["mean"], 10.16, abs_tol=0.1)


def test_fracmul_rounds_and_handles_signs():
    assert d.fracmul(0x40000000, 0x40000000) == 0x20000000
    assert d.fracmul(0xC0000000, 0x40000000) == -0x20000000
    assert d.fracmul(3, 0x40000000) == 2  # 1.5 rounds to 2


def test_main_prints_the_offline_checks(capsys):
    assert d.main(["delay"]) == 0
    assert d.main(["sy"]) == 0
    assert d.main(["b3"]) == 0
    out = capsys.readouterr().out
    assert "ok" in out and "43401.99" in out and "image" in out
