from __future__ import annotations

import csv
import io
import os
import struct
from pathlib import Path

import ns4_wave_cats as wc
import pytest

ROOT = Path(__file__).resolve().parents[2]
CATS_CSV = ROOT / "docs/research/data/synth-wave-categories.csv"
TYPES_CSV = ROOT / "docs/research/data/synth-wave-types.csv"
# The firmware is local only; point NS4_FW_CODE at `bank0_CODE.bin` when it is not under target/.
FW = Path(os.environ.get("NS4_FW_CODE", ROOT / "target/fw_unpacked/bank0_CODE.bin"))


def node(typ: str, cat: str, ci: int, name: str, wid: int, wi: int) -> dict:
    return {
        "type": typ, "category": cat, "cat_index": ci, "name": name,
        "wave_id": wid, "wave_index": wi,
    }  # fmt: skip


def read_rows(path: Path) -> list[dict]:
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def test_cond_evaluates_unsigned_and_signed_compare() -> None:
    assert wc._cond((5, 5), "eq") is True
    assert wc._cond((5, 6), "ne") is True
    assert wc._cond((1, 2), "lo") is True and wc._cond((2, 1), "lo") is False
    assert wc._cond((0xFFFFFFFF, 1), "lt") is True  # -1 < 1 signed
    assert wc._cond((0xFFFFFFFF, 1), "hs") is True  # but larger unsigned
    assert wc._cond(None, "") is True and wc._cond(None, "ne") is None


def test_rows_from_nodes_drops_duplicates_and_orders_by_wave_index() -> None:
    nodes = [
        node("Analog", "Shape Sine", 4, "Pulse", 13, 2),
        node("Analog", "Shape Sine", 4, "Saw", 38, 0),
        node("Analog", "Shape Sine", 4, "Square", 39, 1),
        node("Analog", "Shape Sine", 4, "Square", 39, 1),  # re-fired by the walk
        node("Wave", "Organ", 3, "Second", 0, 0),
    ]
    rows = wc.rows_from_nodes(nodes)
    assert [r["name"] for r in rows] == ["Saw", "Square", "Pulse", "Second"]


def test_rows_from_nodes_rejects_a_category_with_a_lost_index() -> None:
    nodes = [node("Analog", "Pure", 0, "Sine", 0, 0), node("Analog", "Pure", 0, "Saw", 2, 2)]
    with pytest.raises(RuntimeError, match="not 0..n-1"):
        wc.rows_from_nodes(nodes)


def test_to_csv_round_trips_the_columns() -> None:
    rows = wc.rows_from_nodes([node("Wave", "Keys", 4, "DX 1", 30, 0)])
    parsed = list(csv.DictReader(io.StringIO(wc.to_csv(rows))))
    assert parsed == [
        {
            "type": "Wave", "cat_index": "4", "category": "Keys",
            "wave_index": "0", "name": "DX 1", "wave_id": "30",
        }
    ]  # fmt: skip


def test_cmp_imm_decodes_only_cmp_with_an_immediate() -> None:
    fw = bytearray(8)
    struct.pack_into("<II", fw, 0, 0xE3560019, 0xE1A00000)
    assert wc._cmp_imm(bytes(fw), wc.BASE) == 0x19
    with pytest.raises(ValueError):
        wc._cmp_imm(bytes(fw), wc.BASE + 4)


def test_committed_table_has_the_firmware_shape() -> None:
    rows = read_rows(CATS_CSV)
    analog = [r for r in rows if r["type"] == "Analog"]
    wave = [r for r in rows if r["type"] == "Wave"]

    def sizes(rs: list[dict]) -> list[int]:
        out: dict[int, int] = {}
        for r in rs:
            out[int(r["cat_index"])] = out.get(int(r["cat_index"]), 0) + 1
        return [out[k] for k in sorted(out)]

    assert sizes(analog) == [7, 3, 9, 5, 7, 6, 5, 3]
    assert sizes(wave) == [6, 6, 14, 10, 10]  # `WAVE_CATEGORY_SIZES` of cbin/synth_osc.rs
    assert len({r["wave_id"] for r in wave}) == 46
    cats = {int(r["cat_index"]): r["category"] for r in analog}
    assert [cats[i] for i in range(8)] == [
        "Pure", "Sub Osc", "Sync", "Shape", "Shape Sine", "Multi", "Super", "Misc",
    ]  # fmt: skip


def test_analog_wave_ids_match_the_dsp_side_wave_type_table() -> None:
    """VERIFIED cross-check: the ARM name tree and the DSP P-table agree on every wave id.

    `synth-wave-types.csv` was derived from the DSP image (`tools/synth_wave_probe.py`); the tree
    ids come from the ARM routine `0xC01ADBC0`. Two independent sources, 45 of 45 equal.
    """
    dsp = {
        (int(r["cat"]), int(r["wave"])): int(r["type"])
        for r in read_rows(TYPES_CSV)
        if r["mode"] == "1" and r["bank"] == "0"
    }
    analog = [r for r in read_rows(CATS_CSV) if r["type"] == "Analog"]
    assert len(analog) == 45
    for r in analog:
        key = (int(r["cat_index"]), int(r["wave_index"]))
        assert dsp[key] == int(r["wave_id"]), r


def test_firmware_walk_reproduces_the_committed_table() -> None:
    """Needs the local firmware and capstone; CI skips it."""
    pytest.importorskip("capstone")
    if not FW.exists():
        pytest.skip("firmware image not available")
    rows = wc.rows_from_nodes(wc.walk(FW.read_bytes()))
    assert wc.to_csv(rows) == CATS_CSV.read_text(encoding="utf-8")
    shape = wc.fm_shape(FW.read_bytes())
    assert (shape["FM-H"]["groups"], shape["FM-H"]["items"]) == (5, 25)
    assert (shape["FM-I"]["groups"], shape["FM-I"]["items"]) == (5, 61)
    assert shape["FM-H"]["ratios"][:3] == [0.5, 1.0, 2.0]
