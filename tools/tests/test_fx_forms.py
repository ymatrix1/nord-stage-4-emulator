"""Unit tests for tools/fx_forms.py, including a check against the committed capture series."""

from __future__ import annotations

import csv
import struct
from pathlib import Path

import fx_forms as ff
import pytest

DATA = Path(__file__).resolve().parents[2] / "docs" / "research" / "data" / "fx-param-fits.csv"


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def raw_series(slot: str, typ: str, ident: str, lst: str, entry: str, ctx: str) -> dict[int, int]:
    """Samples of one committed series as 32-bit words (`{value: word}`)."""
    with DATA.open(encoding="utf-8", newline="") as fh:
        for r in csv.DictReader(fh):
            same = (r["slot"], r["type"], r["id"], r["list"], r["entry"], r["ctx"])
            if same == (slot, typ, f"Edt_Effects_{ident}", lst, entry, ctx):
                return ff.parse_samples(r["samples"])
    raise AssertionError(f"series not found: {(slot, typ, ident, lst, entry, ctx)}")


def series(slot: str, typ: str, ident: str, lst: str, entry: str, ctx: str) -> dict[int, float]:
    """Samples of one committed series decoded as float32 (`{value: float}`)."""
    return {v: f32(w) for v, w in raw_series(slot, typ, ident, lst, entry, ctx).items()}


def close(got: float, want: float, span: float) -> bool:
    return abs(got - want) <= 1e-5 * max(span, 1e-9)


CASES = [
    ("Dly", "0", "kDlyMix", "0xc0279098", "0", "-27", ff.delay_wet),
    ("Dly", "0", "kDlyMix", "0xc0279098", "0", "4", ff.delay_dry),
    ("Dly", "0", "kDlyFbkAmt", "0xc0279098", "0", "-26", ff.delay_feedback),
    ("Dly", "0", "kDlyFbkAmt", "0xc0279098", "0", "-25", ff.delay_feedback),
    ("Cmp", "", "kCmpAmt", "0xc0441558", "0", "-27", ff.compressor_threshold),
    ("Cmp", "", "kCmpAmt", "0xc0441558", "0", "-28", ff.compressor_makeup),
    ("Fx1", "4", "kFx1Amt", "0xc02bb10c", "1", "-27", ff.crossfade_wet),
    ("Fx1", "4", "kFx1Amt", "0xc02bb10c", "1", "-26", ff.crossfade_dry),
]


@pytest.mark.parametrize(("slot", "typ", "ident", "lst", "entry", "ctx", "form"), CASES)
def test_closed_form_matches_committed_series(slot, typ, ident, lst, entry, ctx, form) -> None:
    got = series(slot, typ, ident, lst, entry, ctx)
    assert len(got) >= 3
    span = max(got.values()) - min(got.values())
    for v, word in got.items():
        assert close(float(form(v)), word, span), (ident, ctx, v, word, form(v))


@pytest.mark.parametrize("typ", ["2", "3"])
def test_phase_increment_matches_integer_words(typ: str) -> None:
    got = raw_series("Fx1", typ, "kFx1RateClkDiv", "0xc02bb030", "1", "-32")
    assert len(got) >= 3
    for v, word in got.items():
        assert word == ff.phase_increment(v)


def test_all_pass_amount_is_half_scale() -> None:
    wet = series("Fx1", "2", "kFx1Amt", "0xc02bb030", "1", "-26")
    dry = series("Fx1", "2", "kFx1Amt", "0xc02bb030", "1", "-25")
    for v, word in wet.items():
        assert close(ff.crossfade_wet(v, 254.0), word, 0.5)
    for v, word in dry.items():
        assert close(ff.crossfade_dry(v, 254.0), word, 0.5)


def test_amp_drive_phase_target() -> None:
    got = series("Amp", "7", "kAmpDrive", "0xc04438d8", "0", "-32")
    for v, word in got.items():
        assert close(ff.amp_drive(v), word, 1.0)


def test_forms_endpoints() -> None:
    assert ff.delay_wet(0) == 0 and ff.delay_wet(64) == 1 and ff.delay_wet(127) == 1
    assert (
        ff.delay_dry(0) == 1
        and ff.delay_dry(64) == 1
        and ff.delay_dry(127) == pytest.approx((1 / 64) ** 2)
    )
    assert ff.compressor_threshold(127) == pytest.approx(-36.0)
    assert ff.compressor_makeup(127) == pytest.approx(6.72)
    assert ff.phase_increment(0) == 12800 and ff.phase_increment(127) == 12800 + 16384 * 127


def test_parse_samples() -> None:
    assert ff.parse_samples("0:0x3f800000;64:0x00000001") == {0: 0x3F800000, 64: 1}
