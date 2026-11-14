"""Unit tests for tools/arm_i2c.py: the I2C0 register model with four 24xx EEPROMs."""

from __future__ import annotations

import arm_i2c as ai


def _write_phase(bus: ai.I2cBus, slave: int, data: list[int], stop: bool) -> None:
    bus.write(ai.I2C_SAR, slave)
    bus.write(ai.I2C_CNT, len(data))
    bus.write(ai.I2C_MDR, ai.MDR_STT | ai.MDR_MST | ai.MDR_TRX | (ai.MDR_STP if stop else 0))
    assert bus.read(ai.I2C_STR) & ai.STR_XRDY
    for byte in data:
        bus.write(ai.I2C_DXR, byte)


def _read_phase(bus: ai.I2cBus, slave: int, count: int) -> list[int]:
    bus.write(ai.I2C_SAR, slave)
    bus.write(ai.I2C_CNT, count)
    bus.write(ai.I2C_MDR, ai.MDR_STT | ai.MDR_MST | ai.MDR_STP)
    out = []
    for _ in range(count):
        assert bus.read(ai.I2C_STR) & ai.STR_RRDY
        out.append(bus.read(ai.I2C_DRR))
    return out


def test_blank_eeprom_reads_ff_and_finishes_with_ardy_and_bus_release() -> None:
    bus = ai.I2cBus()
    _write_phase(bus, 0x50, [0x12, 0x34], stop=False)
    assert bus.read(ai.I2C_STR) & ai.STR_ARDY
    assert bus.busy
    assert _read_phase(bus, 0x50, 3) == [0xFF] * 3
    assert bus.read(ai.I2C_STR) & ai.STR_ARDY
    assert not bus.busy and not bus.read(ai.I2C_STR) & ai.STR_BB
    assert not bus.read(ai.I2C_MDR) & ai.MDR_MST


def test_write_then_random_read_returns_the_stored_bytes() -> None:
    bus = ai.I2cBus()
    _write_phase(bus, 0x51, [0x01, 0x02, 0xAA, 0xBB, 0xCC], stop=True)
    assert not bus.busy
    _write_phase(bus, 0x51, [0x01, 0x03], stop=False)
    assert _read_phase(bus, 0x51, 2) == [0xBB, 0xCC]
    assert bus.eeproms[1].data[0x0102] == 0xAA
    assert bus.eeproms[0].data[0x0102] == 0xFF  # the slaves do not alias


def test_page_write_wraps_inside_the_page() -> None:
    bus = ai.I2cBus()
    _write_phase(bus, 0x52, [0x00, 0x7F, 1, 2], stop=True)
    assert bus.eeproms[2].data[0x7F] == 1
    assert bus.eeproms[2].data[0x00] == 2
    assert bus.eeproms[2].data[0x80] == 0xFF


def test_unknown_slave_sets_nack_and_releases_the_bus() -> None:
    bus = ai.I2cBus()
    bus.write(ai.I2C_SAR, 0x20)
    bus.write(ai.I2C_CNT, 1)
    bus.write(ai.I2C_MDR, ai.MDR_STT | ai.MDR_MST | ai.MDR_TRX | ai.MDR_STP)
    assert bus.read(ai.I2C_STR) & ai.STR_NACK
    assert not bus.busy


def test_status_write_clears_only_the_bits_written_as_one() -> None:
    bus = ai.I2cBus()
    _write_phase(bus, 0x53, [0, 0], stop=True)
    assert bus.read(ai.I2C_STR) & ai.STR_ARDY
    bus.write(ai.I2C_STR, ai.STR_ARDY)
    assert not bus.read(ai.I2C_STR) & ai.STR_ARDY


def test_transaction_log_records_slave_direction_and_count() -> None:
    bus = ai.I2cBus()
    _write_phase(bus, 0x50, [0, 0], stop=False)
    _read_phase(bus, 0x50, 1)
    assert bus.log == [(0x50, 1, 2), (0x50, 0, 1)]


def test_eeprom_pointer_wraps_at_64k() -> None:
    e = ai.Eeprom()
    e.set_address(0xFF, 0xFF)
    e.data[0xFFFF] = 7
    assert e.read_byte() == 7 and e.pointer == 0


def test_zero_length_write_is_an_address_probe_that_completes_at_once() -> None:
    bus = ai.I2cBus()
    bus.write(ai.I2C_SAR, 0x52)
    bus.write(ai.I2C_CNT, 0)
    bus.write(ai.I2C_MDR, ai.MDR_STT | ai.MDR_MST | ai.MDR_TRX | ai.MDR_STP)
    assert bus.read(ai.I2C_STR) & ai.STR_ARDY
    assert not bus.read(ai.I2C_STR) & ai.STR_NACK
    assert not bus.busy
