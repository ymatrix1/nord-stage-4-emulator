"""Tests for tools/fx_model_lfo.py (table LFO routine 0x1228CF model, synthetic scenarios only)."""

import random

import fx_biquad_check as fb
import fx_model_lfo as lfo
import fx_models
import fx_oracle as fo
from fx_pe import M32, Missing

F = fb.bits


def run(case, rt=None):
    rt = rt or lfo.routine()
    mem = Missing({**rt.static, **case})
    machine = rt.model(mem, None)
    return mem, machine


def test_golden_digest_of_the_model_written_words():
    # recorded from the model that matched the oracle on 1,200 random scenarios (check lfo)
    assert fx_models.digest(lfo.routine()) == (
        "3ee8e68bbf6e80d713829db99648ea18e9579e695ae9964b7d25cc63d7652741"
    )


def test_instruction_count_is_a_preamble_plus_a_loop_body_per_block():
    counts = []
    for n in (1, 2, 3):
        case = lfo.random_scenario(random.Random(9))
        for a in range(lfo.IN_ADDR, lfo.IN_ADDR + 4 * n):
            case.setdefault(a, F(0.25))
        case[lfo.CTX + 0x15] = n
        _, m = run(case)
        counts.append(len(m.pcs))
        assert m.ir["i0"] == lfo.IN_ADDR + 4 * n  # four words (two stereo frames) per pass
    assert counts[1] - counts[0] == counts[2] - counts[1] == 103  # 0x122935 .. 0x122A19
    assert counts[0] == 146


def test_phase_smoothing_index_and_table_interpolation():
    """The state `CTX-0x1F` follows the target `CTX-0x20` with `k = 0.9908`; the table output
    for the phase word `p * 2^31` is `A[floor(128 p)] + frac (A[idx + 1] - A[idx])`."""
    rt = lfo.routine()
    case = lfo.random_scenario(random.Random(10))
    case[lfo.CTX - 0x20] = case[lfo.CTX - 0x1F] = F(0.25)  # state == target: phase stays 0.25
    case[lfo.CTX - 2], case[lfo.CTX - 1] = 31, (-31) & M32
    case[lfo.CTX - 0xD] = F(1.0)  # gain of the first table output
    case.update({lfo.TAB_A + k: F(float(k)) for k in range(260)})  # ramp: A[k] = k
    mem, _ = run(case, rt)
    assert mem[lfo.CTX - 0x1F] == F(0.25)
    assert mem[lfo.CTX - 0xA] == F(32.0)  # idx = 0.25 * 128 = 32, fraction 0
    case[lfo.CTX - 0x20] = case[lfo.CTX - 0x1F] = F(0.25 + 1 / 512)
    mem, _ = run(case, rt)
    assert mem[lfo.CTX - 0xA] == F(32.25)  # a quarter of the way to A[33]


def test_the_state_moves_toward_the_target_by_one_minus_k():
    case = lfo.random_scenario(random.Random(11))
    case[lfo.CTX - 0x20], case[lfo.CTX - 0x1F] = F(0.5), F(0.1)
    mem, _ = run(case)
    got = fb.from_bits(mem[lfo.CTX - 0x1F])
    k = float(fb.from_bits(lfo.K))  # 0.99078 (quoted as 0.9908 elsewhere)
    want = 0.5 + (0.1 - 0.5) * k
    assert abs(float(got) - want) < 1e-6


def test_scenarios_fill_every_word_the_model_reads():
    rt = lfo.routine()
    mem, written = fo.model_run(rt, lfo.random_scenario(random.Random(12)))  # KeyError if not
    assert lfo.IN_ADDR in written and lfo.CTX - 0x1F in written
    assert mem[lfo.CTX - 0x1F] != 0
