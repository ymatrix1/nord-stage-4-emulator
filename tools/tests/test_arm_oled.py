"""Unit tests for tools/arm_oled.py: a synthetic font image, the panel byte stream and encoders."""

from __future__ import annotations

import struct
import zlib

import arm_oled as ao

BASE = ao.LOAD_BASE
FONT_AT = BASE + 0x100
TABLE_AT = BASE + 0x200
COLS_AT = BASE + 0x300


def synthetic_image() -> bytes:
    """A 3-row font for `A..C`: A has 2 columns, B one empty column, C three columns."""
    glyphs = [(0b011, 0b111), (0b000,), (0b001, 0b010, 0b100)]
    image = bytearray(0x400)
    struct.pack_into("<IBBxxII", image, 0x100, 3, ord("A"), ord("C"), TABLE_AT, 0b111)
    ptr = COLS_AT
    for i, cols in enumerate(glyphs):
        struct.pack_into("<II", image, 0x200 + 8 * i, ptr, len(cols))
        struct.pack_into(f"<{len(cols)}I", image, ptr - BASE, *cols)
        ptr += 4 * len(cols)
    return bytes(image)


def test_parse_font_reads_header_table_and_columns() -> None:
    font = ao.parse_font(synthetic_image(), FONT_AT)
    assert (font.height, font.first, font.last) == (3, 65, 67)
    assert font.columns(ord("A")) == (0b011, 0b111)
    assert font.columns(ord("C")) == (1, 2, 4)
    assert font.advance(ord("B")) == 1
    assert font.has(ord("B")) and not font.has(ord("D"))


def test_characters_outside_the_range_use_the_first_glyph() -> None:
    font = ao.parse_font(synthetic_image(), FONT_AT)
    assert font.columns(ord("Z")) == font.columns(ord("A"))


def test_text_width_sums_advances_and_trims_an_empty_last_column() -> None:
    font = ao.parse_font(synthetic_image(), FONT_AT)
    assert ao.text_width(font, "AC") == 5
    assert ao.text_width(font, "AB") == 2  # 2 + 1, minus the empty final column of B
    assert ao.text_width(font, "") == 0
    assert ao.text_width(font, "A\nC") == 2  # first line only
    assert ao.text_width(font, "AZ") == 2  # Z is out of range: no advance, as TEXT_MEASURE


def test_draw_text_sets_column_bits_top_row_first() -> None:
    font = ao.parse_font(synthetic_image(), FONT_AT)
    frame = ao.blank_frame()
    end = ao.draw_text(frame, font, "AC", x=1, y=2)
    assert end == 6
    # glyph A: column 0 has rows 0,1 lit, column 1 rows 0,1,2
    assert [ao.get_pixel(frame, 1, 2 + r) for r in range(3)] == [1, 1, 0]
    assert [ao.get_pixel(frame, 2, 2 + r) for r in range(3)] == [1, 1, 1]
    # glyph C starts at x=3: a diagonal
    assert [ao.get_pixel(frame, 3 + c, 2 + c) for c in range(3)] == [1, 1, 1]


def test_draw_text_clips_at_the_limit() -> None:
    font = ao.parse_font(synthetic_image(), FONT_AT)
    frame = ao.blank_frame()
    assert ao.draw_text(frame, font, "AAA", x=0, y=0, limit=3) == 3
    assert ao.get_pixel(frame, 3, 0) == 0


def test_page_layout_matches_the_firmware_frame_buffer() -> None:
    frame = ao.blank_frame()
    ao.set_pixel(frame, 5, 9)  # page 1, bit 1
    assert frame[1 * ao.WIDTH + 5] == 0b10
    ao.set_pixel(frame, 5, 9, on=False)
    assert frame[1 * ao.WIDTH + 5] == 0
    ao.set_pixel(frame, 200, 200)  # off screen: ignored
    assert not any(frame)


def test_pbm_png_and_ascii_encoders() -> None:
    frame = ao.blank_frame()
    ao.set_pixel(frame, 0, 0)
    head = b"P4\n128 64\n"
    pbm = ao.to_pbm(frame)
    assert pbm.startswith(head)
    assert pbm[len(head)] == 0x80  # first pixel is the top bit of the first byte
    assert len(ao.to_pbm(frame, scale=2)) == len(b"P4\n256 128\n") + 32 * 128
    png = ao.to_png(frame, scale=2)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    idat = png.index(b"IDAT")
    size = struct.unpack(">I", png[idat - 4 : idat])[0]
    raw = zlib.decompress(png[idat + 4 : idat + 4 + size])
    assert len(raw) == 128 * (256 + 1)
    assert raw[1:3] == bytes([255, 255])  # pixel (0,0) enlarged to 2 x 2
    assert ao.to_ascii(frame).splitlines()[0].startswith("#.")


def test_ssd13xx_addressing_and_command_log() -> None:
    chip = ao.Ssd13xx()
    for byte in (0xFD, 0x12, 0xAE, 0x20, 0x00, 0xB3, 0x05, 0x12):
        chip.command(byte)
    # unlock + parameter, display off, memory mode 0, page 3, column low nibble 5, high nibble 2
    assert chip.mode == 0 and chip.page == 3 and chip.col == 0x25
    chip.data(0xAA)
    assert chip.ram[3 * 128 + 0x25] == 0xAA
    assert chip.commands.count(0x12) == 2


def test_parameter_bytes_are_not_taken_for_addresses() -> None:
    chip = ao.Ssd13xx()
    chip.command(0xD5)
    chip.command(0xB4)  # clock divider parameter, not "page 4"
    chip.command(0x81)
    chip.command(0x0F)  # contrast parameter, not "column low nibble"
    assert chip.page == 0 and chip.col == 0


def test_horizontal_mode_wraps_into_the_next_page_and_page_mode_does_not() -> None:
    horizontal = ao.Ssd13xx()
    horizontal.command(0x20)
    horizontal.command(0x00)
    for _ in range(129):
        horizontal.data(1)
    assert (horizontal.page, horizontal.col) == (1, 1)
    paging = ao.Ssd13xx()
    for _ in range(129):
        paging.data(1)
    assert (paging.page, paging.col) == (0, 1)


def test_a_1024_byte_burst_is_one_frame() -> None:
    chip = ao.Ssd13xx()
    for b in (0x20, 0x00, 0xB0, 0x00, 0x10):
        chip.command(b)
    for i in range(1024):
        chip.data(i & 0xFF)
    assert len(chip.frames) == 1
    assert chip.frames[0][130] == 130 and chip.frames[0][1023] == 0xFF
    chip.data(7)  # the next burst starts again at the wrapped address
    assert len(chip.frames) == 1


def test_panel_link_routes_ports_and_latch_per_display() -> None:
    link = ao.PanelLink()
    link.write(ao.LATCH_ADDRESS, 0x00)  # command line
    link.write(0x60000017, 0xB2)
    link.write(0x60000027, 0xB5)
    assert link.panels[0].page == 2 and link.panels[1].page == 5
    link.write(ao.LATCH_ADDRESS, 0x04)  # display 0 data line only
    link.write(0x60000017, 0x55)
    link.write(0x60000027, 0xB6)  # display 1 is still in command mode
    assert link.panels[0].ram[2 * 128] == 0x55
    assert link.panels[1].page == 6
    link.write(0x60000099, 1)  # unknown address: ignored


def test_unique_frames_drop_repeats() -> None:
    link = ao.PanelLink()
    link.write(ao.LATCH_ADDRESS, 0)
    for b in (0x20, 0x00):
        link.write(0x60000017, b)
    for _ in range(3):
        link.write(ao.LATCH_ADDRESS, 0)
        for b in (0xB0, 0x00, 0x10):
            link.write(0x60000017, b)
        link.write(ao.LATCH_ADDRESS, 0x04)
        for _ in range(1024):
            link.write(0x60000017, 0x11)
    assert len(link.frames(0)) == 3
    assert len(link.frames(0, unique=True)) == 1
    assert link.frames(1) == []
