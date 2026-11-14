"""`kRvbAmt` closed form (tools/fx_forms.py) against the 128 captured word pairs."""

from __future__ import annotations

import csv
import struct
from pathlib import Path

import fx_forms as ff

CSV = Path(__file__).resolve().parents[2] / "docs" / "research" / "data" / "fx-reverb-amt.csv"


def f32(w: int) -> float:
    return struct.unpack("<f", struct.pack("<I", w))[0]


def rows() -> list[tuple[int, float, float]]:
    with CSV.open(encoding="utf-8", newline="") as fh:
        return [(int(r["value"]), f32(int(r["dry_word"])), f32(int(r["wet_word"])))
                for r in csv.DictReader(fh)]  # fmt: skip


def test_closed_form_matches_all_128_captured_pairs() -> None:
    data = rows()
    assert [v for v, _, _ in data] == list(range(128))
    for v, dry, wet in data:
        assert abs(ff.reverb_wet(v) - wet) < 1e-6, v
        assert abs(ff.reverb_dry(v) - dry) < 1e-6, v


def test_pair_is_equal_power_with_exact_end_points() -> None:
    for v in range(128):
        w, d = ff.reverb_wet(v), ff.reverb_dry(v)
        assert abs(w * w + d * d - 1.0) < 1e-12
    assert (ff.reverb_wet(0), ff.reverb_dry(0)) == (0.0, 1.0)
    assert (ff.reverb_wet(127), ff.reverb_dry(127)) == (1.0, 0.0)
    assert ff.reverb_wet(64) == 0.375  # x = 1/2: x^2 (2 - x)
