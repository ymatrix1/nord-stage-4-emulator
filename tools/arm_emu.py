#!/usr/bin/env python3
"""ARM926EJ-S emulation harness for the Nord Stage 4 host firmware (`bank0_CODE.bin`), on `unicorn`.

Runs firmware functions in isolation. The code image sits at `0xC0000000`, the RAM init block
(`bank0_SRAM.bin`) at `0x80000000`, and unmapped accesses auto-map zero pages. The DSP comm objects
that the real boot code builds are replaced by a small fake (`install_dsp_ports`): every word the
firmware stores to a DSP port is logged and grouped into a `Message` (`path 2`: system address,
optional pad word, value; `path 1`: DM word address, value). Because the writer wrappers and the
inlined senders both end in the same port stores, one write hook sees all of them.

Helpers for code that needs more of the system than the harness provides: `add_stub` (replace a
function), `skip_instruction` (for example the MMU-enable `mcr`), `guard_null_calls` (a call
through a null vtable returns 0), `enable_poll_breaker` (busy-wait loops on device registers
exit), `bump_new` (heap allocator for `operator new`).

Evidence and limits: `docs/research/arm-fw-emulation.md`.

    uv run --with unicorn python tools/arm_emu.py call bank0_CODE.bin 0xC01CE034 0x80010000 \
        --sram bank0_SRAM.bin --poke 0x80010000=32 --poke 0x80010008=0x5500
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

LOAD_BASE = 0xC0000000
SRAM_BASE = 0x80000000
RAM_SIZE = 0x01000000  # 0xC0000000..0xC1000000: code, data, BSS (channel tables end at 0xC054B000)
STACK_TOP = 0x700F0000
SENTINEL = 0x70100000  # return address of a top-level call; never executed
PAGE = 0x1000

# VERIFIED: tools/arm_params.py (docs/research/arm-param-messages.md): flow control routine of the
# DSP comm object, chosen so that `call` never waits for a DSP acknowledge.
FLOW_CONTROL = 0xC006CA48
INIT_ENTRY = 0xC00002A4
# VERIFIED: the poke wrappers load `[0xC055B150]` (the port object) and read `+0x40 + 4*comm` (port
# pointer) and `+0x90 + comm` (pad byte); the inlined senders do the same.
PORT_OBJECT_PTR = 0xC055B150
# VERIFIED: path 1 senders load the comm object from the table at 0xC053F8A4 (entry `comm + 1`) and
# store through `[obj + 0x10]`; flow control state lives at `[obj + 0x20]`.
COMM_TABLE = 0xC053F8A4

# Fake DSP port block (harness-only addresses, outside every firmware region).
FAKE_BASE = 0xA0000000
FAKE_SIZE = 0x20000
PORT2_BASE = 0xA0010000  # path 2 port of comm c: PORT2_BASE + 0x10 * c
PORT1_BASE = 0xA0011000  # path 1 port of comm c: PORT1_BASE + 0x10 * c
COMM_COUNT = 5
# ASSUMED: comm index -> DSP image, same order as the download index (arm-param-messages.md s3).
COMM_NAMES = ("FX", "PNO", "ORG", "SY", "SMP")

# ASSUMED: AM1705 peripheral windows (tools/arm_xref.py PERIPHERALS) polled by firmware wait loops.
DEVICE_WINDOWS = ((0x01C00000, 0x01F00000), (0x60000000, 0x6A000000), (0xB0000000, 0xB0010000))
# ASSUMED: values tried, in order, on a polled device word until a firmware wait loop exits.
POLL_VALUES = (0xFFFFFFFF, 0, 3, 1, 0x3F, 0x10001, 0x80000000, 0x0F0F0F0F)


@dataclass(frozen=True)
class Message:
    """One ARM -> DSP message after grouping the port stores.

    `path` 2: `word1` is a system byte address (`0x28000000 + 4 w` core 1, `0x28800000 + 4 w`
    core 2), `word2` the value. `path` 1: `word1` is a DM word address. `comm` is the comm object
    index (see `COMM_NAMES`), `pc` the store instruction.
    """

    path: int
    comm: int
    word1: int
    word2: int
    pc: int = 0

    @property
    def chip(self) -> str:
        return COMM_NAMES[self.comm] if 0 <= self.comm < len(COMM_NAMES) else f"comm{self.comm}"

    @property
    def core(self) -> int:
        """DSP core (1 or 2) for path 2 messages, 0 for path 1."""
        if self.path != 2:
            return 0
        return 2 if self.word1 >= 0x28800000 else 1

    @property
    def dm_word(self) -> int:
        """DM word address in the receiving DSP (path 1: `word1`; path 2: from the byte address)."""
        if self.path == 1:
            return self.word1
        return (self.word1 - (0x28800000 if self.core == 2 else 0x28000000)) // 4

    def to_dict(self) -> dict[str, object]:
        return {
            "chip": self.chip,
            "comm": self.comm,
            "path": self.path,
            "core": self.core,
            "w1": f"{self.word1:#010x}",
            "w2": f"{self.word2:#010x}",
            "dm": f"{self.dm_word:#x}",
            "pc": f"{self.pc:#010x}",
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict())


def u32(data: bytes, offset: int = 0) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def _reg_id(n: int) -> int:
    from unicorn.arm_const import (  # type: ignore
        UC_ARM_REG_LR,
        UC_ARM_REG_PC,
        UC_ARM_REG_R0,
        UC_ARM_REG_SP,
    )

    if n < 13:
        return UC_ARM_REG_R0 + n
    return {13: UC_ARM_REG_SP, 14: UC_ARM_REG_LR, 15: UC_ARM_REG_PC}[n]


def group_port_writes(
    writes: list[tuple[int, int, int]], pad: tuple[int, ...] = (1, 1, 1, 0, 0)
) -> list[Message]:
    """Group `(port, value, pc)` stores into messages.

    Ports `PORT2_BASE + 0x10 c` carry `(address, [0 if pad[c]], value)`; ports
    `PORT1_BASE + 0x10 c` carry `(word1, word2)`. Incomplete trailing groups are dropped.
    """
    pending: dict[int, list[tuple[int, int]]] = {}
    out: list[Message] = []
    for port, value, pc in writes:
        if PORT2_BASE <= port < PORT2_BASE + 0x10 * COMM_COUNT:
            path, comm = 2, (port - PORT2_BASE) // 0x10
            need = 3 if pad[comm] else 2
        elif PORT1_BASE <= port < PORT1_BASE + 0x10 * COMM_COUNT:
            path, comm, need = 1, (port - PORT1_BASE) // 0x10, 2
        else:
            continue
        buf = pending.setdefault(port, [])
        buf.append((value, pc))
        if len(buf) == need:
            out.append(Message(path, comm, buf[0][0], buf[-1][0], buf[-1][1]))
            buf.clear()
    return out


@dataclass
class ArmEmu:
    """Unicorn-backed ARM926EJ-S with stubs, fake DSP ports and an auto-map log."""

    code: bytes
    sram: bytes | None = None
    pad: tuple[int, ...] = (1, 1, 1, 0, 0)
    ram_size: int = RAM_SIZE
    port_writes: list[tuple[int, int, int]] = field(default_factory=list)
    auto_mapped: list[int] = field(default_factory=list)
    null_calls: list[tuple[int, int]] = field(default_factory=list)
    poll_hits: dict[tuple[int, int], int] = field(default_factory=dict)
    stubs: dict[int, Callable[[ArmEmu], int | None]] = field(default_factory=dict)

    def __post_init__(self) -> None:
        try:
            import unicorn
            import unicorn.arm_const
            from unicorn import UC_ARCH_ARM, UC_MODE_ARM, Uc  # type: ignore
        except ImportError as exc:  # pragma: no cover
            raise SystemExit("needs unicorn: uv run --with unicorn python ...") from exc
        self._u = unicorn
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_ARM)
        # ASSUMED: the host SoC core is an ARM926EJ-S (TI AM1705); the default model aborts on the
        # firmware's cp15 sequences.
        self.uc.ctl_set_cpu_model(unicorn.arm_const.UC_CPU_ARM_926)
        self.uc.mem_map(LOAD_BASE, self.ram_size)
        self.uc.mem_write(LOAD_BASE, self.code)
        self.uc.mem_map(SRAM_BASE, 0x100000)
        if self.sram:
            self.uc.mem_write(SRAM_BASE, self.sram)
        self.uc.mem_map(STACK_TOP - 0x100000, 0x200000)
        self.uc.mem_map(FAKE_BASE, FAKE_SIZE)
        self.uc.hook_add(
            unicorn.UC_HOOK_MEM_READ_UNMAPPED
            | unicorn.UC_HOOK_MEM_WRITE_UNMAPPED
            | unicorn.UC_HOOK_MEM_FETCH_UNMAPPED,
            self._on_unmapped,
        )
        self.install_dsp_ports()

    # ----- register and memory helpers -------------------------------------------------------
    def reg(self, n: int) -> int:
        return self.uc.reg_read(_reg_id(n))

    def set_reg(self, n: int, value: int) -> None:
        self.uc.reg_write(_reg_id(n), value & 0xFFFFFFFF)

    def read32(self, address: int) -> int:
        return u32(bytes(self.uc.mem_read(address, 4)))

    def write32(self, address: int, value: int) -> None:
        data = struct.pack("<I", value & 0xFFFFFFFF)
        try:
            self.uc.mem_write(address, data)
        except self._u.UcError:  # page outside the mapped regions (small test images)
            self.uc.mem_map(address & ~(PAGE - 1), PAGE)
            self.uc.mem_write(address, data)

    def read_bytes(self, address: int, size: int) -> bytes:
        return bytes(self.uc.mem_read(address, size))

    def write_bytes(self, address: int, data: bytes) -> None:
        self.uc.mem_write(address, data)

    def _on_unmapped(self, uc, _access, address, _size, _value, _user) -> bool:  # noqa: ANN001
        page = address & ~(PAGE - 1)
        try:
            uc.mem_map(page, PAGE)
        except Exception:
            return False
        self.auto_mapped.append(page)
        return True

    # ----- DSP ports -------------------------------------------------------------------------
    @property
    def messages(self) -> list[Message]:
        return group_port_writes(self.port_writes, self.pad)

    def clear_messages(self) -> None:
        self.port_writes.clear()

    def install_dsp_ports(self) -> None:
        """Build fake DSP comm objects and log every store to their ports.

        The firmware reaches a DSP through objects built at boot (comm table `COMM_TABLE`, port
        object `PORT_OBJECT_PTR`). The fake keeps the layout the senders read: port pointer at
        `+0x40 + 4 c`, pad byte at `+0x90 + c`, path 1 object `[+0x10]` port, `[+0x20]` flow
        control state. Flow control (`FLOW_CONTROL`) is stubbed to return at once. Calling this
        again re-writes the objects (the static initialiser clears the pointer words).
        """
        base = FAKE_BASE
        for c in range(COMM_COUNT):
            self.write32(base + 0x40 + 4 * c, PORT2_BASE + 0x10 * c)
            self.uc.mem_write(base + 0x90 + c, bytes([self.pad[c]]))
            obj = base + 0x1000 + 0x100 * c
            self.write32(obj + 0x10, PORT1_BASE + 0x10 * c)
            self.write32(obj + 0x20, base + 0x2000 + 0x40 * c)
            self.write32(COMM_TABLE + 4 * (c + 1), obj)
        self.write32(PORT_OBJECT_PTR, base)
        if not getattr(self, "_ports_hooked", False):
            self._ports_hooked = True

            def hook(_uc, _access, address, _size, value, _user) -> None:  # noqa: ANN001
                self.port_writes.append((address & ~0xF, value & 0xFFFFFFFF, self.reg(15)))

            self.uc.hook_add(
                self._u.UC_HOOK_MEM_WRITE,
                hook,
                begin=PORT2_BASE,
                end=PORT1_BASE + 0x10 * COMM_COUNT - 1,
            )
            self.add_stub(FLOW_CONTROL, lambda _e: 1)

    # ----- stubs and patches -----------------------------------------------------------------
    def add_stub(self, address: int, fn: Callable[[ArmEmu], int | None]) -> None:
        """Replace the function at `address`: `fn(emu)` runs instead, its return goes to r0."""
        self.stubs[address] = fn
        self.uc.hook_add(self._u.UC_HOOK_CODE, self._stub_hook, begin=address, end=address)

    def _stub_hook(self, _uc, address, _size, _user) -> None:  # noqa: ANN001
        ret = self.stubs[address](self)
        if ret is not None:
            self.set_reg(0, ret)
        self.set_reg(15, self.reg(14) & ~1)

    def skip_instruction(self, address: int) -> None:
        """Make the single instruction at `address` a no-op (for example an MMU-enable `mcr`)."""
        self.uc.hook_add(
            self._u.UC_HOOK_CODE,
            lambda _uc, a, _s, _u: self.set_reg(15, a + 4),
            begin=address,
            end=address,
        )

    def guard_null_calls(self, size: int = 0x10000) -> None:
        """Treat a call through a null function pointer or vtable as `return 0`.

        Low memory is mapped and any instruction fetched there returns to the caller with r0 = 0,
        which lets code run past objects the partial boot never built (logged in `null_calls`).
        """
        self.uc.mem_map(0, size)

        def hook(_uc, address, _size, _user) -> None:  # noqa: ANN001
            self.null_calls.append((address, self.reg(14)))
            self.set_reg(0, 0)
            self.set_reg(15, self.reg(14) & ~1)

        self.uc.hook_add(self._u.UC_HOOK_CODE, hook, begin=0, end=size - 1)

    def enable_poll_breaker(
        self, windows: tuple[tuple[int, int], ...] = DEVICE_WINDOWS, limit: int = 40
    ) -> None:
        """Let busy-wait loops on device registers terminate.

        A read of the same device word from the same instruction more than `limit` times is a poll
        loop; the harness then rotates the word through `POLL_VALUES` until the loop exits.
        """
        counts: dict[tuple[int, int], int] = {}

        def hook(uc, _access, address, _size, _value, _user) -> None:  # noqa: ANN001
            key = (self.reg(15), address & ~3)
            n = counts.get(key, 0) + 1
            counts[key] = n
            if n > limit:
                v = POLL_VALUES[((n - limit) // 8) % len(POLL_VALUES)]
                self.poll_hits[key] = self.poll_hits.get(key, 0) + 1
                try:
                    uc.mem_write(address & ~3, struct.pack("<I", v))
                except Exception:  # unmapped page: the unmapped hook maps it on the next access
                    pass

        for lo, hi in windows:
            self.uc.hook_add(self._u.UC_HOOK_MEM_READ, hook, begin=lo, end=hi - 1)

    def bump_new(self, address: int, heap_base: int = 0x90000000, size: int = 0x4000000) -> None:
        """Stub the allocator at `address` (r0 = size) with a bump pointer over zeroed memory."""
        self.uc.mem_map(heap_base, size)
        top = [heap_base]

        def alloc(emu: ArmEmu) -> int:
            p = top[0]
            top[0] += max((emu.reg(0) + 15) & ~15, 16)
            return p

        self.add_stub(address, alloc)

    # ----- running ---------------------------------------------------------------------------
    def run(self, pc: int, until: int = SENTINEL, max_insns: int = 5_000_000) -> None:
        """Run from `pc` until the PC reaches `until` (or `max_insns` instructions)."""
        self.uc.emu_start(pc, until, count=max_insns)

    def call(self, address: int, *args: int, max_insns: int = 5_000_000) -> int:
        """Call `address(*args)` with the AAPCS (r0..r3, then the stack); returns r0."""
        sp = STACK_TOP - 0x1000
        for i, a in enumerate(args[:4]):
            self.set_reg(i, a)
        extra = args[4:]
        sp -= 4 * len(extra)
        for i, a in enumerate(extra):
            self.write32(sp + 4 * i, a)
        self.set_reg(13, sp)
        self.set_reg(14, SENTINEL)
        self.run(address, SENTINEL, max_insns)
        return self.reg(0)

    def run_skipping_calls(self, pc: int, until: int, max_insns: int = 50_000_000) -> None:
        """Run with every `bl` skipped except calls to stubs (static-initialiser style)."""
        skip: list[int | None] = [None]

        def hook(uc, address, _size, _user) -> None:  # noqa: ANN001
            insn = u32(bytes(uc.mem_read(address, 4)))
            if (insn >> 24) == 0xEB:
                off = ((insn & 0xFFFFFF) ^ 0x800000) - 0x800000
                if (address + 8 + off * 4) & 0xFFFFFFFF not in self.stubs:
                    skip[0] = address + 4
                    uc.emu_stop()

        h = self.uc.hook_add(self._u.UC_HOOK_CODE, hook)
        try:
            for _ in range(500_000):
                self.uc.emu_start(pc, until, count=max_insns)
                if skip[0] is None:
                    break
                pc, skip[0] = skip[0], None
        finally:
            self.uc.hook_del(h)


def load_firmware(code_path: Path, sram_path: Path | None = None) -> ArmEmu:
    return ArmEmu(code_path.read_bytes(), sram_path.read_bytes() if sram_path else None)


def run_static_init(emu: ArmEmu) -> None:
    """Run the static initialiser at `INIT_ENTRY` (stores the channel tables and static objects)."""
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run_skipping_calls(INIT_ENTRY, SENTINEL)
    emu.install_dsp_ports()  # the initialiser zeroes BSS words that the fake comm objects use


def to_msg_script(messages: list[Message], chip: str, core: int, start_pass: int = 0) -> str:
    """`sharc-org --msg-script` text for the messages that reach one DSP core.

    Lines are `PASS W1 W2` with W1 the DM word address (use `--msg-base 0`). A `#` comment keeps
    the source address.
    """
    lines = []
    for m in messages:
        if m.chip == chip and (m.core == core or (m.path == 1 and core == 0)):
            lines.append(
                f"{start_pass} {m.dm_word:#x} {m.word2:#010x}  # {m.word1:#010x} pc {m.pc:#x}"
            )
    return "\n".join(lines) + ("\n" if lines else "")


def messages_from_jsonl(text: str) -> list[Message]:
    """Parse the output of `Message.to_json` lines back into messages."""
    out = []
    for line in text.splitlines():
        if not line.strip():
            continue
        d = json.loads(line)
        out.append(
            Message(
                d["path"], d["comm"], int(d["w1"], 0), int(d["w2"], 0), int(d.get("pc", "0"), 0)
            )
        )
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=["call"])
    ap.add_argument("image", type=Path)
    ap.add_argument("args", nargs="+", help="function address then r0..r3")
    ap.add_argument("--sram", type=Path)
    ap.add_argument("--poke", action="append", default=[], help="ADDR=VALUE 32-bit RAM poke")
    ns = ap.parse_args(argv)
    emu = load_firmware(ns.image, ns.sram)
    run_static_init(emu)
    emu.guard_null_calls()
    emu.enable_poll_breaker()
    for p in ns.poke:
        a, v = p.split("=")
        emu.write32(int(a, 0), int(v, 0))
    vals = [int(x, 0) for x in ns.args]
    emu.call(vals[0], *vals[1:], max_insns=1_000_000)
    for m in emu.messages:
        print(m.to_json())
    return 0


if __name__ == "__main__":
    sys.exit(main())
