#!/usr/bin/env python3
"""Partial boot of the Nord Stage 4 host firmware on `tools/arm_emu.py`, plus the edit registry.

Three pieces, each independent of the others:

* **Peripheral models** the boot code waits on: the 64-bit RTOS tick at `0x80001DC0`
  (`install_tick`, advances on every read) and the GPIO handshake used by the DSP block-transfer
  routine `0xC006CCBC` (`install_gpio_echo`, input bits follow the last SET/CLR written).
* **`boot`**: runs the application constructor thunk `0xC025D524` with the stubs of
  `install_boot_stubs` for a budget and reports where it stopped (`BootResult`). It never completes
  (see `docs/research/arm-organ-messages.md` section 4 for the stop point).
* **Edit registry**: the real registration routine `0xC01A44C8` runs against a zeroed context and
  builds the 422 edit parameter descriptors (`build_edit_registry`, `read_edit_registry`,
  `registry_csv`); `construct_organ_layer` then runs the organ layer constructor `0xC0019BAC` and
  `subscribed_ids` reads back which edit ids it subscribed to.

    uv run --with unicorn python tools/arm_boot.py registry target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --csv docs/research/arm-edit-params.csv
    uv run --with unicorn python tools/arm_boot.py layer target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin
    uv run --with unicorn python tools/arm_boot.py boot target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --insns 400000000

Evidence and limits: `docs/research/arm-organ-messages.md`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_emu as ae

# ----- addresses ------------------------------------------------------------------------------
# VERIFIED: a `ldrd` of this 8-byte word is the delay-loop time base (`0xC0080684`, `0xC0080690`).
# Role (RTOS tick, 64 bit, counted up) ASSUMED from use.
TICK_ADDRESS = 0x80001DC0
# ASSUMED: TI AM1808 family GPIO block (AM1705 shares it): bank pair `n` registers start at
# `GPIO_BASE + 0x10 + 0x28 n`: DIR `+0`, OUT_DATA `+4`, SET_DATA `+8`, CLR_DATA `+0xC`, IN_DATA
# `+0x10`. The firmware code `0xC006CD70..0xC006CDF4` reads `[bank + 0xC]` (IN) and writes
# `[bank + 4]` / `[bank + 8]` (SET / CLR) from a pointer to OUT_DATA, which fits.
GPIO_BASE = 0x01E26000
GPIO_BANK_STRIDE = 0x28
GPIO_BANKS = 5
# VERIFIED (disassembly): application constructor thunk `bl 0xC0247B64` and the logger / SPI
# download routines that the boot has to skip.
APP_THUNK = 0xC025D524
LOGGER = 0xC0080314  # variadic `log(obj, args)`; kind 8 enters an endless LED blink loop
SPI_DOWNLOAD_BLOCK = 0xC0240548  # sends a block and polls the SPI slave for status
DSP_DOWNLOAD = 0xC0241C58
BOARD_CONFIG_QUERY = 0xC024BF70
MMU_ENABLE_MCR = 0xC0024624
OPERATOR_NEW = 0xC025D56C
# VERIFIED: registration routine (no direct caller) and organ layer constructor.
REGISTER_EDIT_PARAMS = 0xC01A44C8
ORGAN_LAYER_CTOR = 0xC0019BAC
# VERIFIED: global registry object; `[+0x1C]` holds the context that `0xC01A44C8` builds.
REGISTRY_GLOBAL = 0xC055B0A0
# VERIFIED: global pointer to the dispatcher that layer constructors register their client in.
DISPATCHER_GLOBAL = 0xC055B160

# Harness-only regions.
EDIT_CTX = 0x98000000
EDIT_CTX_SIZE = 0x100000
LAYER_BASE = 0x99000000
LAYER_SIZE = 0x100000
LOW_MEMORY = 0x10000

# VERIFIED (read back from the real registry, `arm-edit-params.csv`: ids, names, ranges match the
# registration call arguments, e.g. kDb1 = 0..8 default 8): entry offsets of one descriptor.
ENTRY_ID = 0x00
ENTRY_DOMAIN = 0x04
ENTRY_NAME_PTR = 0x08
ENTRY_MIN = 0x10
ENTRY_MAX = 0x14
ENTRY_DEFAULT = 0x18
ENTRY_WIDTHS = 0x70  # three words copied from the registration call
# VERIFIED: context layout: `[0]` count, `[1 + i]` entry pointers, `[0x8CC + 4 id]` index of an id.
CTX_INDEX_BASE = 0x8CC


@dataclass(frozen=True)
class EditParam:
    """One edit parameter descriptor read back from the registry."""

    index: int
    edit_id: int
    domain: int
    name: str
    minimum: int
    maximum: int
    default: int
    widths: tuple[int, int, int]

    def csv_row(self) -> str:
        w = ";".join(str(x) for x in self.widths)
        return (
            f"{self.index},{self.edit_id:#x},{self.domain:#x},{self.name},"
            f"{self.minimum},{self.maximum},{self.default},{w}"
        )


CSV_HEADER = "index,id,domain,name,min,max,default,w70;w74;w78"


def _signed(value: int) -> int:
    return value - (1 << 32) if value & 0x80000000 else value


# ----- peripheral models ----------------------------------------------------------------------
def install_tick(emu: ae.ArmEmu, step: int = 5000) -> dict[str, int]:
    """Advance the 64-bit tick at `TICK_ADDRESS` by `step` on every read.

    ASSUMED: the real tick is a hardware timer count; busy-wait loops only need it to grow.
    Returns the state dict (`t` = current value) so a test can observe it.
    """
    state = {"t": 0}

    def hook(uc, _access, _address, _size, _value, _user) -> None:  # noqa: ANN001
        state["t"] += step
        uc.mem_write(TICK_ADDRESS, struct.pack("<Q", state["t"]))

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_READ, hook, begin=TICK_ADDRESS, end=TICK_ADDRESS + 7)
    return state


def install_gpio_echo(emu: ae.ArmEmu) -> dict[str, int]:
    """GPIO banks where every IN_DATA bit follows the most recent SET (1) or CLR (0) write.

    The DSP block transfer toggles an output pin and waits for an input pin to equal it; the real
    DSP answers on another pin. ASSUMED: echoing the level into all input bits is enough for the
    handshake (it ends the wait loops of `0xC006CCBC`); other readers of IN_DATA see wrong pins.
    """
    emu.uc.mem_map(GPIO_BASE, 0x1000)
    state = {"writes": 0, "level": 0}

    def hook(uc, _access, address, _size, _value, _user) -> None:  # noqa: ANN001
        reg = (address - GPIO_BASE - 0x10) % GPIO_BANK_STRIDE
        bank = (address - GPIO_BASE - 0x10) // GPIO_BANK_STRIDE
        if 0 <= bank < GPIO_BANKS and reg in (0x8, 0xC):
            state["writes"] += 1
            state["level"] = 0xFFFFFFFF if reg == 0x8 else 0
            word = struct.pack("<I", state["level"])
            for b in range(GPIO_BANKS):
                uc.mem_write(GPIO_BASE + 0x10 + GPIO_BANK_STRIDE * b + 0x10, word)

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_WRITE, hook, begin=GPIO_BASE, end=GPIO_BASE + 0xFF)
    return state


def install_boot_stubs(emu: ae.ArmEmu) -> dict[str, int]:
    """Everything `boot` needs besides the static initialiser. Returns observable model state.

    Every stub is a FAKE (see `docs/research/arm-organ-messages.md` section 4):
    MMU enable skipped, `operator new` bump allocator, DSP image download and SPI block download
    return at once, the board configuration query reports "present, value 0", the logger returns
    (its kind 8 would blink an LED forever), device polls rotate through values.
    """
    emu.guard_null_calls()
    emu.bump_new(OPERATOR_NEW)
    emu.skip_instruction(MMU_ENABLE_MCR)
    emu.add_stub(DSP_DOWNLOAD, lambda _e: 0)
    emu.add_stub(SPI_DOWNLOAD_BLOCK, lambda _e: 0)
    emu.add_stub(LOGGER, lambda _e: 0)

    def board_config(e: ae.ArmEmu) -> int:
        e.write32(e.reg(2), 0)
        return 1

    emu.add_stub(BOARD_CONFIG_QUERY, board_config)
    tick = install_tick(emu)
    gpio = install_gpio_echo(emu)
    emu.enable_poll_breaker()
    return {"tick": tick["t"], "gpio_writes": gpio["writes"]}


@dataclass
class BootResult:
    """Where a bounded `boot` run stopped."""

    pc: int
    lr: int
    instructions: int
    error: str = ""
    null_calls: int = 0
    messages: int = 0
    history: list[int] = field(default_factory=list)

    @property
    def finished(self) -> bool:
        return self.pc == ae.SENTINEL


def boot(emu: ae.ArmEmu, max_insns: int = 400_000_000, chunk: int = 50_000) -> BootResult:
    """Run the application constructor thunk for at most `max_insns` instructions.

    Low memory (the null-call guard) is re-zeroed between chunks: code that stores through a null
    `this` would otherwise leave data that later null reads treat as vtables.
    """
    zeros = bytes(LOW_MEMORY)
    pc = APP_THUNK
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(14, ae.SENTINEL)
    done = 0
    error = ""
    while done < max_insns:
        try:
            emu.uc.emu_start(pc, ae.SENTINEL, count=chunk)
        except Exception as exc:  # unicorn raises UcError for faults
            error = str(exc)
            pc = emu.reg(15)
            break
        done += chunk
        pc = emu.reg(15)
        emu.uc.mem_write(0, zeros)
        if pc == ae.SENTINEL:
            break
    return BootResult(pc, emu.reg(14), done, error, len(emu.null_calls), len(emu.messages))


# ----- edit registry --------------------------------------------------------------------------
def build_edit_registry(emu: ae.ArmEmu, ctx: int = EDIT_CTX, layers: int = 6) -> int:
    """Run the registration routine `0xC01A44C8` into a zeroed context; returns the context.

    `layers` is the first word of the configuration struct (`0xC052B8BC`, default 6 in the
    image); the registration routine copies it to that global. Needs `bump_new(OPERATOR_NEW)`.
    """
    emu.uc.mem_map(ctx, EDIT_CTX_SIZE)
    cfg = ctx + EDIT_CTX_SIZE // 2
    emu.write32(cfg, layers)
    emu.call(REGISTER_EDIT_PARAMS, ctx, cfg, REGISTRY_GLOBAL, max_insns=50_000_000)
    return ctx


def _cstr(emu: ae.ArmEmu, address: int, limit: int = 0x80) -> str:
    raw = emu.read_bytes(address, limit).split(b"\0")[0]
    return raw.decode("latin1")


def read_edit_registry(emu: ae.ArmEmu, ctx: int = EDIT_CTX) -> list[EditParam]:
    """Decode every descriptor of a registry context."""
    out = []
    for i in range(emu.read32(ctx)):
        e = emu.read32(ctx + 4 + 4 * i)
        out.append(
            EditParam(
                index=i,
                edit_id=emu.read32(e + ENTRY_ID),
                domain=emu.read32(e + ENTRY_DOMAIN),
                name=_cstr(emu, emu.read32(e + ENTRY_NAME_PTR)),
                minimum=_signed(emu.read32(e + ENTRY_MIN)),
                maximum=_signed(emu.read32(e + ENTRY_MAX)),
                default=_signed(emu.read32(e + ENTRY_DEFAULT)),
                widths=(
                    emu.read32(e + ENTRY_WIDTHS),
                    emu.read32(e + ENTRY_WIDTHS + 4),
                    emu.read32(e + ENTRY_WIDTHS + 8),
                ),
            )
        )
    return out


def registry_csv(params: list[EditParam]) -> str:
    return CSV_HEADER + "\n" + "\n".join(p.csv_row() for p in params) + "\n"


# ----- organ layer ----------------------------------------------------------------------------
def construct_organ_layer(emu: ae.ArmEmu, layer: int = LAYER_BASE, dispatcher: int = 0) -> int:
    """Run the organ layer constructor `0xC0019BAC(layer, 0, 0, 0)` on zeroed memory.

    A zeroed dispatcher object is installed at `DISPATCHER_GLOBAL` first (the constructor appends
    its client to `[dispatcher + 0x14C]` / `[dispatcher + 0x150 + 4 n]`). Needs the registry and
    `bump_new(OPERATOR_NEW)`. Returns `layer`.
    """
    emu.uc.mem_map(layer, LAYER_SIZE)
    if not dispatcher:
        dispatcher = layer + LAYER_SIZE - 0x1000
    emu.write32(DISPATCHER_GLOBAL, dispatcher)
    emu.call(ORGAN_LAYER_CTOR, layer, 0, 0, 0, max_insns=50_000_000)
    return layer


def subscribed_ids(emu: ae.ArmEmu, layer: int = LAYER_BASE, limit: int = 0x400) -> list[int]:
    """Edit ids whose bit is set in the layer's subscription bitmap (words at `+0x64`)."""
    ids = []
    for word in range(limit // 32):
        bits = emu.read32(layer + 0x64 + 4 * word)
        ids.extend(32 * word + b for b in range(32) if bits >> b & 1)
    return ids


# ----- CLI ------------------------------------------------------------------------------------
def _fresh(code: Path, sram: Path | None) -> ae.ArmEmu:
    emu = ae.load_firmware(code, sram)
    ae.run_static_init(emu)
    emu.guard_null_calls()
    emu.bump_new(OPERATOR_NEW)
    return emu


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=["registry", "layer", "boot"])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--insns", type=int, default=400_000_000)
    ns = ap.parse_args(argv)
    if ns.command == "boot":
        emu = ae.load_firmware(ns.image, ns.sram)
        ae.run_static_init(emu)
        install_boot_stubs(emu)
        r = boot(emu, ns.insns)
        print(
            f"stopped pc={r.pc:#x} lr={r.lr:#x} after {r.instructions} instructions "
            f"nulls={r.null_calls} error={r.error!r}"
        )
        return 0
    emu = _fresh(ns.image, ns.sram)
    ctx = build_edit_registry(emu)
    params = read_edit_registry(emu, ctx)
    if ns.command == "registry":
        text = registry_csv(params)
        if ns.csv:
            ns.csv.write_text(text, encoding="utf-8")
        print(f"{len(params)} edit parameters")
        return 0
    emu.write32(REGISTRY_GLOBAL + 0x1C, ctx)
    layer = construct_organ_layer(emu)
    ids = subscribed_ids(emu, layer)
    print(f"{len(ids)} subscribed ids: {[hex(i) for i in ids]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
