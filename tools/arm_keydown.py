#!/usr/bin/env python3
"""Key-down / key-up events in the booted host firmware: the board-present flag and a UART2 model.

Two findings make real MIDI note events run through the real firmware code in `arm_steady`'s
emulation (evidence and tables: `docs/research/keydown-paths.md`):

1. **The idle hooks were never registered.** `0xC024DB74(cfg)` registers the per-phase hooks of
   the main loop (`0xC00A1204(slot, fn)` stores `fn` at `[[0xC0597FC0 + 0x670] + 4 * (slot + 2)]`).
   With `[cfg + 0x12] == 0` or `[cfg + 0x13] == 0` it registers four tiny stubs (`0xC024D1E4`,
   `0xC024D200`, `0xC024D218`, `0xC024D21C`) instead of the real hooks, among them slot 2 =
   `0x800015A8`, whose body calls the MIDI parser `0xC008D448` on the UART2 ring. The harness boot
   stub `arm_boot.install_boot_stubs` skips the board probe that sets those two bytes, so they
   stayed 0. `install_board_present` writes 1 into both bytes when `0xC024DB74` is entered.
2. **UART2 is a TI 16550**: the receive ISR (irq 61, handler `0x80000434`) reads `LSR` (`+0x14`,
   bit 0 = data ready) and `RBR` (`+0`) and stores the byte into the ring that the parser reads.
   `Uart2` models `RBR`, `LSR`, `IIR` as a byte queue; `push_midi` queues bytes and runs the ISR.

    uv run --with unicorn python tools/arm_keydown.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap --note 60 --velocity 100
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_boot_full as bf
import arm_emu as ae
import arm_steady as ast

# VERIFIED (run, `docs/research/keydown-paths.md` section 1): board configuration object
# attach routine; `[cfg + 0x12]`, `[cfg + 0x13]` are the two "board present" bytes tested by
# `0xC024BF8C` (`[cfg+0x12] ? [cfg+0x13] : 0`) before the real hooks are registered.
BOARD_ATTACH = 0xC024DB74
BOARD_PRESENT_OFFSET = 0x12
# VERIFIED (run): base of the hook table object and the registration routine.
HOOK_BASE = 0xC0597FC0
HOOK_POINTER = 0x670
HOOK_REGISTER = 0xC00A1204
# VERIFIED (run): slot -> real hook (SRAM image) once the board is present.
REAL_HOOKS = {
    1: 0x80000B30,
    2: 0x800015A8,
    3: 0x800013D8,
    4: 0x80000AE0,
    6: 0x80000D1C,
    7: 0x80000AF4,
}
# VERIFIED (disassembly of the ISR `0x80000434` and the AINTC table `0xC055F674`): UART2 receive
# interrupt number and the 16550 register block it reads (`LSR` bit 0, `RBR`).
UART2_BASE = 0x01D0D000
UART2_IRQ = 61
LSR_OFFSET, IIR_OFFSET, RBR_OFFSET = 0x14, 0x08, 0x00
# ASSUMED: TI 16550 `LSR` values: `0x60` (THRE | TEMT) idle, `0x61` with a byte ready; `IIR`
# `0x04` (receive data available) with data, `0x01` (no interrupt pending) idle.
LSR_IDLE, LSR_READY = 0x60, 0x61
IIR_IDLE, IIR_RX = 0x01, 0x04
# VERIFIED (run): the UART2 software ring `[buf, size, head, tail]` and the MIDI parser
# `0xC008D448(this = UART driver object)`; the ISR advances the head, the parser the tail.
UART_RING = 0x90165770
MIDI_PARSER = 0xC008D448
TICK_INSNS = ast.INSNS_PER_TICK


def install_board_present(emu: ae.ArmEmu) -> None:
    """Make the board probe report "present" so `0xC024DB74` registers the real hooks."""

    def hook(_uc, _address, _size, _user) -> None:  # noqa: ANN001
        emu.write_bytes(emu.reg(0) + BOARD_PRESENT_OFFSET, b"\x01\x01")

    emu.uc.hook_add(emu._u.UC_HOOK_CODE, hook, begin=BOARD_ATTACH, end=BOARD_ATTACH)


@dataclass
class Uart2:
    """Receive side of the 16550 at `UART2_BASE`: a byte queue behind `RBR`, `LSR`, `IIR`."""

    queue: list[int] = field(default_factory=list)
    reads: int = 0

    def install(self, emu: ae.ArmEmu) -> Uart2:
        """Add the read hook (after `arm_boot.install_boot_stubs`, whose poll breaker also hooks
        this window: the later hook runs last and wins)."""

        def hook(uc, _access, address, _size, _value, _user) -> None:  # noqa: ANN001
            off = address - UART2_BASE
            if off == LSR_OFFSET:
                word = LSR_READY if self.queue else LSR_IDLE
            elif off == RBR_OFFSET:
                word = self.queue.pop(0) if self.queue else 0
                self.reads += 1
            elif off == IIR_OFFSET:
                word = IIR_RX if self.queue else IIR_IDLE
            else:
                return
            uc.mem_write(address & ~3, struct.pack("<I", word))

        try:  # a read hook only fires on mapped memory (the firmware boot maps it on first use)
            emu.uc.mem_map(UART2_BASE, ae.PAGE)
        except emu._u.UcError:
            pass
        emu.uc.hook_add(emu._u.UC_HOOK_MEM_READ, hook, begin=UART2_BASE, end=UART2_BASE + 0x1F)
        return self

    def push(self, emu: ae.ArmEmu, data: bytes) -> int:
        """Queue `data` and run the receive ISR; returns the ISR address (0: none attached)."""
        self.queue.extend(data)
        return ast.inject_irq(emu, UART2_IRQ)


def note_on(note: int, velocity: int, channel: int = 0) -> bytes:
    return bytes([0x90 | (channel & 15), note & 0x7F, velocity & 0x7F])


def note_off(note: int, channel: int = 0, velocity: int = 0) -> bytes:
    return bytes([0x80 | (channel & 15), note & 0x7F, velocity & 0x7F])


def ring_state(emu: ae.ArmEmu, ring: int = UART_RING) -> tuple[int, int, int]:
    """`(size, head, tail)` of the UART2 software ring."""
    return emu.read32(ring + 4), emu.read32(ring + 8), emu.read32(ring + 12)


def real_hooks(emu: ae.ArmEmu) -> dict[int, int]:
    """Hook function per slot as stored in the hook table (slot `n` at word `n + 2`)."""
    table = emu.read32(HOOK_BASE + HOOK_POINTER)
    return {slot: emu.read32(table + 4 * (slot + 2)) for slot in range(1, 8)}


def hooks_are_real(emu: ae.ArmEmu) -> bool:
    return all(real_hooks(emu).get(s) == fn for s, fn in REAL_HOOKS.items())


@dataclass
class Rig:
    """A settled emulator with the board present and the UART2 model attached."""

    emu: ae.ArmEmu
    uart: Uart2

    def send(self, data: bytes) -> None:
        self.uart.push(self.emu, data)

    def tick(self, count: int = 1) -> list[tuple[int, ae.Message]]:
        """Run `count` 1 ms ticks; returns `(tick index, message)` for every message sent."""
        out: list[tuple[int, ae.Message]] = []
        for n in range(count):
            self.emu.clear_messages()
            ast.run_ticking(self.emu, TICK_INSNS, per_tick=TICK_INSNS)
            out += [(n, m) for m in self.emu.messages]
        return out


def build_rig(code: Path, sram: Path | None, cache: Path | None = None) -> Rig:
    """Boot (30 s) and settle (30 s), or restore `cache`; attach the UART2 model."""
    emu = bf.prepare(code, sram)
    install_board_present(emu)
    if cache is not None and cache.exists():
        ast.restore(emu, cache.read_bytes())
        emu.install_dsp_ports()
        ast.enable_real_timer(emu)
    else:
        pc, _n, err = ast.boot_to_main_loop(emu)
        if pc != ast.MAIN_LOOP:
            raise SystemExit(f"boot stopped at {pc:#x} {err}")
        emu.install_dsp_ports()
        ast.enable_real_timer(emu)
        ast.settle(emu)
        if cache is not None:
            cache.write_bytes(ast.snapshot(emu))
    return Rig(emu, Uart2().install(emu))


# VERIFIED (run, `docs/research/keydown-paths.md` section 3): functions the note path reaches.
WATCH = {
    0xC0085630: "midi_note_on_post",
    0xC00855B8: "midi_note_off_post",
    0xC008D448: "midi_parser",
    0xC00BC7AC: "piano_event_slot3",
    0xC00BC8AC: "piano_key_event",
    0xC00AD5CC: "piano_voice_frame",
    0xC00A9274: "piano_voice_start",
    0xC00BF890: "organ_key_down2",
    0xC01EC690: "synth_voice_event",
}
CSV_HEADER = "phase,note,velocity,tick,chip,path,core,dm_word,value"


def count_calls(emu: ae.ArmEmu, watch: dict[int, str] | None = None) -> dict[str, int]:
    """Install entry counters on `watch` (default `WATCH`); returns the live counter dict."""
    counts: dict[str, int] = {}
    for address, name in (watch or WATCH).items():
        counts[name] = 0

        def hook(_uc, _a, _s, _u, name: str = name) -> None:  # noqa: ANN001
            counts[name] += 1

        emu.uc.hook_add(emu._u.UC_HOOK_CODE, hook, begin=address, end=address)
    return counts


def message_rows(
    phase: str, note: int, velocity: int, msgs: list[tuple[int, ae.Message]]
) -> list[str]:
    """CSV lines (`CSV_HEADER`) for the messages of one phase; derived values only."""
    return [
        f"{phase},{note},{velocity},{t},{m.chip},{m.path},{m.core},{m.dm_word:#x},{m.word2:#010x}"
        for t, m in msgs
    ]


def capture_note(
    rig: Rig,
    note: int,
    velocity: int,
    counts: dict[str, int],
    on_ticks: int = 30,
    off_ticks: int = 30,
) -> list[str]:
    """Note-on, `on_ticks`, note-off, `off_ticks`: the CSV rows; `counts` (from `count_calls`)
    is reset first and holds the per-function entry counts afterwards."""
    for name in counts:
        counts[name] = 0
    rig.send(note_on(note, velocity))
    rows = message_rows("on", note, velocity, rig.tick(on_ticks))
    rig.send(note_off(note))
    return rows + message_rows("off", note, velocity, rig.tick(off_ticks))


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--note", type=int, default=60)
    ap.add_argument("--velocity", type=int, default=100)
    ap.add_argument("--channel", type=int, default=0)
    ap.add_argument("--ticks", type=int, default=20)
    ns = ap.parse_args(argv)
    rig = build_rig(ns.image, ns.sram, ns.cache)
    print(f"real hooks registered: {hooks_are_real(rig.emu)}")
    counts = count_calls(rig.emu)
    rig.tick(2)
    rig.send(note_on(ns.note, ns.velocity, ns.channel))
    print(f"ring (size, head, tail) after the ISR: {ring_state(rig.emu)}")
    msgs = rig.tick(ns.ticks)
    print(f"ring after {ns.ticks} ticks: {ring_state(rig.emu)}; {len(msgs)} DSP messages")
    print("function entries:", {k: v for k, v in counts.items() if v})
    print(CSV_HEADER)
    print("\n".join(message_rows("on", ns.note, ns.velocity, msgs)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
