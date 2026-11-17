"""Tests for tools/synth_filter_type_link.py."""

import json

import synth_filter_type_link as m


def doc(selectors: list[int], stages: list[int]) -> dict:
    words = {
        str(i): {f"s{m.LAYER_STRIPS[i][0]}w1": f"{(0x86 - 2 * i) << 24 | stages[i]:#010x}"}
        for i in range(3)
    }
    fields = [{"selector": s, "drive": 0, "index": s} for s in selectors]
    return {"program": "P", "load": {"fields": fields, "org2_words": words}}


def test_stage_table_is_the_identity_order() -> None:
    assert m.STAGE_TABLE[0] == 0x1253BD and m.STAGE_TABLE[6] == 0x125B79
    assert len(m.STAGE_TABLE) == len(m.STAGE_NAMES) == 7


def test_matching_layers_report_one() -> None:
    rows = m.link_rows(doc([0, 3, 6], [0x1253BD, 0x1257B2, 0x125B79]))
    assert [r["match"] for r in rows] == [1, 1, 1]
    assert rows[1]["layer"] == "B" and rows[1]["stage"] == "0x1257b2"


def test_a_wrong_stage_is_flagged(tmp_path) -> None:  # noqa: ANN001
    rows = m.link_rows(doc([0, 1, 2], [0x1253BD, 0x1253BD, 0x12549D]))
    assert [r["match"] for r in rows] == [1, 0, 1]
    p = tmp_path / "x.load.json"
    p.write_text(json.dumps(doc([0, 0, 0], [0x1253BD] * 3)), "utf-8")
    out = tmp_path / "o.csv"
    assert m.main([str(p), "--csv", str(out)]) == 0
    assert out.read_text("utf-8").splitlines()[0] == ",".join(m.COLUMNS)
