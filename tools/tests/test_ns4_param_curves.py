"""Tests for tools/ns4_param_curves.py (synthetic tables only, no firmware data)."""

from __future__ import annotations

import math
import struct

import ns4_param_curves as pc

SEMI = 2.0 ** (1.0 / 12.0)


def tan_table(n: int = 200, u0: float = 2.2e-5, clip: float = 1.99) -> list[float]:
    return [min(math.tan(u0 * SEMI**k), clip) for k in range(n)]


def gain_table() -> list[float]:
    """Synthetic copy of the ORG gain shape: 2^((j-353)/24) up to 0 dB, then 16^(k/127)."""
    lo = [2.0 ** ((j - 353) / 24) for j in range(354)]
    hi = [16.0 ** (k / 127) for k in range(1, 128)]
    return lo + hi


def image(words: list[float], at: int = 0, size: int = 4096) -> bytes:
    buf = [0.0] * size
    buf[at : at + len(words)] = words
    return struct.pack(f"<{size}f", *buf)


def test_fit_geometric_recovers_ratio_in_db_and_semitones() -> None:
    f = pc.fit_geometric([0.01 * SEMI**k for k in range(60)])
    assert f is not None and f.model == "geometric"
    assert abs(f.params["semitones_step"] - 1.0) < 1e-6
    assert abs(f.params["db_step"] - 20 * math.log10(SEMI)) < 1e-6
    assert f.err < 1e-3


def test_fit_geometric_rejects_non_positive_values() -> None:
    assert pc.fit_geometric([1.0, 0.0, 2.0]) is None
    assert pc.fit_prewarp([1.0, -1.0, 2.0]) is None


def test_fit_prewarp_recovers_tangent_table_and_u0() -> None:
    y = tan_table(187, clip=10.0)
    f = pc.fit_prewarp(y)
    assert f is not None
    assert abs(f.params["semitones_step"] - 1.0) < 1e-5
    assert abs(f.params["u0"] / 2.2e-5 - 1) < 1e-5
    assert f.err < 0.01
    plain = pc.fit_geometric(y)
    assert plain is not None and plain.err > 100  # tan bends away from a pure exponential


def test_fit_linear_and_power() -> None:
    lin = pc.fit_linear([1.98 - 0.0154687 * i for i in range(128)])
    assert lin.err < 1e-4 and abs(lin.params["b"] + 0.0154687) < 1e-9
    powf = pc.fit_power([(i / 99) ** 2.5 for i in range(100)])
    assert abs(powf.params["p"] - 2.5) < 1e-3 and powf.err < 1e-3


def test_best_fit_prefers_the_exact_model() -> None:
    assert pc.best_fit([0.5 * 1.03**i for i in range(50)]).model == "geometric"
    assert pc.best_fit([3.0 + 0.5 * i for i in range(50)]).model == "linear"
    assert pc.best_fit(tan_table(187, clip=10.0)).model == "prewarp"


def test_trim_guard_drops_repeated_tail() -> None:
    assert pc.trim_guard([1.0, 2.0, 3.0, 3.0, 3.0]) == [1.0, 2.0, 3.0]
    assert pc.trim_guard([1.0, 2.0]) == [1.0, 2.0]


def test_name_ratio_distinguishes_quarter_db_from_2_pow_one_24th() -> None:
    assert pc.name_ratio(10 ** (0.25 / 20)) == "0.25 dB"
    assert pc.name_ratio(2.0 ** (1 / 24)).startswith("2^(1/24)")
    assert pc.name_ratio(SEMI).startswith("2^(1/12)")
    assert pc.name_ratio(1.0123) == ""


def test_segments_splits_the_two_part_gain_table() -> None:
    segs = pc.segments(gain_table())
    exact = [s for s in segs if pc.is_exact(s)]
    assert len(exact) == 2
    assert abs(exact[0].fit.params["semitones_step"] - 0.5) < 1e-5  # 2^(1/24) per step
    assert exact[0].length == 354
    assert abs(exact[1].fit.params["ratio"] - 16 ** (1 / 127)) < 1e-6
    assert exact[1].length == 127


def test_segments_marks_a_noisy_fragment_as_not_exact() -> None:
    seg = pc.Segment(0, 20, pc.Fit("geometric", 2.9, {}))
    assert not pc.is_exact(seg)
    assert not pc.is_exact(pc.Segment(0, 8, pc.Fit("geometric", 0.0, {})))


def test_split_monotone_breaks_at_direction_reversal_and_jump() -> None:
    up = [0.01 * 1.05**i for i in range(60)]
    down = [1.0 * 0.97**i for i in range(60)]
    runs = pc.split_monotone(up + down, 0, 120, 24)
    assert runs == [(0, 60), (59, 120)] or runs == [(0, 60), (60, 120)]
    jump = [0.01 * 1.02**i for i in range(40)] + [0.0001 * 1.02**i for i in range(40)]
    assert len(pc.split_monotone(jump, 0, 80, 24)) == 2


def test_scan_finds_tables_with_guard_and_skips_denormal_junk() -> None:
    table = [0.002 * 1.059**i for i in range(128)]
    table += [table[-1]]  # interpolation guard entry
    vals = pc.load_f32(image(table, at=256))
    found = pc.scan(vals, 0, 4096, 64)
    assert len(found) == 1 and found[0].dm == 256 and len(found[0].values) == 128
    junk = [1e-39 * (i + 1) for i in range(100)]
    assert pc.scan(pc.load_f32(image(junk)), 0, 4096, 64) == []


def test_report_line_names_a_semitone_table() -> None:
    t = pc.Table(0xB0A63, tuple(0.01 * SEMI**k for k in range(80)))
    line = pc.report_line(t)
    assert "DM 0x" in line and "geometric" in line and "2^(1/12)" in line


def test_segment_lines_exact_filter_keeps_only_closed_forms() -> None:
    t = pc.Table(0, tuple(gain_table()))
    assert len(pc.segment_lines(t, exact=True)) == 2


def test_cli_scan_and_at(tmp_path, capsys) -> None:
    table = [0.002 * 1.059**i for i in range(100)]
    path = tmp_path / "sram.bin"
    path.write_bytes(image(table, at=512))
    assert pc.main(["scan", str(path), "--lo", "0", "--hi", "4096", "--min-len", "64"]) == 0
    assert "DM 0x00200" in capsys.readouterr().out
    assert pc.main(["at", str(path), "0x200", "100", "--segments"]) == 0
    assert "geometric" in capsys.readouterr().out
    empty = tmp_path / "empty.bin"
    empty.write_bytes(image([]))
    assert pc.main(["scan", str(empty), "--lo", "0", "--hi", "4096"]) == 1


def test_relation_spread_zero_for_exact_inverse_and_complement_maps() -> None:
    known = [3.0 * 1.04**i for i in range(128)]
    inv = [1 / (7.0 * k) for k in known]  # y = 1/(7 k)
    assert pc.relation_spread(inv, known, "1/y", "ratio") < 1e-9
    assert pc.relation_spread(inv, known, "y", "prod") < 1e-9
    coef = [1 - math.exp(-1 / (5.0 * k)) for k in known]  # smoothing coefficient of a time k
    assert pc.relation_spread(coef, known, "-ln(1-y)", "prod") < 1e-9
    assert pc.relation_spread(inv[::-1], known, "y", "prod", flip=True) < 1e-9
    assert pc.relation_spread(known, known[::-1], "y", "ratio") > 1.0
    assert pc.relation_spread([0.0] * 128, known, "y", "ratio") is None


def test_cli_relate(tmp_path, capsys) -> None:
    known = [3.0 * 1.04**i for i in range(128)]
    table = [1 / (7.0 * k) for k in known]
    img = tmp_path / "sram.bin"
    img.write_bytes(image(table, at=300))
    ref = tmp_path / "known.txt"
    ref.write_text(" ".join(str(k) for k in known))
    assert pc.main(["relate", str(img), str(ref), "--lo", "0", "--hi", "4096"]) == 0
    assert "1/y ratio" in capsys.readouterr().out
    ref.write_text(" ".join(str(1.0 + (i * 37) % 11) for i in range(128)))
    assert (
        pc.main(
            ["relate", str(img), str(ref), "--lo", "0", "--hi", "4096", "--max-spread", "0.001"]
        )
        == 1
    )
