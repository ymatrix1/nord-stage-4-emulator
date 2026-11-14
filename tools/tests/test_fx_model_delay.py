"""Tests for tools/fx_model_delay.py (delay routine 0x1206B0 model, synthetic scenarios only)."""

import random
from fractions import Fraction

import fx_biquad_check as fb
import fx_model_delay as d
import fx_models
import fx_oracle as fo
from fx_pe import M32, Missing, signed32


def run(case):
    rt = d.routine()
    mem = Missing({**rt.static, **case})
    machine = rt.model(mem, None)
    return mem, machine


def test_golden_digest_of_the_model_written_words():
    # recorded from the model that matched the oracle on 1,000 random scenarios (check delay)
    rt = d.routine()
    assert (
        fx_models.digest(rt) == "11cc5d01593f09a86c49a8c11ca904d8c93b7f2201607d6027bada9c9f8a8c83"
    )


def test_the_routine_executes_236_instructions_and_stays_in_its_range():
    case = d.random_scenario(random.Random(3))
    _, m = run(case)
    assert len(m.pcs) == 236
    assert m.pcs[0] == d.PC and max(m.pcs) < d.END and m.pcs == sorted(m.pcs)


def test_scenarios_are_deterministic_and_fill_every_word_the_model_reads():
    a, b = d.random_scenario(random.Random(4)), d.random_scenario(random.Random(4))
    assert a == b
    rt = d.routine()
    mem, written = fo.model_run(rt, a)  # raises KeyError on an unfilled read
    assert d.CTX - 0x20 in written and d.OUT_ADDR in written and d.IN_ADDR in written
    assert {d.SCRATCH_1, d.SCRATCH_2} <= written  # scratch pairs written then read back
    assert mem[d.IN_ADDR] != a[d.IN_ADDR]  # the frames are rewritten in place


def test_time_ratio_is_the_clamped_quotient_of_the_two_levels():
    """`CTX+0xC`: `clamp(P7 / P9 (1 - k) + s k)`; with `k = 0` it is the ratio itself."""
    case = d.random_scenario(random.Random(5))
    f = fb.bits
    case[d.CTX + 7], case[d.CTX + 8] = f(0.4), f(0.4)
    case[d.CTX + 9], case[d.CTX + 0xA] = f(0.8), f(0.8)
    case[d.CTX + 0xE] = 0  # k = 0
    case[d.CTX + 0x24], case[d.CTX + 0x25] = f(1.0), f(0.01)
    mem, _ = run(case)
    got = fb.from_bits(mem[d.CTX + 0xC])
    assert abs(got - Fraction(1, 2)) < Fraction(1, 2**20)  # two Newton steps from the seed
    assert mem[d.CTX + 0xC] == mem[d.CTX + 0xD]  # both channels, linked levels
    case[d.CTX + 0x24] = f(0.3)  # upper clamp below the ratio
    mem, _ = run(case)
    assert mem[d.CTX + 0xC] == f(0.3)


def test_meter_is_a_decaying_peak_hold_of_the_output():
    """`CTX+7`: `max(|y|, previous * decay)`; a huge previous level decays by `CTX+0xB`."""
    case = d.random_scenario(random.Random(6))
    f = fb.bits
    case[d.CTX + 7], case[d.CTX + 8] = f(0.9), f(0.9)
    case[d.CTX + 0xB] = f(0.5)
    mem, _ = run(case)
    peak = fb.from_bits(mem[d.CTX + 7])
    assert peak >= Fraction(9, 10) * Fraction(1, 2) * (1 - Fraction(1, 2**22))


def test_stores_use_exact_pair_layout_and_wrap_free_addresses():
    case = d.random_scenario(random.Random(7))
    mem, m = run(case)
    assert all(0 <= a <= M32 for a in m.written)
    # the output buffer receives two pairs (4 words) and `I0` advanced by 4 words
    assert {d.OUT_ADDR + k for k in range(4)} <= m.written
    assert m.ir["i0"] == d.IN_ADDR + 4 and signed32(m.ir["i4"]) == d.OUT_ADDR + 4


def test_a_changed_smoothing_word_changes_the_result():
    rng = random.Random(8)
    case = d.random_scenario(rng)
    base, _ = run(case)
    case2 = dict(case)
    case2[d.CTX - 0x1B] ^= 0x00400000  # target of the first smoother
    changed, _ = run(case2)
    assert changed[d.CTX - 0x14] != base[d.CTX - 0x14]
