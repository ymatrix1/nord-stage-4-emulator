"""Tests for the all-pass (0x122C39) and resonant filter (0x1243F4, 0x124497) models."""

import random

import fx_biquad_check as fb
import fx_model_allpass as ap
import fx_model_resonant as r2
import fx_model_resonant4 as r4
import fx_models
import fx_oracle as fo
from fx_pe import Missing

F = fb.bits


def run(rt, case):
    mem = Missing({**rt.static, **case})
    machine = rt.model(mem, None)
    return mem, machine


def test_golden_digests_of_the_model_written_words():
    # recorded from the models that matched the oracle on 1,200 random scenarios each
    rts = fx_models.routines()
    digests = {name: fx_models.digest(rts[name]) for name in ("allpass", "resonant", "resonant4")}
    assert len(set(digests.values())) == 3
    assert digests == {
        "allpass": "655c00f519fec8a2e55a41c9abb538f2dc6555a7d4bf84e68fc0017bb398027f",
        "resonant": "e2475cccd652ea2c02df1995a8a2d91c6880092141ee30bf2f92dddb72a115c8",
        "resonant4": "25ad4c7706846480dbd7c490b9e5b9191310861ab4e1e5d4ff8ce64380863018",
    }


def test_allpass_runs_a_preamble_and_a_loop_body_per_block():
    counts = []
    for n in (1, 2, 3):
        case = ap.random_scenario(random.Random(2))
        for a in range(ap.IN_ADDR, ap.IN_ADDR + 4 * n):
            case.setdefault(a, F(0.25))
        case[ap.CTX + 4] = n
        mem, m = run(ap.routine(), case)
        counts.append(len(m.pcs))
        assert m.ir["i0"] == ap.IN_ADDR + 4 * n
    assert counts[1] - counts[0] == counts[2] - counts[1] == 60  # 0x122C8E .. 0x122D28
    assert counts[0] == 98


def test_allpass_phase_word_is_the_integer_sum_and_the_gain_uses_the_counter():
    case = ap.random_scenario(random.Random(4))
    case[ap.CTX - 0x1F], case[ap.CTX - 0x20] = 0xFFFFFFF0, 0x20
    mem, _ = run(ap.routine(), case)
    assert mem[ap.CTX - 0x1F] == mem[ap.CTX - 0x1E] == 0x10  # wraps modulo 2^32
    # a zero mix leaves only the dry path: out = g x, one common gain for all four words
    case[ap.CTX + 2] = 0
    out, _ = run(ap.routine(), case)
    ratios = [
        fb.from_bits(out[ap.IN_ADDR + k]) / fb.from_bits(case[ap.IN_ADDR + k]) for k in range(4)
    ]
    assert all(abs(r / ratios[0] - 1) < 1e-6 for r in ratios)


def test_resonant_two_frame_routine_is_parameter_smoothing_then_filtering():
    rt = r2.routine()
    case = r2.random_scenario(random.Random(5))
    mem, m = run(rt, case)
    assert len(m.pcs) == 60 and m.ir["i0"] == r2.IN_ADDR + 4
    # the smoothed integrator gain is stored back toward its target by the shared coefficient
    k = fb.from_bits(case[r2.CTX - 0x11])
    tgt, st = fb.from_bits(case[r2.CTX - 0x1F]), fb.from_bits(case[r2.CTX - 0x1D])
    got = fb.from_bits(mem[r2.CTX - 0x1D])
    assert abs(float(got - (tgt + k * (st - tgt)))) < 1e-6


def test_resonant_clip_bounds_the_integrator():
    rt = r2.routine()
    case = r2.random_scenario(random.Random(6))
    case[r2.CTX - 0xE] = F(0.01)  # tiny limit
    for off in (-0x7, -0x6):
        case[r2.CTX + off] = F(0.9)
    mem, _ = run(rt, case)
    for off in (-0x7, -0x6):  # |w'| <= limit
        assert abs(fb.from_bits(mem[r2.CTX + off])) <= fb.from_bits(F(0.01))


def test_resonant_four_frame_routine_covers_the_delayed_return_slots():
    rt = r4.routine()
    case = r4.random_scenario(random.Random(7))
    mem, m = run(rt, case)
    assert len(m.pcs) == 89 and m.pcs[-2:] == [0x124580, 0x124583]
    assert {r4.IN_ADDR + k for k in range(8)} <= m.written  # all 4 frames rewritten in place
    assert m.ir["i0"] == r4.IN_ADDR  # not advanced


def test_zero_context_passes_the_input_through():
    """No gains, no states: `y = x` in both resonant routines."""
    for rt in (r2.routine(), r4.routine()):
        case = {a: 0 for a in range(r2.CTX - 0x20, r2.CTX)}
        case.update({r2.IN_ADDR + k: F(0.5 + k) for k in range(8)})
        mem, _ = run(rt, case)
        assert [mem[r2.IN_ADDR + k] for k in range(8)] == [F(0.5 + k) for k in range(8)]


def test_scenarios_fill_every_word_the_models_read():
    for name in ("allpass", "resonant", "resonant4"):
        rt = fx_models.routines()[name]
        fo.model_run(rt, rt.scenario(random.Random(8)))  # KeyError on an unfilled read
