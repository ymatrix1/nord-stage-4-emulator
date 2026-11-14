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


# ASSUMED: the ten encoder bits of strobe `0x80` (mask `0x000F003F`) are five A / B pairs that the
# firmware decodes as quadrature; a pair is `(port, bit of A)` and B is the next bit. Which pair
# is which dial is found by turning it and reading the screen (`docs/research/arm-oled-render.md`).
ENCODER_PAIRS: tuple[tuple[int, int], ...] = ((0, 0), (0, 2), (1, 0), (1, 2), (1, 4))
# Gray cycle of `(A, B)` levels for one detent, positive direction (the other runs it backwards).
# EMPIRICAL: with this order the step decoder `0xC0082A9C` adds +1 three times per cycle; each
# state has to be held about 30 ticks because the scan reads column `0x80` only once per ~10 ticks.
GRAY_CYCLE: tuple[tuple[int, int], ...] = ((1, 1), (1, 0), (0, 0), (0, 1))


def encoder_steps(pair: int, detents: int) -> list[set[tuple[int, int, int]]]:
    """Held-key sets (low bits) that turn encoder `pair` by `detents` (negative: backwards)."""
    port, bit = ENCODER_PAIRS[pair]
    cycle = GRAY_CYCLE[1:] + GRAY_CYCLE[:1]  # from idle (1, 1) and back to it
    if detents < 0:
        cycle = cycle[2::-1] + cycle[3:]
    steps: list[set[tuple[int, int, int]]] = []
    for _ in range(abs(detents)):
        for a, b in cycle:
            held: set[tuple[int, int, int]] = set()
            if not a:
                held.add((ENCODER_STROBE, port, bit))
            if not b:
                held.add((ENCODER_STROBE, port, bit + 1))
            steps.append(held)
    return steps


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
