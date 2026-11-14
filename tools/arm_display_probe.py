#!/usr/bin/env python3
"""Where does the host firmware talk to hardware that could be a display? (local firmware only)

Boots the OS v1.66 host image to its main loop with the harness of `tools/arm_steady.py`, logs every
store to the peripheral windows that could carry OLED traffic (SPI0/1, I2C0, UART1/2, GPIO, the
EMIFA chip selects) and counts the calls of the UI stubs (`arm_boot_full.TEXT_MEASURE`,
`EVENT_INSERT`). The report answers one question: does a screen ever reach a pixel or command
stream in the emulated steady state? Findings: `docs/research/oled-strings-vs-firmware.md` s5.

    uv run --with unicorn python tools/arm_display_probe.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin [--ticks 100000000]

The summary helpers are pure functions (tested without the firmware).
"""

from __future__ import annotations

import argparse
import collections
import sys
from collections.abc import Iterable
from pathlib import Path

# Peripheral windows of the AM1705 (names as in `tools/arm_xref.py`); EMIFA chip selects are split
# in the low (panel / FPGA registers, `0x6000....`) and the DSP window (`0x6200....`).
WINDOWS: dict[str, tuple[int, int]] = {
    "SPI0": (0x01C41000, 0x01C42000),
    "SPI1": (0x01E12000, 0x01E13000),
    "I2C0": (0x01C22000, 0x01C23000),
    "UART1": (0x01D0C000, 0x01D0D000),
    "UART2": (0x01D0D000, 0x01D0E000),
    "GPIO": (0x01E26000, 0x01E27000),
    "EMIFA_CS2": (0x60000000, 0x62000000),
    "EMIFA_CS3": (0x62000000, 0x64000000),
}
# TI I2C register offsets (ASSUMED from the TRM layout, corroborated by the 0x50..0x53 slave
# addresses the firmware writes to ICSAR).
I2C_CNT, I2C_SAR, I2C_DXR, I2C_MDR = 0x14, 0x1C, 0x20, 0x24
I2C_STT = 0x2000

Write = tuple[str, int, int]  # (window, address, pc)


def window_of(address: int) -> str | None:
    """Name of the peripheral window that contains `address`."""
    for name, (lo, hi) in WINDOWS.items():
        if lo <= address < hi:
            return name
    return None


def top_writes(writes: Iterable[Write], n: int = 10) -> list[tuple[str, int, int]]:
    """The `n` most written `(window, address, count)`."""
    count: collections.Counter[tuple[str, int]] = collections.Counter((w, a) for w, a, _ in writes)
    return [(w, a, c) for (w, a), c in count.most_common(n)]


def per_window(writes: Iterable[Write]) -> dict[str, int]:
    """Store count per window."""
    out: collections.Counter[str] = collections.Counter(w for w, _, _ in writes)
    return dict(out)


def i2c_transactions(log: Iterable[tuple[int, int]]) -> list[tuple[int, int, list[int]]]:
    """Group I2C register stores `(offset, value)` into `(slave, count, data bytes)` transactions.

    A transaction ends at a store of `ICMDR` with the start bit set.
    """
    out: list[tuple[int, int, list[int]]] = []
    sar = cnt = 0
    data: list[int] = []
    for off, val in log:
        if off == I2C_SAR:
            sar = val
        elif off == I2C_CNT:
            cnt = val
        elif off == I2C_DXR:
            data.append(val & 0xFF)
        elif off == I2C_MDR and val & I2C_STT:
            out.append((sar, cnt, data))
            data = []
    return out


def slaves(txs: Iterable[tuple[int, int, list[int]]]) -> dict[int, int]:
    """Transactions per I2C slave address."""
    return dict(collections.Counter(sar for sar, _, _ in txs))


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--ticks", type=int, default=100_000_000, help="instructions after the boot")
    ns = ap.parse_args(argv)
    import arm_boot_full as bf
    import arm_steady as st

    emu = bf.prepare(ns.image, ns.sram)
    calls: collections.Counter[str] = collections.Counter()
    for name, addr in (("TEXT_MEASURE", bf.TEXT_MEASURE[0]), ("EVENT_INSERT", bf.EVENT_INSERT)):

        def stub(_e: object, name: str = name) -> int:
            calls[name] += 1
            return 0

        emu.add_stub(addr, stub)
    writes: list[Write] = []
    i2c: list[tuple[int, int]] = []

    def on_write(_uc, _access, address, _size, value, _user) -> None:  # noqa: ANN001
        name = window_of(address)
        if name is not None:
            writes.append((name, address, emu.reg(15)))
            if name == "I2C0":
                i2c.append((address - WINDOWS["I2C0"][0], value & 0xFFFFFFFF))

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_WRITE, on_write, begin=0x01C00000, end=0x64000000)
    pc, n, err = st.boot_to_main_loop(emu)
    print(f"main loop: pc={pc:#x} after {n} instructions {err!r}")
    print("boot stores per window:", per_window(writes))
    print("UI stub calls during boot:", dict(calls))
    txs = i2c_transactions(i2c)
    print(f"I2C0: {len(txs)} transactions, per slave {slaves(txs)}")
    for w, a, c in top_writes(writes, 8):
        print(f"  {w} {a:#x} x{c}")
    writes.clear()
    calls.clear()
    st.enable_real_timer(emu)
    st.run_ticking(emu, ns.ticks)
    print(f"steady state ({ns.ticks} insns): stores {per_window(writes)} stubs {dict(calls)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
