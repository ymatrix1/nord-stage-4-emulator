#!/usr/bin/env python3
"""Nord Stage 4 OLED: font objects, the panel byte stream and frame encoders (pure functions).

Everything here works on bytes: the OS v1.66 host image (`target/fw_unpacked/bank0_CODE.bin`, load
base `0xC0000000`, local only) for the fonts, and the write stream the host firmware sends to the
two OLED controllers (captured by `tools/arm_panel.py`). Nothing firmware-derived is stored in the repo.
Findings and evidence: `docs/research/arm-oled-render.md`, spec `docs/specs/arm-oled-render.md`.

Frame format (VERIFIED, `0xC0076924` flush + the controller init): 128 x 64, 1 bit per pixel, 8 pages
of 128 bytes; byte `page * 128 + x`, bit `i` (LSB first) is row `page * 8 + i`, 1 = lit. The same
layout is the firmware's own frame buffer (`[display + 0x18]`, `0x400` bytes), so frames captured
from the bus equal the buffer the draw routines wrote.

Font object (VERIFIED by `tools/arm_panel.py oracle`, which renders strings through the firmware's
own draw routine `0xC004064C` and compares): 16 bytes `{height u32, first u8 @+4, last u8 @+5,
table ptr u32 @+8, mask u32 @+12 = (1 << height) - 1}`; the table holds `last - first + 1` entries
`{column array ptr, width}`; a glyph is `width` little-endian u32 column words, bit `i` = row `i`
from the top. The advance of a glyph is its width (the column count includes the gap).
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass, field

LOAD_BASE = 0xC0000000
WIDTH, HEIGHT = 128, 64
PAGES = HEIGHT // 8
FRAME_BYTES = WIDTH * PAGES

# VERIFIED (run, `arm_panel.py fonts`): array of pointers to font objects, 17 entries, from the
# tallest (13 px) towards the smallest; entries 0..16 are every font the UI code references.
FONT_TABLE = 0xC029E5AC
FONT_TABLE_COUNT = 17
# VERIFIED (disassembly `0xC00769A0`, run): the 8 px font of the crash / debug text screen, not in
# the table above; first char 0x20, last 0x8C.
CRASH_FONT = 0xC0282E48
# VERIFIED (disassembly `0xC004064C`): the glyph blitter. `(text, font, plane, mask, x, y, limit)`,
# the last three on the stack; `plane` is the frame buffer, `mask` an equally sized buffer of 0xFF.
DRAW_TEXT = 0xC004064C
# VERIFIED (disassembly `0xC001B9D0`): `(text, font) -> width in pixels` of the first line.
TEXT_MEASURE = 0xC001B9D0


@dataclass(frozen=True)
class Font:
    """One firmware font: column bitmaps for the characters `first..=last`."""

    addr: int
    height: int
    first: int
    last: int
    glyphs: tuple[tuple[int, ...], ...] = field(repr=False)

    def has(self, char: int) -> bool:
        return self.first <= char <= self.last

    def columns(self, char: int) -> tuple[int, ...]:
        """Column words of `char`; characters outside the range use the first glyph (the space)."""
        return self.glyphs[char - self.first] if self.has(char) else self.glyphs[0]

    def advance(self, char: int) -> int:
        return len(self.columns(char))


def _u32(image: bytes, addr: int) -> int:
    return struct.unpack_from("<I", image, addr - LOAD_BASE)[0]


def parse_font(image: bytes, addr: int) -> Font:
    """Decode the font object at `addr` (image address, `0xC0......`)."""
    off = addr - LOAD_BASE
    height = struct.unpack_from("<I", image, off)[0]
    first, last = image[off + 4], image[off + 5]
    table = _u32(image, addr + 8)
    glyphs = []
    for i in range(last - first + 1):
        ptr, width = _u32(image, table + 8 * i), _u32(image, table + 8 * i + 4)
        glyphs.append(struct.unpack_from(f"<{width}I", image, ptr - LOAD_BASE))
    return Font(addr, height, first, last, tuple(glyphs))


def font_table(image: bytes) -> list[Font]:
    """All fonts of the UI font array `FONT_TABLE`."""
    return [parse_font(image, _u32(image, FONT_TABLE + 4 * i)) for i in range(FONT_TABLE_COUNT)]


def text_width(font: Font, text: str) -> int:
    """Pixel width of the first line, as `TEXT_MEASURE` (`0xC001B9D0`) computes it.

    The sum of the advances, minus one when the last character's final column is empty (the
    routine drops the trailing gap). Stops at NUL or newline.
    """
    chars = []
    for ch in text:
        if ch in "\0\n":
            break
        chars.append(ord(ch))
    if not chars:
        return 0
    total = sum(font.advance(c) for c in chars if font.has(c))
    last = chars[-1]
    if font.has(last):
        cols = font.columns(last)
        if cols and cols[-1] == 0:
            total -= 1
    return total


def blank_frame() -> bytearray:
    return bytearray(FRAME_BYTES)


def set_pixel(frame: bytearray, x: int, y: int, on: bool = True) -> None:
    if 0 <= x < WIDTH and 0 <= y < HEIGHT:
        i, bit = (y // 8) * WIDTH + x, 1 << (y % 8)
        frame[i] = (frame[i] | bit) if on else (frame[i] & ~bit & 0xFF)


def get_pixel(frame: bytes, x: int, y: int) -> int:
    return (frame[(y // 8) * WIDTH + x] >> (y % 8)) & 1


def draw_text(
    frame: bytearray, font: Font, text: str, x: int = 0, y: int = 0, limit: int = WIDTH
) -> int:
    """Reference rasteriser: OR the glyph columns into `frame`; returns the x after the text.

    Same geometry as the firmware's `0xC004064C` for printable ASCII (checked against it by
    `arm_panel.py oracle`). Columns at or beyond `limit` are clipped.
    """
    for ch in text:
        if ch in "\0\n":
            break
        for col in font.columns(ord(ch)):
            if x >= limit:
                return x
            for row in range(font.height):
                if (col >> row) & 1:
                    set_pixel(frame, x, y + row)
            x += 1
    return x


def to_ascii(frame: bytes, on: str = "#", off: str = ".") -> str:
    return "\n".join(
        "".join(on if get_pixel(frame, x, y) else off for x in range(WIDTH)) for y in range(HEIGHT)
    )


def to_pbm(frame: bytes, scale: int = 1) -> bytes:
    """Binary PBM (P4), lit pixels black on white as PBM expects (1 = black)."""
    w, h = WIDTH * scale, HEIGHT * scale
    rows = []
    for y in range(h):
        bits = [get_pixel(frame, x // scale, y // scale) for x in range(w)]
        bits += [0] * (-len(bits) % 8)
        rows.append(
            bytes(
                sum(b << (7 - i) for i, b in enumerate(bits[k : k + 8]))
                for k in range(0, len(bits), 8)
            )
        )
    return f"P4\n{w} {h}\n".encode() + b"".join(rows)


def _chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def to_png(frame: bytes, scale: int = 1, lit: int = 255, dark: int = 0) -> bytes:
    """8-bit grey PNG of the frame (lit pixels `lit`), `scale` times enlarged."""
    w, h = WIDTH * scale, HEIGHT * scale
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw.extend(lit if get_pixel(frame, x // scale, y // scale) else dark for x in range(w))
    head = struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", head)
        + _chunk(b"IDAT", zlib.compress(bytes(raw)))
        + _chunk(b"IEND", b"")
    )


# ----- the panel byte stream ---------------------------------------------------------------------

# VERIFIED (run: init stream of `0xC00770BC`; commands with a parameter byte): SSD1309-class
# 128 x 64 controller commands that take one parameter byte. The sequence the firmware sends is
# `FD 12, AE, D5 A0, A8 3F, D3 20, 60, C8, A1, DA 12, 81 DF, D9 82, DB 34, A4, A6, 20 00`.
PARAM_COMMANDS = frozenset({0x20, 0x81, 0xA8, 0xD3, 0xD5, 0xD9, 0xDA, 0xDB, 0xFD})


class Ssd13xx:
    """Command / data stream of one OLED controller to its display RAM, as the firmware drives it.

    Models page and column addressing (`B0..B7`, `00..0F` / `10..1F`, memory mode `20 xx`);
    everything else is recorded in `commands` and ignored. A frame is complete after 1024 data
    bytes without a command in between (the firmware's flush writes the whole buffer in one burst).
    """

    def __init__(self) -> None:
        self.ram = bytearray(FRAME_BYTES)
        self.page = 0
        self.col = 0
        self.mode = 2  # power-on default of the SSD130x family: page addressing
        self.commands: list[int] = []
        self.frames: list[bytes] = []
        self._pending: int | None = None
        self._burst = 0

    def command(self, byte: int) -> None:
        self.commands.append(byte)
        self._burst = 0
        if self._pending is not None:
            if self._pending == 0x20:
                self.mode = byte & 3
            self._pending = None
        elif byte in PARAM_COMMANDS:
            self._pending = byte
        elif 0xB0 <= byte <= 0xB7:
            self.page = byte & 7
        elif byte <= 0x0F:
            self.col = (self.col & 0xF0) | byte
        elif 0x10 <= byte <= 0x1F:
            self.col = (self.col & 0x0F) | ((byte & 0x0F) << 4)

    def data(self, byte: int) -> None:
        self.ram[(self.page % PAGES) * WIDTH + self.col % WIDTH] = byte
        self.col += 1
        if self.col == WIDTH:
            self.col = 0
            if self.mode != 2:  # horizontal addressing wraps into the next page
                self.page = (self.page + 1) % PAGES
        self._burst += 1
        if self._burst == FRAME_BYTES:
            self.frames.append(bytes(self.ram))
            self._burst = 0


# VERIFIED (run, display transport objects read after the boot): EMIFA CS2 (`0x6000....`) carries
# both panels. A latch byte at `0x60000025` holds the data / command line of each controller;
# each controller has one byte port. Display 0: port `0x60000017`, latch mask `0x04`; display 1:
# port `0x60000027`, latch mask `0x20`. Latch bit set = data byte, clear = command byte
# (`0xC0072790` command writer clears the bit and stores the latch, `0xC00727F0` data writer sets).
LATCH_ADDRESS = 0x60000025
PANEL_PORTS: dict[int, tuple[int, int]] = {0x60000017: (0, 0x04), 0x60000027: (1, 0x20)}


class PanelLink:
    """Both controllers behind the EMIFA CS2 latch; feed it every store to `0x6000....`."""

    def __init__(self) -> None:
        self.latch = 0
        self.panels = (Ssd13xx(), Ssd13xx())

    def write(self, address: int, value: int) -> None:
        value &= 0xFF
        if address == LATCH_ADDRESS:
            self.latch = value
        elif address in PANEL_PORTS:
            index, mask = PANEL_PORTS[address]
            panel = self.panels[index]
            (panel.data if self.latch & mask else panel.command)(value)

    def frames(self, index: int, unique: bool = False) -> list[bytes]:
        out = self.panels[index].frames
        if not unique:
            return list(out)
        seen: set[bytes] = set()
        return [f for f in out if not (f in seen or seen.add(f))]
