#!/usr/bin/env python3
"""Run the Nord Stage 4 host firmware boot to its steady state (the main event loop) and keep it.

`arm_boot_full.boot_full` ran the boot for a fixed instruction budget and only sampled the pc.
This module stops **at the main loop** (`MAIN_LOOP`), so every client the boot builds exists, and
adds `snapshot` / `restore` so that experiments (id sweeps, message captures) start from the
steady state in a second instead of replaying 0.43 B instructions (about 30 s).

    uv run --with unicorn python tools/arm_steady.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --save target/steady.snap

RTOS findings (what the firmware has instead of a preemptive scheduler) and the evidence:
`docs/research/arm-organ-messages.md` section 10.
"""

from __future__ import annotations

import argparse
import pickle
import sys
import zlib
from pathlib import Path

import arm_boot as ab
import arm_boot_full as bf
import arm_emu as ae

# VERIFIED (run, `docs/research/arm-organ-messages.md` section 10): first instruction of the
# main event loop body (`mov sl, #0` after the loop set-up of the routine at `0xC00A0140`). The
# real boot reaches it after about 432 M instructions with the stubs of `arm_boot_full.prepare`.
MAIN_LOOP = 0xC00A0248
# VERIFIED (disassembly 0xC00A0250..0xC00A0264): the loop reads the current context index
# `[0x8000C000]` and the ring buffer table `0xC05613F4` (`ring = table[idx + [0xC0539570]]`,
# `head = [ring]`, `tail = [ring + 4]`). Empty ring: the idle / tick branch at `0xC00A03FC`.
CONTEXT_INDEX = 0x8000C000
RING_TABLE = 0xC05613F4
# VERIFIED (disassembly of `0xC00A05D4`): posting to a queue brackets the update with a write of
# `0x80000000 | level` to this register, which is the AM1705 AINTC host interrupt nesting level
# register (`0xFFFEE000 + 0x1104`): an interrupt-level critical section, not a task switch.
NESTING_LEVEL_REG = 0xFFFEF104
CHUNK = 200_000
# VERIFIED (run, `intc_attach` 0xC005012C hooked during the boot): handler table indexed by AINTC
# system interrupt number, filled by `0xC005012C(dev, handler, channel)` with `irq = [dev + 4]`.
# Observed after the boot: 14 -> 0x80000AD8, 22 -> 0x800007D8, 58 -> 0x80000770, 61 -> 0x80000434.
IRQ_TABLE = 0xC055F674
# VERIFIED (disassembly): 22 is the 1 kHz timer (`0xC005019C` programs `[timer + 0x24]` from
# 24 MHz / 0x5DC0 = 1 ms) whose handler `0x800007D8` increments the 64-bit tick `0x80001DC0`,
# switches the context index `[0x8000C000]` to 3 and drains the context 3 ring.
TIMER_IRQ = 22
# ASSUMED: virtual time. One 1 ms tick per this many ARM instructions (about 200 MIPS, the order
# of an ARM926 at 300 MHz); the emulator itself runs 15 to 20 M instructions per second.
INSNS_PER_TICK = 200_000


def snapshot(emu: ae.ArmEmu) -> bytes:
    """Serialise memory regions, registers and the Python-side model state of `emu`."""
    regions = []
    for begin, end, perms in emu.uc.mem_regions():
        raw = bytes(emu.uc.mem_read(begin, end - begin + 1))
        regions.append((begin, end - begin + 1, perms, zlib.compress(raw, 1)))
    state = {
        "regs": [emu.reg(n) for n in range(16)],
        "tick": dict(getattr(emu, "tick_state", {})),
        "heap": getattr(emu, "heap_top", [0])[0],
        "stub_addrs": sorted(emu.stubs),
    }
    return zlib.compress(pickle.dumps((regions, state)), 1)


def _mapped(emu: ae.ArmEmu, address: int) -> bool:
    return any(b <= address <= e for b, e, _ in emu.uc.mem_regions())


def restore(emu: ae.ArmEmu, blob: bytes) -> None:
    """Load a `snapshot` into an emulator prepared the same way (stubs and hooks installed)."""
    regions, state = pickle.loads(zlib.decompress(blob))  # noqa: S301 - our own snapshot file
    for begin, size, _perms, comp in regions:
        for page in range(begin, begin + size, ae.PAGE):
            if not _mapped(emu, page):
                emu.uc.mem_map(page, ae.PAGE)
        emu.uc.mem_write(begin, zlib.decompress(comp))
    for n in range(16):
        emu.set_reg(n, state["regs"][n])
    if hasattr(emu, "tick_state"):
        emu.tick_state.update(state["tick"] if isinstance(state["tick"], dict) else {})
    if hasattr(emu, "heap_top"):
        emu.heap_top[0] = state["heap"]


def run_until(
    emu: ae.ArmEmu, stop_pc: int | None, max_insns: int, pc: int | None = None, chunk: int = CHUNK
) -> tuple[int, int, str]:
    """Run from `pc` (default: the current pc) until `stop_pc` is about to execute.

    Low memory (the null-call guard page) is re-zeroed between chunks as in `arm_boot.boot_until`.
    Returns `(pc, instructions_run, error)`; `pc == stop_pc` means the stop was reached.
    """
    zeros = bytes(ab.LOW_MEMORY)
    hit: list[int] = []
    handle = None
    if stop_pc is not None:

        def stop(uc, address, _size, _user) -> None:  # noqa: ANN001
            hit.append(address)
            uc.emu_stop()

        handle = emu.uc.hook_add(emu._u.UC_HOOK_CODE, stop, begin=stop_pc, end=stop_pc)
    pc = emu.reg(15) if pc is None else pc
    done, error = 0, ""
    try:
        while done < max_insns and not hit:
            try:
                emu.uc.emu_start(pc, ae.SENTINEL, count=min(chunk, max_insns - done))
            except Exception as exc:  # unicorn raises UcError for faults
                error = str(exc)
                pc = emu.reg(15)
                break
            done += chunk
            pc = emu.reg(15)
            emu.uc.mem_write(0, zeros)
            if pc == ae.SENTINEL:
                break
    finally:
        if handle is not None:
            emu.uc.hook_del(handle)
    return (stop_pc if hit else pc), min(done, max_insns), error


def call_preserving(emu: ae.ArmEmu, fn: int, *args: int, max_insns: int = 2_000_000) -> int:
    """Call `fn(*args)` (r0..r3) in the middle of a running program and restore every register.

    The call uses the stack below the current sp, so a main loop that is interrupted between two
    `run_until` chunks resumes unchanged. Returns r0.
    """
    saved = [emu.reg(n) for n in range(16)]
    cpsr = emu.uc.reg_read(emu._u.arm_const.UC_ARM_REG_CPSR)
    emu.set_reg(13, (saved[13] - 0x800) & ~7)
    emu.set_reg(14, ae.SENTINEL)
    for i, a in enumerate(args[:4]):
        emu.set_reg(i, a)
    try:
        emu.uc.emu_start(fn, ae.SENTINEL, count=max_insns)
        result = emu.reg(0)
    finally:
        for n in range(16):
            emu.set_reg(n, saved[n])
        emu.uc.reg_write(emu._u.arm_const.UC_ARM_REG_CPSR, cpsr)
    return result


def inject_irq(emu: ae.ArmEmu, irq: int = TIMER_IRQ) -> int:
    """Run the registered handler of system interrupt `irq` as a call; returns its address."""
    handler = emu.read32(IRQ_TABLE + 4 * irq)
    if handler:
        call_preserving(emu, handler)
    return handler


def enable_real_timer(emu: ae.ArmEmu) -> None:
    """Stop the +5000-per-read tick hack: the timer interrupt injected by `run_ticking` counts."""
    emu.tick_state["step"] = 0


def run_ticking(emu: ae.ArmEmu, insns: int, per_tick: int = INSNS_PER_TICK) -> tuple[int, str]:
    """Run `insns` instructions from the current pc, injecting the timer interrupt every
    `per_tick` instructions. Returns `(pc, error)`."""
    pc, done, error = emu.reg(15), 0, ""
    while done < insns and not error:
        pc, ran, error = run_until(emu, None, min(per_tick, insns - done), pc=pc, chunk=per_tick)
        done += max(ran, 1)
        if not error:
            try:
                inject_irq(emu)
            except Exception as exc:  # unicorn raises UcError for faults
                error = f"irq: {exc}"
    return emu.reg(15), error


def settle(emu: ae.ArmEmu, window: int = 20_000_000, max_windows: int = 20) -> int:
    """Run with the timer until a whole `window` sends no DSP message; returns the windows run.

    The first ~140 M instructions (700 ticks) after the main loop starts push the initial state to
    every DSP (VERIFIED run, `docs/research/arm-organ-messages.md` section 10): thousands of
    messages, then silence. Messages sent while settling are discarded.
    """
    for n in range(1, max_windows + 1):
        emu.clear_messages()
        run_ticking(emu, window)
        if not emu.port_writes:
            return n
    return max_windows


def boot_to_main_loop(emu: ae.ArmEmu, max_insns: int = 1_000_000_000) -> tuple[int, int, str]:
    """Run the application constructor thunk until the main loop is about to run."""
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(14, ae.SENTINEL)
    return run_until(emu, MAIN_LOOP, max_insns, pc=ab.APP_THUNK)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--insns", type=int, default=1_000_000_000)
    ap.add_argument("--save", type=Path, help="write a snapshot of the steady state")
    ns = ap.parse_args(argv)
    emu = bf.prepare(ns.image, ns.sram)
    pc, n, err = boot_to_main_loop(emu, ns.insns)
    print(f"stopped pc={pc:#x} after ~{n} instructions error={err!r} nulls={len(emu.null_calls)}")
    if ns.save and pc == MAIN_LOOP:
        ns.save.write_bytes(snapshot(emu))
    return 0 if pc == MAIN_LOOP else 1


if __name__ == "__main__":
    sys.exit(main())
