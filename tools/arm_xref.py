#!/usr/bin/env python3
"""ARM-side cross-reference helper for the host (TI AM1705, ARM926EJ-S) firmware image.

`bank0_CODE.bin` loads at 0xC0000000 and is ARM mode (little endian). This tool finds the code that
touches peripherals and the driver layer that talks to the DSPs, with a pure-Python decoder for the
few instruction forms needed (LDR literal, MOV immediate, BL, BLX register), so it runs without
extra packages. `dis` uses `capstone` when it is installed (`uv run --with capstone ...`).

    python tools/arm_xref.py mmio target/fw_unpacked/bank0_CODE.bin
    python tools/arm_xref.py callers target/fw_unpacked/bank0_CODE.bin 0xC006CA48
    python tools/arm_xref.py literal target/fw_unpacked/bank0_CODE.bin 0x62000010
    python tools/arm_xref.py downloads target/fw_unpacked/bank0_CODE.bin 0xC0241C58 0xC0241D80
    python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC0241C58 0xC0241CC8

Findings and evidence: `docs/research/arm-dsp-protocol.md`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000
# End of ARM code in bank0_CODE (docs/research/synth-0x2f-ns4decode-crosscheck.md, block map).
CODE_END = 0x279000

# Peripheral windows of the OMAP-L137 / AM17x family (AM1705 is a subset). ASSUMED from the TI
# memory map (not re-read from the TRM here); SPI0, SPI1, GPIO and SYSCFG0 are corroborated by
# the firmware's own driver code (see the research note), the others are only names for lookup.
PERIPHERALS: list[tuple[int, int, str]] = [
    (0x01C00000, 0x1000, "EDMA3 CC"),
    (0x01C10000, 0x1000, "PSC0"),
    (0x01C11000, 0x1000, "PLL0"),
    (0x01C14000, 0x1000, "SYSCFG0"),
    (0x01C20000, 0x1000, "Timer0"),
    (0x01C21000, 0x1000, "Timer1"),
    (0x01C22000, 0x1000, "I2C0"),
    (0x01C41000, 0x1000, "SPI0"),
    (0x01C42000, 0x1000, "UART0"),
    (0x01D0C000, 0x1000, "UART1"),
    (0x01D0D000, 0x1000, "UART2"),
    (0x01E10000, 0x1000, "UHPI"),
    (0x01E12000, 0x1000, "SPI1"),
    (0x01E26000, 0x1000, "GPIO"),
    (0x01E27000, 0x1000, "PSC1"),
    (0x01E2C000, 0x1000, "SYSCFG1"),
    (0x60000000, 0x02000000, "EMIFA CS2"),
    (0x62000000, 0x02000000, "EMIFA CS3"),
    (0x64000000, 0x02000000, "EMIFA CS4"),
    (0x66000000, 0x02000000, "EMIFA CS5"),
    (0x68000000, 0x00008000, "EMIFA control"),
]


# Device registers on the EMIFA chip-select windows sit near the window start; larger offsets
# are almost always ordinary data words (floats, text, divide-by-constant magic numbers).
EMIFA_REG_SPAN = 0x10000


def peripheral_name(value: int) -> str | None:
    """Name of the peripheral window that contains `value`, or None."""
    for base, size, name in PERIPHERALS:
        if base <= value < base + size:
            return name
    return None


def ror32(value: int, amount: int) -> int:
    amount &= 31
    return ((value >> amount) | (value << (32 - amount))) & 0xFFFFFFFF


def word_at(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def decode_ldr_literal(insn: int, address: int) -> int | None:
    """Address of the literal word read by `LDR Rd, [pc, #+-imm]`, or None for other words."""
    if (insn >> 25) & 7 != 2:
        return None
    pre, up, byte, wback, load = (
        (insn >> 24) & 1,
        (insn >> 23) & 1,
        (insn >> 22) & 1,
        (insn >> 21) & 1,
        (insn >> 20) & 1,
    )
    if (insn >> 16) & 0xF != 15 or not (pre and not byte and not wback and load):
        return None
    if (insn >> 28) == 0xF:
        return None
    imm = insn & 0xFFF
    return address + 8 + (imm if up else -imm)


def decode_mov_imm(insn: int) -> tuple[int, int] | None:
    """(rd, value) for `MOV rd, #imm` (any condition and S bit), else None."""
    if (insn >> 26) & 3 != 0 or not (insn >> 25) & 1 or (insn >> 21) & 0xF != 0xD:
        return None
    if (insn >> 28) == 0xF:
        return None
    return (insn >> 12) & 0xF, ror32(insn & 0xFF, ((insn >> 8) & 0xF) * 2)


def decode_bl(insn: int, address: int) -> int | None:
    """Target of a `BL` word, or None."""
    if (insn >> 25) & 7 != 5 or not (insn >> 24) & 1 or (insn >> 28) == 0xF:
        return None
    imm = insn & 0xFFFFFF
    if imm & 0x800000:
        imm -= 1 << 24
    return (address + 8 + imm * 4) & 0xFFFFFFFF


def decode_blx_reg(insn: int) -> int | None:
    """Register used by `BLX Rm`, or None."""
    if insn & 0x0FFFFFF0 == 0x012FFF30 and (insn >> 28) != 0xF:
        return insn & 0xF
    return None


def literal_value(data: bytes, address: int, base: int = LOAD_BASE) -> int | None:
    off = address - base
    if 0 <= off <= len(data) - 4:
        return word_at(data, off)
    return None


def bl_callers(
    data: bytes, target: int, base: int = LOAD_BASE, end: int | None = None
) -> list[int]:
    """Addresses of `BL target` instructions in `data[:end]`."""
    limit = min(len(data), (end if end is not None else len(data)))
    return [
        base + off
        for off in range(0, limit - 3, 4)
        if decode_bl(word_at(data, off), base + off) == target
    ]


def literal_refs(data: bytes, value: int, base: int = LOAD_BASE) -> list[tuple[int, list[int]]]:
    """Literal-pool words equal to `value` and the `LDR rX, [pc, #n]` sites that load each."""
    out: list[tuple[int, list[int]]] = []
    needle = struct.pack("<I", value)
    at = data.find(needle)
    while at != -1:
        if at % 4 == 0:
            pool = base + at
            sites = [
                base + o
                for o in range(max(0, at - 0x1000), min(len(data) - 3, at + 8), 4)
                if decode_ldr_literal(word_at(data, o), base + o) == pool
            ]
            out.append((pool, sites))
        at = data.find(needle, at + 1)
    return out


def mmio_literals(
    data: bytes, end: int = CODE_END, base: int = LOAD_BASE
) -> list[tuple[int, int, str]]:
    """Aligned words in `data[:end]` that name a peripheral register block or EMIFA window.

    Returns (literal address, value, peripheral name). Plain data that happens to fall in a
    window is filtered by requiring the low bits to look like a register base: 4 KiB aligned for
    on-chip peripherals, any value for the EMIFA chip-select windows.
    """
    hits: list[tuple[int, int, str]] = []
    for off in range(0, min(len(data), end) - 3, 4):
        value = word_at(data, off)
        name = peripheral_name(value)
        if name is None:
            continue
        if name.startswith("EMIFA"):
            if name != "EMIFA control" and value & 0x01FFFFFF >= EMIFA_REG_SPAN:
                continue
            if all(0x20 <= b < 0x7F for b in value.to_bytes(4, "little")):
                continue  # ASCII text
        elif value & 0xFFF:
            continue
        hits.append((base + off, value, name))
    return hits


def download_calls(
    data: bytes, start: int, end: int, base: int = LOAD_BASE
) -> list[tuple[int, int | None, int, int]]:
    """Calls of the form `blx rX` with r1 = index, r2 = pointer, r3 = length in [start, end).

    Tracks only `MOV rd,#imm` and `LDR rd,[pc,#n]` assignments to r1..r3 and forgets them after
    each call, which is the shape of the DSP image download routine (see the research note).
    r2 and r3 must be known; the index is None when r1 was not set by one of those forms (the
    index 0 call takes it from an earlier result). Returns (call address, index, pointer, length).
    """
    regs: dict[int, int] = {}
    calls: list[tuple[int, int | None, int, int]] = []
    for addr in range(start, end, 4):
        insn = word_at(data, addr - base)
        mov = decode_mov_imm(insn)
        if mov is not None:
            regs[mov[0]] = mov[1]
            continue
        lit = decode_ldr_literal(insn, addr)
        if lit is not None:
            value = literal_value(data, lit, base)
            if value is not None:
                regs[(insn >> 12) & 0xF] = value
            continue
        if decode_blx_reg(insn) is not None:
            if 2 in regs and 3 in regs:
                calls.append((addr, regs.get(1), regs[2], regs[3]))
            regs.clear()
    return calls


def _annotate(data: bytes, insn_text: str, address: int, word: int, base: int) -> str:
    lit = decode_ldr_literal(word, address)
    if lit is None:
        return ""
    value = literal_value(data, lit, base)
    if value is None:
        return ""
    note = f"  ; ={value:#x}"
    name = peripheral_name(value)
    return note + (f" [{name}]" if name else "")


def disassemble(data: bytes, lo: int, hi: int, base: int = LOAD_BASE) -> list[str]:
    """Annotated ARM-mode disassembly; needs `capstone` (not a project dependency)."""
    try:
        import capstone  # type: ignore[import-not-found]
    except ImportError as exc:  # pragma: no cover - exercised only without capstone
        raise SystemExit(
            "dis needs capstone: uv run --with capstone python tools/arm_xref.py ..."
        ) from exc
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_ARM)
    lines = []
    for addr in range(lo, hi, 4):
        word = word_at(data, addr - base)
        for ins in md.disasm(data[addr - base : addr - base + 4], addr):
            text = f"{ins.mnemonic} {ins.op_str}"
            lines.append(f"{addr:08x}  {text}{_annotate(data, text, addr, word, base)}")
    return lines


def _int(text: str) -> int:
    return int(text, 0)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=["mmio", "callers", "literal", "downloads", "dis"])
    ap.add_argument("image", type=Path)
    ap.add_argument("args", nargs="*", type=_int)
    ns = ap.parse_args(argv)
    data = ns.image.read_bytes()
    if ns.command == "mmio":
        for addr, value, name in mmio_literals(data):
            print(f"{addr:#010x}  {value:#010x}  {name}")
    elif ns.command == "callers":
        sites = bl_callers(data, ns.args[0], end=CODE_END)
        print(f"{len(sites)} call sites")
        print(" ".join(f"{a:#x}" for a in sites))
    elif ns.command == "literal":
        for pool, sites in literal_refs(data, ns.args[0]):
            print(f"pool {pool:#x} loaded at {', '.join(f'{s:#x}' for s in sites) or '-'}")
    elif ns.command == "downloads":
        for call, idx, ptr, length in download_calls(data, ns.args[0], ns.args[1]):
            index = "?" if idx is None else idx
            print(f"{call:#x}  index {index}  ptr {ptr:#x}  len {length:#x}  end {ptr + length:#x}")
    else:
        print("\n".join(disassemble(data, ns.args[0], ns.args[1])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
