#!/usr/bin/env python3
"""Front-panel key matrix of the host board: strobe `0x60000015`, reads `0x60000010` / `0x60000020`.

The panel scan is a state machine in SRAM (`0x800014E8`, one column per call, called from the
phase hook slot 2 installed by `0xC024DB74`; the ROM twin is `0xC024DA6C`). Table `0xC05298D8`
(11 entries of 5 words, `{column id, strobe, kind, mask, destination}`):

| strobe | column id | kind | mask (`word`) | port bits used |
|---:|---:|---:|---|---|
| `0x01` | 0 | key | `0x003F007F` | `0x10`: 6, `0x20`: 7 |
| `0x02` | 1 | key | `0x00FF007F` | `0x10`: 8, `0x20`: 7 |
| `0x04` | 2 | key | `0x00FF00FF` | 8 + 8 |
| `0x08` | 3 | key | `0x00FF00FF` | 8 + 8 |
| `0x10` | 4 | key | `0x00FF00FF` | 8 + 8 |
| `0x20` | 5 | key | `0x00FF003F` | `0x10`: 8, `0x20`: 6 |
| `0x40` | 6 | key | `0x00FF0000` | `0x10`: 8 |
| `0x80` | 0 | encoder | `0x000F003F` | `0x10`: 4, `0x20`: 6 |

Each scan step strobes one column, waits, reads `word = byte[0x20] | halfword[0x10] << 16`,
masks it and stores `~word`; a changed word is posted to the event queue (`0x80001D20` key,
`0x80001D40` encoder). The inversion means **a pressed key reads 0** (pull-ups, ASSUMED: the idle
level has to be 1 so that the stored word is 0; with the old all-zero answer every key looked
pressed). `KeyMatrix` answers idle `0xFF` and clears the bits of held keys.

Evidence and limits: `docs/research/arm-oled-render.md` section 7.
"""

from __future__ import annotations

from dataclasses import dataclass, field

STROBE_ADDRESS = 0x60000015
PORT_ADDRESSES = (0x60000010, 0x60000020)  # port 0 / port 1
# VERIFIED (table `0xC05298D8`): strobe values of the seven key columns, in scan order.
KEY_STROBES = (0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40)
ENCODER_STROBE = 0x80
# VERIFIED (same table): usable bits per `(strobe, port)`, from the masks in the module docstring.
USABLE_BITS: dict[tuple[int, int], int] = {
    (0x01, 0): 0x3F,
    (0x01, 1): 0x7F,
    (0x02, 0): 0xFF,
    (0x02, 1): 0x7F,
    (0x04, 0): 0xFF,
    (0x04, 1): 0xFF,
    (0x08, 0): 0xFF,
    (0x08, 1): 0xFF,
    (0x10, 0): 0xFF,
    (0x10, 1): 0xFF,
    (0x20, 0): 0xFF,
    (0x20, 1): 0x3F,
    (0x40, 0): 0xFF,
    (0x40, 1): 0x00,
    (0x80, 0): 0x0F,
    (0x80, 1): 0x3F,
}


def all_keys() -> list[tuple[int, int, int]]:
    """Every `(strobe, port, bit)` the scan can see on a key column, in scan order."""
    return [
        (strobe, port, bit)
        for strobe in KEY_STROBES
        for port in (0, 1)
        for bit in range(8)
        if USABLE_BITS[(strobe, port)] >> bit & 1
    ]


@dataclass
class KeyMatrix:
    """Idle-high key matrix: `read` answers the port byte for the column strobed last."""

    strobe: int = 0
    held: set[tuple[int, int, int]] = field(default_factory=set)

    def press(self, strobe: int, port: int, bit: int) -> None:
        if not USABLE_BITS.get((strobe, port), 0) >> bit & 1:
            raise ValueError(f"no key at strobe {strobe:#x} port {port} bit {bit}")
        self.held.add((strobe, port, bit))

    def release(self, strobe: int, port: int, bit: int) -> None:
        self.held.discard((strobe, port, bit))

    def release_all(self) -> None:
        self.held.clear()

    def port_byte(self, port: int) -> int:
        """Byte on `port` for the current strobe: all ones, held keys of that column cleared."""
        value = 0xFF
        for strobe, p, bit in self.held:
            if strobe == self.strobe and p == port:
                value &= ~(1 << bit)
        return value & 0xFF

    def read(self, address: int) -> int | None:
        """Byte at a panel port address (`0x60000010..13`, `0x60000020..23`), `None` otherwise."""
        for port, base in enumerate(PORT_ADDRESSES):
            if address == base:
                return self.port_byte(port)
            if base < address < base + 4:
                return 0xFF
        return None

    def write(self, address: int, value: int) -> None:
        if address == STROBE_ADDRESS:
            self.strobe = value & 0xFF
