"""Tests for tools/fx_model_chorus.py and tools/fx_golden_mod2.py: the Mod 2 chorus models."""

import random

import fx_biquad_check as fb
import fx_golden_mod2 as gm
import fx_model_chorus as ch
import fx_models
import fx_oracle as fo
from fx_pe import M32


def run(kind, seed=3):
    rt = ch.routine(kind)
    return rt, fo.model_run(rt, rt.scenario(random.Random(seed)))


def test_the_registry_has_both_routines_and_the_digests_are_stable():
    for name in ("chor4", "chor5"):
        rt = fx_models.routines()[name]
        assert fx_models.digest(rt) == fx_models.digest(rt)
    assert fx_models.digest(fx_models.routines()["chor4"]) != fx_models.digest(
        fx_models.routines()["chor5"]
    )


def test_the_phase_is_an_integer_sum_and_the_loop_runs_one_pass_per_block():
    case = ch.random_scenario(random.Random(4), 4)
    mem, written = fo.model_run(ch.routine(4), dict(case))
    assert mem[ch.CTX - 0x18] == (case[ch.CTX - 0x18] + case[ch.CTX - 0x20]) & M32
    count = case[ch.CTX + 9]
    assert {ch.IN_ADDR + k for k in range(4 * count)} <= written
    assert ch.IN_ADDR + 4 * count not in written


def test_the_variation_routine_reads_its_count_and_ring_from_other_words():
    case = ch.random_scenario(random.Random(5), 5)
    mem, written = fo.model_run(ch.routine(5), dict(case))
    count = case[ch.CTX + 2]
    assert {ch.IN_ADDR + k for k in range(4 * count)} <= written
    assert mem[ch.CTX - 0xD] != case[ch.CTX - 0xD]  # the ring pointer advanced
    assert all(ch.BUF <= mem[ch.CTX - 0xD] < ch.BUF + 0x200 for _ in (0,))


def test_the_ring_pointer_stays_inside_the_buffer():
    for kind, off in ((4, -7), (5, -0xD)):
        for seed in range(6):
            _, (mem, _) = run(kind, seed)
            assert ch.BUF <= mem[ch.CTX + off] < ch.BUF + 0x200


def test_ifeq_and_truncation_helpers():
    assert ch.ifeq([True, False], [1, 1], [2, 2]) == [1, 2]
    assert ch.ftrunc([fb.bits(-2.7)] * 2) == [(-2) & M32] * 2
    assert ch.ineg([5, 0]) == [(-5) & M32, 0]


def test_the_golden_ring_word_is_a_float_in_range_and_the_output_has_both_kinds():
    assert all(-1.0 <= fb.from_bits(gm.ring_word(k)) < 1.0 for k in range(64))
    text = gm.golden(4) + gm.golden(5)
    for name in ("CH4_CTX_IN", "CH5_FRAMES_OUT", "CH4_RING_DIFF", "CH5_CTX_OUT"):
        assert name in text
