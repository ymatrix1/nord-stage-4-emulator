"""Unit tests for tools/arm_xref.py using hand-assembled ARM words."""

from __future__ import annotations

import struct

import arm_xref as ax

BASE = ax.LOAD_BASE


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def ldr_pc(rd: int, offset: int) -> int:
    """LDR rd, [pc, #offset] (always)."""
    up = 1 if offset >= 0 else 0
    return 0xE51F0000 | (up << 23) | (rd << 12) | (abs(offset) & 0xFFF)


def mov_imm(rd: int, imm: int) -> int:
    return 0xE3A00000 | (rd << 12) | imm


def bl(src: int, dst: int) -> int:
    return 0xEB000000 | (((dst - (src + 8)) >> 2) & 0xFFFFFF)


def test_peripheral_name_windows() -> None:
    assert ax.peripheral_name(0x01C41000) == "SPI0"
    assert ax.peripheral_name(0x01E12000) == "SPI1"
    assert ax.peripheral_name(0x01E26010) == "GPIO"
    assert ax.peripheral_name(0x62000010) == "EMIFA CS3"
    assert ax.peripheral_name(0x12345678) is None


def test_ror32_and_mov_imm() -> None:
    assert ax.ror32(0xFF, 8) == 0xFF000000
    # mov r1, #4 ; mov r3, #0x200 (imm 2 ror 24)
    assert ax.decode_mov_imm(mov_imm(1, 4)) == (1, 4)
    assert ax.decode_mov_imm(0xE3A03C02) == (3, 0x200)
    assert ax.decode_mov_imm(0xE1A00001) is None  # mov r0, r1 (register form)


def test_ldr_literal_decode() -> None:
    insn = ldr_pc(3, 0x10)
    assert ax.decode_ldr_literal(insn, 0x1000) == 0x1000 + 8 + 0x10
    assert ax.decode_ldr_literal(ldr_pc(3, -4), 0x1000) == 0x1000 + 8 - 4
    assert ax.decode_ldr_literal(0xE5934000, 0x1000) is None  # ldr r4,[r3]


def test_bl_and_blx_decode() -> None:
    assert ax.decode_bl(bl(0x100, 0x400), 0x100) == 0x400
    assert ax.decode_bl(bl(0x400, 0x100), 0x400) == 0x100
    assert ax.decode_bl(0xE12FFF35, 0) is None
    assert ax.decode_blx_reg(0xE12FFF35) == 5
    assert ax.decode_blx_reg(0xE1A00000) is None


def test_bl_callers() -> None:
    target = BASE + 0x40
    data = words(0, bl(BASE + 4, target), 0, bl(BASE + 12, target), 0, 0, 0, 0, 0, 0, 0, 0)
    assert ax.bl_callers(data, target) == [BASE + 4, BASE + 12]
    assert ax.bl_callers(data, target, end=8) == [BASE + 4]


def test_literal_refs_finds_pool_and_loader() -> None:
    # 0x00: ldr r0,[pc,#0] -> literal at 0x08 ; 0x08 holds the value
    data = words(ldr_pc(0, 0), 0, 0x62000010, 0)
    refs = ax.literal_refs(data, 0x62000010)
    assert refs == [(BASE + 8, [BASE])]


def test_mmio_literals_filters_unaligned_register_values() -> None:
    data = words(
        0x01C41000, 0x01C41004, 0x62000010, 0x5, 0x68000000, 0x66666667, 0x62D0A1B2, 0x61626300
    )
    hits = ax.mmio_literals(data)
    assert [(h[1], h[2]) for h in hits] == [
        (0x01C41000, "SPI0"),
        (0x62000010, "EMIFA CS3"),
        (0x68000000, "EMIFA control"),
    ]
    assert hits[0][0] == BASE


def test_download_calls_tracks_index_pointer_length() -> None:
    # r3 = 0xAE34 (pool), r1 = 4, r2 = ptr (pool), blx r5, then a second call with new values.
    code = [
        ldr_pc(3, 0x20 - 8),  # at 0x00 -> literal at 0x20
        mov_imm(1, 4),  # 0x04
        ldr_pc(2, 0x24 - 0x0C - 8 + 0),  # at 0x08 -> literal at 0x24 (offset 0x24-0x08-8)
        0xE12FFF35,  # 0x0C blx r5
        ldr_pc(3, 0x28 - 0x10 - 8),  # 0x10 -> 0x28
        mov_imm(1, 3),  # 0x14
        ldr_pc(2, 0x2C - 0x18 - 8),  # 0x18 -> 0x2C
        0xE12FFF35,  # 0x1C blx r5
    ]
    code[2] = ldr_pc(2, 0x24 - 0x08 - 8)
    data = words(*code, 0xAE34, 0xC051B574, 0x1DBF4, 0xC04FD980)
    calls = ax.download_calls(data, BASE, BASE + 0x20)
    assert calls == [
        (BASE + 0x0C, 4, 0xC051B574, 0xAE34),
        (BASE + 0x1C, 3, 0xC04FD980, 0x1DBF4),
    ]


def test_download_calls_ignores_incomplete_calls_and_allows_unknown_index() -> None:
    data = words(mov_imm(1, 1), 0xE12FFF35, 0, 0)
    assert ax.download_calls(data, BASE, BASE + 8) == []
    # r2 and r3 known, r1 never set: index is None
    data = words(ldr_pc(3, 0x10 - 8), ldr_pc(2, 0x14 - 4 - 8), 0xE12FFF35, 0, 0x28410, 0xC044667C)
    assert ax.download_calls(data, BASE, BASE + 12) == [(BASE + 8, None, 0xC044667C, 0x28410)]


def test_main_commands(tmp_path, capsys) -> None:
    target = BASE + 0x20
    img = tmp_path / "img.bin"
    img.write_bytes(words(bl(BASE, target), 0x01C41000, 0, 0, 0, 0, 0, 0, 0, 0))
    assert ax.main(["callers", str(img), hex(target)]) == 0
    assert "1 call sites" in capsys.readouterr().out
    assert ax.main(["mmio", str(img)]) == 0
    assert "SPI0" in capsys.readouterr().out
    assert ax.main(["literal", str(img), "0x1c41000"]) == 0
    assert "pool" in capsys.readouterr().out
    assert ax.main(["downloads", str(img), hex(BASE), hex(BASE + 8)]) == 0
