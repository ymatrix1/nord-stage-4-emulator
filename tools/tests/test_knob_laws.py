"""Tests for the knob law folding (no firmware needed)."""

from __future__ import annotations

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import knob_laws as kl  # noqa: E402


def f32(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def row(eid: str, v: int, word: int, mode: str = "auto", dm: str = "0xe02ef") -> dict[str, str]:
    return {
        "id": eid,
        "name": "Edt_Syn_kTest",
        "layer": "4",
        "kind": "synth",
        "mode": mode,
        "value": str(v),
        "dm_word": dm,
        "word": f"{word:#010x}",
        "dest": "ORG/2/2",
    }


def test_where_names_strip_and_common_words() -> None:
    assert kl.where(0xE0003) == "strip 0 word 3"
    assert kl.where(0xE02E9) == "common 0 word 1"
    assert kl.where(0x1234) == "0x1234"


def test_fits_recognise_a_line_and_an_exponential() -> None:
    values = list(range(128))
    line = [f32(-0.5 + v / 127) for v in values]
    assert kl.linear_fit_error(values, line) < 1e-6
    expo = [f32(2.0 ** (v / 12)) for v in values]
    assert kl.exp_fit_error(values, expo) < 1e-5
    assert kl.linear_fit_error(values, expo) > 1e-3
    assert kl.exp_fit_error(values, line) is None  # negative floats
    assert kl.linear_fit_error([0, 1], [5, 5]) is None  # constant


def test_fold_marks_const_and_varying_words_and_q16_only() -> None:
    rows = [row("0x135", v, f32(-0.5 + v / 127)) for v in range(128)]
    rows += [row("0x133", v, 0x0612566E) for v in range(4)]
    # a small-range id that speaks only with the Q16.16 cell
    rows += [row("0x136", v, 0, "auto", "0xe0026") for v in range(4)]
    rows += [row("0x136", v, f32(v / 3), "q16", "0xe0026") for v in range(4)]
    laws = {(r["id"], r["mode"]): r for r in kl.fold(rows)}
    res = laws[("0x135", "auto")]
    assert res["kind"] == "varies" and res["v_lo"] == "0" and res["v_hi"] == "127"
    assert float(res["lin_err"]) < 1e-6
    assert laws[("0x133", "auto")]["kind"] == "const"
    assert laws[("0x133", "auto")]["words"] == "0x0612566e"
    assert laws[("0x136", "auto")]["q16_only"] == "False"
    assert laws[("0x136", "q16")]["q16_only"] == "True"


def test_series_round_trip_through_csv() -> None:
    rows = [row("0x135", v, f32(v / 127)) for v in range(128)]
    text = kl.laws_csv(kl.fold(rows))
    law = kl.read_laws(text)[0]
    series = kl.series_of(law)
    assert series[127] == f32(1.0) and len(series) == 128
    const = kl.series_of({"v_lo": "0", "v_hi": "3", "values": "0-3", "words": "0x00000001"})
    assert const == {0: 1, 1: 1, 2: 1, 3: 1}
    gap = kl.series_of({"v_lo": "1", "v_hi": "4", "values": "1,3,4", "words": "0x1 0x3 0x4"})
    assert gap == {1: 1, 3: 3, 4: 4}
    assert kl.value_list([0, 1, 2]) == "0-2" and kl.value_list([0, 2047, 4095]) == "0,2047,4095"
