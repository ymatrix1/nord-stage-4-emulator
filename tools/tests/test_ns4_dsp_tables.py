import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_dsp_tables as t  # noqa: E402


def pack(values):
    return struct.pack(f"<{len(values)}f", *values)


def test_classify_shapes():
    assert t.classify([1.0 + 0.5 * i for i in range(64)]) == "linear"
    assert t.classify([2.0 * 2 ** (i / 128) for i in range(129)]) == "exponential"
    assert t.classify([math.sin(i / 10) for i in range(64)]) == "curve"


def test_find_tables_splits_at_jumps_and_zero_padding():
    a = [2.0 * 2 ** (i / 64) for i in range(100)]
    b = [0.5 + 0.01 * i for i in range(80)]
    img = pack([0.0] * 4 + a + [0.0] * 4 + b + [0.0] * 4)
    found = t.find_tables(img, min_len=48)
    assert [(x.offset, x.count) for x in found] == [(16, 100), (16 + 400 + 16, 80)]
    assert found[0].kind == "exponential" and found[1].kind == "linear"


def test_find_tables_ignores_short_runs_and_integers():
    img = pack([1.0, 1.1, 1.2] + [0.0] * 8) + struct.pack("<8I", *range(1, 9))
    assert t.find_tables(img, min_len=48) == []


def test_main_prints_and_reports_empty(tmp_path, capsys):
    f = tmp_path / "img.bin"
    f.write_bytes(pack([0.0] + [1.0 + 0.1 * i for i in range(60)] + [0.0]))
    assert t.main([str(f)]) == 0
    assert "1 tables" in capsys.readouterr().out
    g = tmp_path / "empty.bin"
    g.write_bytes(bytes(64))
    assert t.main([str(g)]) == 1


def test_db_range_and_flag(tmp_path, capsys):
    tab = t.Table(0, 129, 0.001, 1.0235, "exponential")
    lo, hi, step = t.db_range(tab)
    assert abs(lo + 60.0) < 1e-9 and abs(hi - 0.2) < 0.01 and abs(step - (hi - lo) / 128) < 1e-12
    f = tmp_path / "img.bin"
    f.write_bytes(pack([0.0] + [0.001 * 2 ** (i / 16) for i in range(129)] + [0.0]))
    assert t.main([str(f), "--db"]) == 0
    assert "dB/step" in capsys.readouterr().out
