"""Tests for tools/fx_routine_models.py (compressor model, helpers, oracle command line)."""

import random
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_biquad_check as fb  # noqa: E402
import fx_routine_models as m  # noqa: E402

#: Output words of `golden_scenario` recorded from `sharc-fx` running the firmware routine on the
#: FX Core2 image (6 blocks, state carried; `check compressor` reports them identical).
ORACLE_OUTPUT = [
    0xBE9F2E14, 0x3E728F5C, 0xBE26C28F, 0x3EC5147A, 0xBC728F5C, 0xBED43D70, 0x3E0870A3, 0xBE8870A3,
    0x3F81076C, 0xBED94FDF, 0x3FC4F062, 0x3DD94FDF, 0xBFA9C666, 0x3F22FBE7, 0xBF4BBAE1, 0x3F9566E9,
    0xBED27AC0, 0xC0286233, 0x3ED27AC0, 0xBFE78706, 0x3F9DDC10, 0xBF7C934D, 0x40038CB8, 0xBE286233,
    0xBE9ED486, 0x3DAF42C9, 0xBE501F4E, 0x3E452B22, 0xBDC52B22, 0x3E995A6F, 0x3C2F42C9, 0xBE8E6643,
    0x3EE09BE2, 0xBF235A19, 0x3F566640, 0xBE750725, 0x3F9E3F48, 0x3E235A19, 0xBF6AD183, 0x3F0EEED5,
    0xBF5C6D46, 0x3FCB788F, 0xBE4B788F, 0xBFFE56B3, 0x3EED61FD, 0xBFA98F22, 0x3F902010, 0xBF298F22,
]  # fmt: skip


def block_context(**over: int) -> dict[int, int]:
    """A context with unity makeup, no compression (threshold far above) unless overridden."""
    ctx = {o: 0 for o in range(m.LO_OFFSET, m.HI_OFFSET + 1)}
    base = {
        "m3": 4, "m4": 6, "mant_scale": (-31) & 0xFFFFFFFF, "fix_scale": 31,
        "log_gain": fb.bits(1.0), "table0": 0x3F800000, "table1": 0x3F000000,
        "smooth": fb.bits(0.5), "makeup_target": fb.bits(1.0), "makeup": fb.bits(1.0),
        "threshold": fb.bits(100.0), "slope": fb.bits(1.0), "reduction_scale": fb.bits(0.5),
        "attack": fb.bits(0.5), "release": fb.bits(0.5),
    }  # fmt: skip
    base.update(over)
    for name, v in base.items():
        ctx[m.FIELDS[name]] = v
    for name in ("makeup_target", "makeup", "attack", "release", "slope", "level", "excess"):
        ctx[m.FIELDS[name] + 1] = ctx[m.FIELDS[name]]
    ctx[m.FIELDS["threshold"] + 1] = base["threshold"]
    return ctx


def run_block(inp: list[int], ctx: dict[int, int]) -> tuple[dict[int, int], dict]:
    mem = {m.IN_ADDR + k: w for k, w in enumerate(inp)}
    mem.update({m.CTX_ADDR + o: w for o, w in ctx.items()})
    trace = m.compressor_0x120630(mem, m.IN_ADDR, m.CTX_ADDR)
    return mem, trace


def test_float_conversion_helpers():
    assert m.logb_word(Fraction(1)) == 0 and m.logb_word(Fraction(3, 4)) == 0xFFFFFFFF  # -1
    assert m.logb_word(Fraction(0)) == 0xFF800000
    assert m.mant_word(Fraction(3, 4)) == 0xC0000000  # 1.5 as 1.31
    assert m.mant_word(Fraction(-8)) == 0x80000000
    # MANT read as a signed integer and scaled by 2^-31 is mantissa - 2 (the log2 estimate trick)
    q = Fraction(3, 4)
    est = m.float_by(m.signed32(m.logb_word(q)) + 1, 0) + m.float_by(
        m.signed32(m.mant_word(q)), -31
    )
    assert est == Fraction(-1) + Fraction(1, 2)  # e + frac = -1 + 0.5
    assert m.fix_by_word(Fraction(3, 2), 0) == 2 and m.fix_by_word(Fraction(5, 2), 0) == 2
    assert m.fix_by_word(Fraction(-1, 2), 1) == 0xFFFFFFFF  # -1
    assert m.fix_by_word(Fraction(1), 31) == 0xFFFFFFFF  # out of range
    assert m.ssf_word(0x40000000, 0x40000000) == 0x20000000  # 0.5 * 0.5 = 0.25
    assert m.ssf_word(0xC0000000, 0x40000000) == 0xE0000000  # -0.5 * 0.5


def test_golden_scenario_reproduces_the_oracle_recording():
    got = m.golden_vectors()
    assert got["output"] == ORACLE_OUTPUT
    assert len(got["context"]) == m.HI_OFFSET - m.LO_OFFSET + 1


def test_golden_scenario_engages_the_gain_reduction():
    blocks, ctx = m.golden_scenario()
    mem = {m.CTX_ADDR + o: w for o, w in ctx.items()}
    gains = []
    for blk in blocks:
        mem.update({m.IN_ADDR + k: w for k, w in enumerate(blk)})
        gains.append(float(m.compressor_0x120630(mem, m.IN_ADDR, m.CTX_ADDR)["gain"][0]))
    assert gains[0] > gains[-1] and 0.5 < gains[-1] < 1.0


def test_below_threshold_the_block_is_scaled_by_the_makeup_gain_only():
    inp = [fb.bits(v) for v in (0.5, -0.25, 0.125, 0.0625, -0.5, 0.25, 0.375, -0.125)]
    mem, trace = run_block(inp, block_context(makeup=fb.bits(0.5), makeup_target=fb.bits(0.5)))
    assert trace["gt"] == [False, False]
    for k, w in enumerate(inp):
        assert abs(fb.from_bits(mem[m.IN_ADDR + k]) - fb.from_bits(w) / 2) < Fraction(1, 10**6)


def test_stereo_link_gives_both_channels_the_same_reduction_fraction():
    loud = [1.0, 0.001, 0.9, 0.001, 0.8, 0.001, 0.7, 0.001]  # left loud, right quiet
    inp = [fb.bits(v) for v in loud]
    ctx = block_context(threshold=fb.bits(-4.0), attack=fb.bits(1.0), reduction_scale=fb.bits(0.2))
    _, trace = run_block(inp, ctx)
    assert trace["r5"][0] == trace["r5"][1] != 0


def test_negative_zero_input_keeps_its_sign_through_a_positive_gain():
    inp = [0x80000000, 0x00000000] + [fb.bits(0.25)] * 6
    mem, _ = run_block(inp, block_context())
    assert mem[m.IN_ADDR] == 0x80000000 and mem[m.IN_ADDR + 1] == 0


def test_aliased_frame_offsets_load_before_they_store():
    # m3 = m4 = 2 alias the second frame; at unity gain every slot still holds its input word
    inp = [fb.bits(v) for v in (0.5, 0.5, 0.25, 0.25, 0.75, 0.75, 0.125, 0.125)]
    mem, _ = run_block(inp, block_context(m3=2, m4=2))
    assert [mem[m.IN_ADDR + k] for k in range(8)] == inp  # frames 2 and 3 are not visited


def test_random_scenarios_run_and_stay_finite():
    rng = random.Random(3)
    for _ in range(20):
        inp, ctx = m.random_scenario(rng)
        mem = m.run_model(inp, ctx)
        for k in range(8):
            assert (mem[m.IN_ADDR + k] >> 23) & 0xFF != 0xFF  # no inf / nan


def test_oracle_args_cover_block_and_context_ranges():
    inp, ctx = m.random_scenario(random.Random(1))
    args = m.oracle_args("img.bin", inp, ctx)
    assert args[0] == "img.bin" and "--call" in args and "0x120630" in args
    n = m.HI_OFFSET - m.LO_OFFSET + 1
    assert f"{m.CTX_ADDR + m.LO_OFFSET:#x}:{n}" in args
    assert len(args[args.index("--fill") + 1].split("=")[1].split(",")) == 8


def test_golden_rust_lists_the_three_arrays():
    text = m.golden_rust()
    for name in ("INPUT", "OUTPUT", "CONTEXT", "CONTEXT_IN"):
        assert f"CMP_GOLDEN_{name}" in text
    assert "0xbe9f2e14" in text
