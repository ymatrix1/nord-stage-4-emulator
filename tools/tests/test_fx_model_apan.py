"""Tests for tools/fx_model_apan.py: the auto-pan routine `0x122D93` model."""

import random

import fx_biquad_check as fb
import fx_model_apan as pan
import fx_models
import fx_oracle as fo
from fx_pe import M32


def run(case):
    return fo.model_run(pan.routine(), case)


def test_the_digest_is_stable():
    rt = fx_models.routines()["apan"]
    assert fx_models.digest(rt) == fx_models.digest(rt)


def test_phase_is_an_integer_sum_stored_to_both_words_and_the_loop_runs_ctx_minus_3_blocks():
    case = pan.random_scenario(random.Random(3))
    count = case[pan.CTX - 3]
    mem, written = run(case)
    for k in (0, 1):  # the counter pair is read through the pointer c[-0x1B]
        assert mem[pan.CTX - 0x1F + k] == (case[pan.PHASE_PTR + k] + case[pan.CTX - 0x20]) & M32
    assert {pan.IN_ADDR + k for k in range(4 * count)} <= written
    assert pan.IN_ADDR + 4 * count not in written


def test_left_and_right_gains_are_p_plus_and_minus_the_same_pan_term():
    case = pan.random_scenario(random.Random(11))
    case[pan.CTX - 3] = 1
    for k in range(4):
        case[pan.IN_ADDR + k] = fb.bits(1.0)
    mem, _ = run(case)
    out = [fb.from_bits(mem[pan.IN_ADDR + k]) for k in range(4)]
    assert out[0] == out[2] and out[1] == out[3]  # both frames of a block share the gains
    p = fb.from_bits(mem[pan.CTX - 0x13])  # smoothed offset P
    assert abs((out[0] + out[1]) / 2 - p) < 1.0e-6 * max(1, abs(p))


def test_scenarios_fill_every_word_the_routine_reads():
    rng = random.Random(1)
    for _ in range(20):
        run(pan.random_scenario(rng))  # a read of an unset word raises
