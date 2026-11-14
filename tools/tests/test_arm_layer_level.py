"""Unit tests for tools/arm_layer_level.py (pure law and a hand-assembled ARM stub, no firmware)."""

from __future__ import annotations

import arm_layer_level as ll
import pytest


def test_float_bits_round_trip() -> None:
    assert ll.float_bits(1.0) == 0x3F800000
    assert ll.float_bits(-0.05) == 0xBD4CCCCD
    assert ll.bits_float(0x3F800000) == 1.0


def test_expected_word_is_the_squared_level() -> None:
    assert ll.expected_word(127) == 1.0
    assert ll.expected_word(0) == ll.MUTE
    assert ll.expected_word(100) == pytest.approx((100 / 127) ** 2, rel=1e-6)
    assert ll.expected_word(64) == pytest.approx(0.2539525, rel=1e-6)


def test_every_case_names_a_handler_and_its_selectors() -> None:
    assert {h for h, _ in ll.CASES} == {ll.PIANO_HANDLER, ll.ORGAN_HANDLER}
    assert {i for _, i in ll.CASES} == {0xB1, 0xB2, 0xB7, 0xB8}
    for selectors, kinds in ll.CASES.values():
        assert selectors and kinds


def test_run_handler_reports_a_missing_send() -> None:
    class Fake:
        """Emulator double: the handler never reaches the sender."""

        def __init__(self) -> None:
            self._level_scratch = True
            self.mem: dict[int, int] = {}

        def write_bytes(self, address: int, data: bytes) -> None:
            pass

        def write32(self, address: int, value: int) -> None:
            self.mem[address] = value

        def add_stub(self, address: int, fn) -> None:  # noqa: ANN001
            self.stub = (address, fn)

        def call(self, *args: int) -> int:
            return 0

    result = ll.run_handler(Fake(), ll.PIANO_HANDLER, 0xB7, 100)
    assert result.stored is None and result.sent is None


def test_run_handler_reads_the_stored_word() -> None:
    class Fake:
        def __init__(self) -> None:
            self._level_scratch = True
            self.mem: dict[int, int] = {}
            self.regs = [0, 0, 0]

        def write_bytes(self, address: int, data: bytes) -> None:
            pass

        def write32(self, address: int, value: int) -> None:
            self.mem[address] = value

        def read32(self, address: int) -> int:
            return self.mem[address]

        def reg(self, n: int) -> int:
            return self.regs[n]

        def add_stub(self, address: int, fn) -> None:  # noqa: ANN001
            self.sender = fn

        def call(self, *args: int) -> int:
            table = self.mem[args[1] + 0x98]
            self.regs = [table, 3, 5]  # (table, layer, slot_word)
            self.mem[table + 4 * (7 * 3 + 5 + 0x34)] = ll.float_bits(0.25)
            self.sender(self)
            return 0

    result = ll.run_handler(Fake(), ll.ORGAN_HANDLER, 0xB1, 64)
    assert result.stored == 0.25 and result.sent is not None
