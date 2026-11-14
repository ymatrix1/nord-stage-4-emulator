from __future__ import annotations

import arm_display_probe as dp


def test_window_of_names_the_peripheral() -> None:
    assert dp.window_of(0x01E12054) == "SPI1"
    assert dp.window_of(0x01C22024) == "I2C0"
    assert dp.window_of(0x62004014) == "EMIFA_CS3"
    assert dp.window_of(0x60000015) == "EMIFA_CS2"
    assert dp.window_of(0x80001DC0) is None


def test_top_writes_and_per_window_count_stores() -> None:
    writes = [("GPIO", 0x1E26044, 1)] * 3 + [("SPI1", 0x1E12038, 2)] + [("GPIO", 0x1E2606C, 3)]
    assert dp.top_writes(writes, 1) == [("GPIO", 0x1E26044, 3)]
    assert dp.per_window(writes) == {"GPIO": 4, "SPI1": 1}


def test_i2c_transactions_group_register_stores() -> None:
    log = [
        (dp.I2C_SAR, 0x50),
        (dp.I2C_CNT, 2),
        (dp.I2C_DXR, 0x1FF),  # only the low byte is data
        (dp.I2C_DXR, 0x07),
        (dp.I2C_MDR, 0x2620),  # start bit set: transaction ends
        (dp.I2C_SAR, 0x51),
        (dp.I2C_CNT, 1),
        (dp.I2C_MDR, 0x0800),  # no start bit: not a transaction
        (dp.I2C_MDR, 0x2E20),
    ]
    txs = dp.i2c_transactions(log)
    assert txs == [(0x50, 2, [0xFF, 0x07]), (0x51, 1, [])]
    assert dp.slaves(txs) == {0x50: 1, 0x51: 1}
