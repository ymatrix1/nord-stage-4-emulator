#!/usr/bin/env python3
"""Boot the Nord Stage 4 host firmware past `0xC00FC9F4` on `tools/arm_emu.py`.

`arm_boot.boot_until` stopped at the edit controller constructor because the next UI step
(`0xC0168658`) dereferenced objects the harness had not built. Two root causes, both fixed here:

* **Static initialisers ran with every `bl` skipped** (`arm_emu.run_static_init`). The C++ init
  array `0xC052B890..0xC052B8A0` has four functions (`INIT_ARRAY`); run for real they populate
  tables such as the edit id table at `0xC0572EC0` (ids `0xE8, 0xEB, ...`) that client constructors
  read. `run_init_array` runs them with the peripheral stubs of `arm_boot.install_boot_stubs`.
* **UI / display objects** (font metrics, the timer event queue) hang off hardware that is not
  modelled. `install_ui_stubs` documents each no-op.

    uv run --with unicorn python tools/arm_boot_full.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --insns 3000000000

Evidence and limits: `docs/research/arm-organ-messages.md` section 9.
"""

from __future__ import annotations

import argparse
import collections
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_boot as ab
import arm_emu as ae

# VERIFIED (disassembly of `__rt_lib_init` 0xC026BA04: loops over `[0xC052B890, 0xC052B8A0)` and
# calls each word): the four static initialisers. Running them reproduces the id table at
# `0xC0572EC0` (first words 0xE8, 0xEB) that the skipped-`bl` harness left zero.
INIT_ARRAY = (0xC00002A4, 0xC024D1A0, 0xC024DC64, 0xC025DBBC)
# VERIFIED (run): table read by client constructors such as 0xC0149A00 (first ids).
ID_TABLE = 0xC0572EC0

# ASSUMED: timer event queue insert (sorted by time at `[ev + 8]`, called from the UI setup
# 0xC0168658 through 0xC001ED84 with a descriptor taken from `[0xC055F278] + 0x28 + 4 i`, a table
# that no code path fills before this point). No-op: the queue only schedules display refresh.
EVENT_INSERT = 0xC001B67C
# ASSUMED: text width / line measure over a font glyph table `[font + 8]` that is null because
# the LCD driver is not modelled. Returns 0 (empty text).
TEXT_MEASURE = (0xC001B9D0,)
# VERIFIED (disassembly 0xC02281BC): SPI master transfer `(dev, a, b, 1, &out)` whose fifth
# argument (stack) receives the slave's answer; the caller at 0xC0228200 then allocates that many
# 0x14-byte objects. The poll breaker feeds `0x3F0077` (4.1 M objects, exhausts the heap). Stub:
# answer 0 (ASSUMED: "nothing to allocate" is a legal slave answer).
SPI_TRANSFER = 0xC0066E64
HIGH_PAGE = 0xFFFF0000


def run_init_array(emu: ae.ArmEmu, per_call: int = 300_000_000) -> None:
    """Run the four static initialisers for real (no `bl` skipping)."""
    for fn in INIT_ARRAY:
        emu.set_reg(13, ae.STACK_TOP - 0x1000)
        emu.set_reg(14, ae.SENTINEL)
        emu.uc.emu_start(fn, ae.SENTINEL, count=per_call)
    emu.install_dsp_ports()  # the initialisers zero BSS words that the fake comm objects use


def install_ui_stubs(emu: ae.ArmEmu, extra: tuple[int, ...] = ()) -> None:
    """No-op the display / timer-queue routines whose objects hang off unmodelled hardware."""
    emu.add_stub(EVENT_INSERT, lambda _e: 0)

    def spi_transfer(e: ae.ArmEmu) -> int:
        out = e.read32(e.reg(13))  # fifth argument: result pointer
        if out:
            e.write32(out, 0)
        return 0

    emu.add_stub(SPI_TRANSFER, spi_transfer)
    for addr in (*TEXT_MEASURE, *extra):
        emu.add_stub(addr, lambda _e: 0)
    # A null font table dereference computes `0xFFFFFFFC`; give it a zero page instead of a fault.
    emu.uc.mem_map(HIGH_PAGE, 0x10000)


@dataclass
class FullBoot:
    """Where a `boot_full` run stopped, with a sampled program counter histogram."""

    pc: int
    lr: int
    instructions: int
    error: str = ""
    null_calls: int = 0
    hot: list[tuple[int, int]] = field(default_factory=list)

    @property
    def finished(self) -> bool:
        return self.pc == ae.SENTINEL


def boot_full(emu: ae.ArmEmu, max_insns: int = 3_000_000_000, chunk: int = 50_000) -> FullBoot:
    """`arm_boot.boot` with a sampled pc histogram (one sample per chunk) for spin detection."""
    zeros = bytes(ab.LOW_MEMORY)
    samples: collections.Counter[int] = collections.Counter()
    pc = ab.APP_THUNK
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(14, ae.SENTINEL)
    done, error = 0, ""
    while done < max_insns:
        try:
            emu.uc.emu_start(pc, ae.SENTINEL, count=chunk)
        except Exception as exc:  # unicorn raises UcError for faults
            error = str(exc)
            pc = emu.reg(15)
            break
        done += chunk
        pc = emu.reg(15)
        samples[pc & ~3] += 1
        emu.uc.mem_write(0, zeros)
        if pc == ae.SENTINEL:
            break
    hot = samples.most_common(8)
    return FullBoot(pc, emu.reg(14), done, error, len(emu.null_calls), hot)


def prepare(code: Path, sram: Path | None) -> ae.ArmEmu:
    """Firmware image + peripheral stubs + real static initialisers + UI stubs."""
    emu = ae.load_firmware(code, sram)
    ab.install_boot_stubs(emu)
    install_ui_stubs(emu)
    run_init_array(emu)
    return emu


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--insns", type=int, default=3_000_000_000)
    ns = ap.parse_args(argv)
    emu = prepare(ns.image, ns.sram)
    r = boot_full(emu, ns.insns)
    print(
        f"stopped pc={r.pc:#x} lr={r.lr:#x} after {r.instructions} instructions "
        f"nulls={r.null_calls} error={r.error!r}"
    )
    print("hot:", [(hex(a), n) for a, n in r.hot])
    return 0


if __name__ == "__main__":
    sys.exit(main())
