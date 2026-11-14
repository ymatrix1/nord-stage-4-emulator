"""Unit tests for tools/fx_fit.py (synthetic samples)."""

from __future__ import annotations

import struct

import fx_fit as ff
import pytest


def word(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def test_f32_and_signed() -> None:
    assert ff.f32(0x3F800000) == 1.0
    assert ff.signed(0xFFFFFFFF) == -1
    assert ff.signed(5) == 5


@pytest.mark.parametrize(
    ("w", "expected"),
    [(0, True), (0x80000000, True), (0x3F800000, True), (0x3200, False), (0x7FFFFFFF, False)],
)
def test_is_float_word(w: int, expected: bool) -> None:
    assert ff.is_float_word(w) is expected


def test_constant_series() -> None:
    fit = ff.fit_series([0, 1, 2], [word(0.5)] * 3)
    assert fit.kind == "const" and fit.a == 0.5 and fit.is_float


def test_linear_float_series() -> None:
    values = [0, 16, 32, 64, 127]
    fit = ff.fit_series(values, [word(0.25 + 0.01 * v) for v in values])
    assert fit.kind == "linear" and fit.err < 1e-5
    assert fit.a == pytest.approx(0.25, rel=1e-5) and fit.b == pytest.approx(0.01, rel=1e-5)


def test_quadratic_series() -> None:
    values = [0, 15, 31, 63, 95, 127]
    fit = ff.fit_series(values, [word(1 + 5.72 * (v / 127) ** 2) for v in values])
    assert fit.kind == "quad" and fit.err < 1e-5
    assert fit.a == pytest.approx(1.0, abs=1e-4) and fit.c == pytest.approx(5.72 / 127**2, rel=1e-3)
    assert "v^2" in fit.describe()


def test_exponential_series_is_not_taken_for_linear() -> None:
    values = [0, 16, 32, 64, 96, 127]
    fit = ff.fit_series(values, [word(0.5 * 2 ** (v / 16)) for v in values])
    assert fit.kind == "exp"
    assert fit.a == pytest.approx(0.5, rel=1e-4) and fit.b == pytest.approx(1 / 16, rel=1e-4)


def test_integer_words_use_signed_values() -> None:
    values = [0, 1, 2, 3]
    fit = ff.fit_series(values, [(100 + 7 * v) & 0xFFFFFFFF for v in values])
    assert fit.kind == "linear" and not fit.is_float
    assert fit.a == pytest.approx(100) and fit.b == pytest.approx(7)


def test_table_fallback_and_short_series() -> None:
    values = [0, 1, 2, 3, 4]
    fit = ff.fit_series(values, [word(x) for x in (1.0, 5.0, 2.0, 9.0, 3.0)])
    assert fit.kind == "table" and fit.describe() == "table"
    assert ff.fit_series([0, 1], [word(1.0), word(2.0)]).kind == "table"


def test_describe_formats_each_kind() -> None:
    assert ff.Fit("const", 1.0, 0, 0, True).describe() == "1"
    assert "v" in ff.Fit("linear", 1.0, 2.0, 0, True).describe()
    assert "2^" in ff.Fit("exp", 1.0, 2.0, 0, True).describe()
