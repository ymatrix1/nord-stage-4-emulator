import math
import os
import struct
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import organ_tone_source as t  # noqa: E402


def _ladder(spread_cents: float, n: int = 92) -> list[int]:
    """Interleaved (copy0, copy1) ladder, 12-TET, second copy detuned by `spread_cents`."""
    out: list[int] = []
    for k in range(n):
        base = 25430.0 * 2 ** (k / 12)
        out.append(round(base))
        out.append(round(base * 2 ** (spread_cents / 1200)))
    return out


def test_table_stats_exact_ladder_has_no_spread():
    s = t.table_stats(_ladder(0.0))
    assert s["pairs"] == 92
    assert s["spread_rms"] == 0.0
    assert s["step_mean"] == pytest.approx(100.0, abs=0.05)
    assert s["step_sd"] < 0.1


def test_table_stats_detuned_pair_reports_spread():
    s = t.table_stats(_ladder(2.0))
    assert s["spread_rms"] == pytest.approx(2.0, abs=0.01)
    assert s["step_mean"] == pytest.approx(100.0, abs=0.05)


def test_table_stats_rejects_odd_length():
    with pytest.raises(ValueError):
        t.table_stats([1, 2, 3])


def test_cents_between_constant_offset():
    a = _ladder(0.0)
    b = [round(v * 2 ** (5 / 1200)) for v in a]
    mean, sd = t.cents_between(a, b)
    assert mean == pytest.approx(5.0, abs=0.05)
    assert sd < 0.1


def test_read_table_bounds_and_values():
    code = bytes(16) + struct.pack("<4I", 1, 2, 3, 4)
    assert t.read_table(code, t.LOAD_BASE + 16, 4) == [1, 2, 3, 4]
    with pytest.raises(ValueError):
        t.read_table(code, t.LOAD_BASE + 16, 5)


def _bl(from_addr: int, to_addr: int, op: int = 0xEB) -> bytes:
    imm = ((to_addr - (from_addr + 8)) // 4) & 0x00FFFFFF
    return struct.pack("<I", (op << 24) | imm)


def test_find_branches_forward_and_backward():
    base = t.LOAD_BASE
    target = base + 0x20
    code = bytearray(0x40)
    code[0x00:0x04] = _bl(base + 0x00, target)  # bl forward
    code[0x30:0x34] = _bl(base + 0x30, target, op=0xEA)  # b backward
    code[0x10:0x14] = struct.pack("<I", 0x1B000000 | 5)  # conditional bl, ignored
    assert t.find_branches(bytes(code), target) == [base + 0x00, base + 0x30]


def test_window_literals_core_and_word():
    base = t.LOAD_BASE
    code = struct.pack("<4I", 0x28309F14, 0x28A00004, 0xC0443EA8, 0x2838838C)
    got = t.window_literals(code, base, base + 16)
    assert got[0] == (base, 0x28309F14, 1, 0xC27C5)
    assert got[1] == (base + 4, 0x28A00004, 2, 0x80001)
    assert got[2][3] == 0xE20E3
    assert len(got) == 3


def test_main_tables_on_synthetic_image(tmp_path, capsys):
    a_off = t.TABLE_A_ADDR - t.LOAD_BASE
    b_off = t.TABLE_B_ADDR - t.LOAD_BASE
    image = bytearray(b_off + 4 * t.ELEMENTS)
    image[a_off : a_off + 4 * t.ELEMENTS] = struct.pack(f"<{t.ELEMENTS}I", *_ladder(2.0))
    image[b_off : b_off + 4 * t.ELEMENTS] = struct.pack(f"<{t.ELEMENTS}I", *_ladder(0.0))
    path = tmp_path / "code.bin"
    path.write_bytes(bytes(image))
    assert t.main(["tables", str(path)]) == 0
    out = capsys.readouterr().out
    assert "table A: copy spread rms 2.0" in out
    assert "table B: copy spread rms 0.00" in out
    assert math.isfinite(float(out.split("sd ")[-1].split()[0]))
