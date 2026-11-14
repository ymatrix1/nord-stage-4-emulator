#!/usr/bin/env python3
"""I2C0 controller and the four 24xx EEPROMs of the host board, for the ARM harness (local only).

The program memory of the Nord Stage 4 is not a file on a card: the file system layer of the host
firmware (`0xC0077DF4` stat, `0xC0096264` directory walk) sits on **four 64 KB I2C EEPROMs at
slave addresses `0x50..0x53`** (`docs/research/arm-oled-render.md` s6). The old harness left the
I2C registers as plain memory, so the poll breaker fed the driver `0xFFFFFFFF` words and every
stat returned a garbage size (`0xFFFFFFDC`), which the Program screen shows as `Write error`.

`I2cBus` is a register-level model of the TI I2C block (`0x01C22000`): `ICSAR`, `ICCNT`, `ICDXR`,
`ICDRR`, `ICMDR`, `ICSTR`. It implements what the driver `0xC005B09C..0xC005B2B0` uses: master
transmit / receive with or without a stop, `ICSTR` ready flags (`ARDY`, `RRDY`, `XRDY`, `NACK`,
`BB`) and the 16-bit address write followed by a restart for random reads of `Eeprom`.

    bus = I2cBus()
    bus.write(I2C_SAR, 0x50); bus.write(I2C_CNT, 2)
    bus.write(I2C_MDR, MDR_STT | MDR_STP | MDR_MST | MDR_TRX)
    bus.write(I2C_DXR, 0x12); bus.write(I2C_DXR, 0x34)      # address 0x1234

Register offsets, bit positions and the interaction with the driver are ASSUMED from the TI
AM1808 TRM layout and corroborated by the driver's own polling masks (`tst #0xc`, `tst #0x400`,
`orr #0x800`, `orr #0x8000`); the EEPROM size (64 KB, 16-bit address) is VERIFIED by the addresses
the firmware writes (`0x0000..0xFFFF` on every slave).
"""

from __future__ import annotations

from dataclasses import dataclass, field

I2C_BASE = 0x01C22000
I2C_SIZE = 0x1000
# ASSUMED: TI I2C register offsets (AM1808 TRM), see the module docstring.
I2C_STR, I2C_CNT, I2C_DRR, I2C_SAR, I2C_DXR, I2C_MDR = 0x08, 0x14, 0x18, 0x1C, 0x20, 0x24
# ICSTR bits.
STR_NACK, STR_ARDY, STR_RRDY, STR_XRDY, STR_BB = 0x02, 0x04, 0x08, 0x10, 0x1000
# ICMDR bits.
MDR_NACKMOD, MDR_STT, MDR_STP, MDR_MST, MDR_TRX = 0x8000, 0x2000, 0x0800, 0x0400, 0x0200
EEPROM_BASE_ADDRESS = 0x50
EEPROM_COUNT = 4
EEPROM_SIZE = 0x10000
# ASSUMED: 24LC512-style page (128 bytes); a write past the page end wraps inside the page.
EEPROM_PAGE = 128


@dataclass
class Eeprom:
    """A 64 KB 24xx device: 16-bit address pointer, sequential read, paged write."""

    data: bytearray = field(default_factory=lambda: bytearray(b"\xff" * EEPROM_SIZE))
    pointer: int = 0

    def set_address(self, high: int, low: int) -> None:
        self.pointer = ((high << 8) | low) & 0xFFFF

    def write_byte(self, value: int) -> None:
        self.data[self.pointer] = value & 0xFF
        page = self.pointer & ~(EEPROM_PAGE - 1)
        self.pointer = page | ((self.pointer + 1) & (EEPROM_PAGE - 1))

    def read_byte(self) -> int:
        value = self.data[self.pointer]
        self.pointer = (self.pointer + 1) & 0xFFFF
        return value


@dataclass
class I2cBus:
    """Master-side register model of the I2C0 block with four EEPROM slaves."""

    eeproms: list[Eeprom] = field(default_factory=lambda: [Eeprom() for _ in range(EEPROM_COUNT)])
    status: int = STR_XRDY
    sar: int = 0
    cnt: int = 0
    mdr: int = 0
    drr: int = 0xFF
    busy: bool = False
    _tx: int = 0  # bytes written in the current transmit phase
    _rx_left: int = 0
    _slave: Eeprom | None = None
    log: list[tuple[int, int, int]] = field(default_factory=list)  # (slave, kind, count)

    # ----- helpers ---------------------------------------------------------------------------
    def _eeprom(self) -> Eeprom | None:
        i = self.sar - EEPROM_BASE_ADDRESS
        return self.eeproms[i] if 0 <= i < EEPROM_COUNT else None

    def _release(self) -> None:
        self.busy = False
        self.mdr &= ~(MDR_MST | MDR_STP | MDR_STT)
        self.status &= ~STR_BB

    def _finish(self) -> None:
        self.status |= STR_ARDY
        self.status &= ~(STR_XRDY | STR_RRDY)
        if self.mdr & MDR_STP:
            self._release()

    def _next_rx(self) -> None:
        if self._rx_left > 0 and self._slave is not None:
            self.drr = self._slave.read_byte()
            self.status |= STR_RRDY
        else:
            self.status &= ~STR_RRDY

    # ----- register interface ----------------------------------------------------------------
    def write(self, offset: int, value: int) -> None:
        value &= 0xFFFFFFFF
        if offset == I2C_STR:
            self.status &= ~(value & ~STR_BB)
        elif offset == I2C_CNT:
            self.cnt = value & 0xFFFF
        elif offset == I2C_SAR:
            self.sar = value & 0x3FF
        elif offset == I2C_DXR:
            self._transmit(value & 0xFF)
        elif offset == I2C_MDR:
            self._set_mode(value)

    def _set_mode(self, value: int) -> None:
        was_stop = bool(value & MDR_STP) and not (self.mdr & MDR_STP)
        self.mdr = value & 0xFFFF
        if value & MDR_STT:
            self._start()
        elif was_stop and self.busy and self._rx_left == 0 and self._tx == 0:
            self._release()

    def _start(self) -> None:
        self.busy = True
        self.status |= STR_BB
        self.status &= ~(STR_ARDY | STR_NACK | STR_RRDY | STR_XRDY)
        self._slave = self._eeprom()
        self._tx = 0
        if self._slave is None:
            self.status |= STR_NACK | STR_ARDY
            self._release()
            return
        if self.mdr & MDR_TRX:
            self.log.append((self.sar, 1, self.cnt))
            self._rx_left = 0
            if self.cnt == 0:  # address probe (ACK polling): the driver waits for ARDY
                self._finish()
            else:
                self.status |= STR_XRDY
        else:
            self.log.append((self.sar, 0, self.cnt))
            self._rx_left = self.cnt
            self._next_rx()
            if self._rx_left == 0:
                self._finish()

    def _transmit(self, byte: int) -> None:
        slave = self._slave
        if not self.busy or not (self.mdr & MDR_TRX) or slave is None:
            return
        if self._tx == 0:
            self._addr_high = byte
        elif self._tx == 1:
            slave.set_address(self._addr_high, byte)
        else:
            slave.write_byte(byte)
        self._tx += 1
        if self._tx >= self.cnt:
            self._finish()

    def read(self, offset: int) -> int:
        if offset == I2C_STR:
            return self.status
        if offset == I2C_DRR:
            return self._receive()
        if offset == I2C_MDR:
            return self.mdr
        if offset == I2C_CNT:
            return self.cnt
        return 0

    def _receive(self) -> int:
        value = self.drr
        if self._rx_left > 0:
            self._rx_left -= 1
            self.status &= ~STR_RRDY
            if self._rx_left == 0:
                self._finish()
            else:
                self._next_rx()
        return value

    _addr_high: int = 0


def install_i2c(emu) -> I2cBus:  # noqa: ANN001 - ArmEmu, needs unicorn
    """Attach `I2cBus` to the I2C0 window of `emu` (register reads and stores are intercepted)."""
    bus = I2cBus()
    try:
        emu.uc.mem_map(I2C_BASE, I2C_SIZE)
    except Exception:  # already mapped by the poll breaker or an earlier call  # noqa: BLE001, S110
        pass
    last = I2C_BASE + I2C_SIZE - 1

    def on_read(uc, _access, address, _size, _value, _user) -> None:  # noqa: ANN001
        off = address - I2C_BASE
        if off in (I2C_STR, I2C_DRR, I2C_MDR, I2C_CNT):
            uc.mem_write(address & ~3, bus.read(off & ~3).to_bytes(4, "little"))

    def on_write(_uc, _access, address, _size, value, _user) -> None:  # noqa: ANN001
        bus.write((address - I2C_BASE) & ~3, value)

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_READ, on_read, begin=I2C_BASE, end=last)
    emu.uc.hook_add(emu._u.UC_HOOK_MEM_WRITE, on_write, begin=I2C_BASE, end=last)
    return bus
