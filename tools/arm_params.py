#!/usr/bin/env python3
"""Recover the ARM -> DSP parameter-message senders from the host firmware (`bank0_CODE.bin`).

The ARM writes every DSP parameter as a short word sequence to a per-DSP port. A *sender* is a
small function that picks a 16-byte channel record `table[ch] = {target, base, size, tag}`
(`table` is one of ~70 RAM arrays), adds a constant field offset to `base`, and writes
`(address, value)` through one of two paths chosen by a per-target flag word:

* path 1 (flag 0, SY and SMP targets): `bl 0xC006CA48` flow control, then the two words
  `base + off` and `value` to the EMIFA port of the comm object;
* path 2 (flag 1, FX / PNO / ORG cores): `base + 4 * off` (a system byte address), an optional
  zero pad word, then `value`, to the poke port of the comm object.

This tool finds the 904 flow-control call sites and the wrapper call sites, classifies every sender
function by the offsets it adds, and (with `unicorn` installed) runs the static initialiser that
fills the channel tables so their contents can be listed.
Evidence: `docs/research/arm-param-messages.md`.

    python tools/arm_params.py senders  target/fw_unpacked/bank0_CODE.bin [--csv OUT.csv]
    python tools/arm_params.py records  IMAGE --mem MEMDUMP.bin --csv OUT.csv
    python tools/arm_params.py tables   MEMDUMP.bin --table 0xC05490B8:12
    uv run --with unicorn python tools/arm_params.py emulate IMAGE OUT_MEMDUMP.bin
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import arm_xref as ax

LOAD_BASE = ax.LOAD_BASE
CODE_END = ax.CODE_END

# VERIFIED: flow-control routine of the DSP comm object, 904 `bl` sites (tools/arm_xref.py callers).
FLOW_CONTROL = 0xC006CA48
# VERIFIED: two-word EMIFA writers `(comm, word1, word2)` at these entries (identical bodies:
# `bl FLOW_CONTROL(r1=1)`, two stores to `[comm+0x10]`, bookkeeping); see the research note.
EMIFA_WRITERS = (0xC0062E10, 0xC0062E78)
# VERIFIED: poke-port writers `(comm, address, value)`: `*(port)=address; [pad 0]; *(port)=value`.
POKE_WRITERS = (0xC0062D28, 0xC0062D60)
# RAM region of the 16-byte channel records (BSS, filled by the static initialiser at 0xC00002A4).
CHAN_REGION = (0xC0544000, 0xC054B000)
INIT_ENTRY = 0xC00002A4
# VERIFIED: per-target flag words at 0xC0278E38 + 0x154 (path 2 when non-zero) and the target ->
# comm object map at 0xC0278E38 + 0x134 (static data of the image, read by `tools/arm_params.py`).
TARGET_FLAG_BASE = 0xC0278F8C
TARGET_COMM_BASE = 0xC0278F6C
NUM_TARGETS = 8


def u32(data: bytes, address: int, base: int = LOAD_BASE) -> int:
    return struct.unpack_from("<I", data, address - base)[0]


def decode_add_imm(insn: int) -> tuple[int, int, int] | None:
    """(rd, rn, imm) for `ADD rd, rn, #imm` (AL condition), else None."""
    if (insn >> 28) != 0xE or (insn >> 26) & 3 != 0 or not (insn >> 25) & 1:
        return None
    if (insn >> 21) & 0xF != 0x4:
        return None
    return (insn >> 12) & 0xF, (insn >> 16) & 0xF, ax.ror32(insn & 0xFF, ((insn >> 8) & 0xF) * 2)


def decode_shifted_lsl4(insn: int) -> tuple[int, int, int] | None:
    """(rd, rn, rm) for `ADD rd, rn, rm, lsl #4` or `LDR rd, [rn, rm, lsl #4]`, else None."""
    if (insn >> 28) != 0xE:
        return None
    if (insn >> 25) & 7 == 0 and (insn >> 21) & 0xF == 0x4 and not (insn >> 4) & 1:
        if (insn >> 7) & 0x1F == 4 and (insn >> 5) & 3 == 0:
            return (insn >> 12) & 0xF, (insn >> 16) & 0xF, insn & 0xF
        return None
    if (insn >> 25) & 7 == 3 and (insn >> 24) & 1 and not (insn >> 22) & 1 and (insn >> 20) & 1:
        if (insn >> 23) & 1 and (insn >> 7) & 0x1F == 4 and (insn >> 5) & 3 == 0:
            return (insn >> 12) & 0xF, (insn >> 16) & 0xF, insn & 0xF
    return None


def decode_str_base(insn: int) -> tuple[int, int, int] | None:
    """(rt, rn, imm) for `STR rt, [rn, #imm]` (AL, positive immediate offset), else None."""
    if (insn >> 28) != 0xE or (insn >> 25) & 7 != 2:
        return None
    pre, up, byte, wback, load = ((insn >> s) & 1 for s in (24, 23, 22, 21, 20))
    if not pre or not up or byte or wback or load:
        return None
    return (insn >> 12) & 0xF, (insn >> 16) & 0xF, insn & 0xFFF


def is_push_lr(insn: int) -> bool:
    """`STMDB sp!, {..., lr}` (the prologue of every sender)."""
    return (insn & 0xFFFF0000) == 0xE92D0000 and bool(insn & 0x4000)


def function_start(data: bytes, address: int, base: int = LOAD_BASE, limit: int = 0x2000) -> int:
    """Address of the closest `push {..., lr}` at or before `address` (the function prologue)."""
    a = address
    while a > address - limit and a >= base:
        if is_push_lr(ax.word_at(data, a - base)):
            return a
        a -= 4
    return address


@dataclass
class Sender:
    """One function that contains flow-control calls or wrapper calls."""

    function: int
    sites: list[int] = field(default_factory=list)
    wrapper_calls: list[int] = field(default_factory=list)
    tables: list[int] = field(default_factory=list)
    offsets: list[int] = field(default_factory=list)  # confirmed (a, 4a) pairs, word offsets
    other_adds: list[int] = field(default_factory=list)  # immediates added to a base, unpaired


def table_literals(data: bytes, lo: int, hi: int, base: int = LOAD_BASE) -> list[int]:
    """Channel-table addresses used with `lsl #4` indexing in `[lo, hi)` (literal loads tracked)."""
    regs: dict[int, int] = {}
    found: list[int] = []
    for a in range(lo, hi, 4):
        if a - base < 0 or a - base + 4 > len(data):
            break
        insn = ax.word_at(data, a - base)
        lit = ax.decode_ldr_literal(insn, a)
        if lit is not None:
            value = ax.literal_value(data, lit, base)
            if value is not None:
                regs[(insn >> 12) & 0xF] = value
            continue
        shifted = decode_shifted_lsl4(insn)
        if shifted is not None:
            _, rn, _ = shifted
            value = regs.get(rn)
            if (
                value is not None
                and CHAN_REGION[0] <= value < CHAN_REGION[1]
                and value not in found
            ):
                found.append(value)
    return found


def immediates_stored(
    data: bytes, lo: int, hi: int, base: int = LOAD_BASE
) -> list[tuple[int, int]]:
    """(address, imm) of each `add rd, rn, #imm` whose result is stored by `str rd,[..]` soon."""
    out: list[tuple[int, int]] = []
    for a in range(lo, hi, 4):
        if a - base < 0 or a - base + 16 > len(data):
            break
        add = decode_add_imm(ax.word_at(data, a - base))
        if add is None:
            continue
        rd = add[0]
        for k in range(1, 4):
            st = decode_str_base(ax.word_at(data, a + 4 * k - base))
            if st is not None and st[0] == rd:
                out.append((a, add[2]))
                break
    return out


def wrapper_arg_imm(data: bytes, call: int, base: int = LOAD_BASE) -> int | None:
    """Immediate of the `add r1, rX, #imm` that feeds r1 of a wrapper call (looks 8 words back)."""
    for k in range(1, 9):
        add = decode_add_imm(ax.word_at(data, call - 4 * k - base))
        if add is not None and add[0] == 1:
            return add[2]
    return None


def pair_offsets(imms: list[int]) -> tuple[list[int], list[int]]:
    """Split immediates into confirmed `(a, 4a)` pairs and the rest.

    Path 1 adds `a` (words), path 2 adds `4a` (bytes). Values 1 to 3 are the bookkeeping
    increments of the comm object and are dropped.
    """
    have = {i for i in imms if i > 3}
    paired = sorted(a for a in have if a * 4 in have)
    rest = sorted(a for a in have if a not in paired and (a % 4 or a // 4 not in paired))
    return paired, rest


def find_senders(data: bytes, base: int = LOAD_BASE, end: int = CODE_END) -> dict[int, Sender]:
    """All functions that call the flow control or a wrapper, with tables and offsets."""
    senders: dict[int, Sender] = {}
    for site in ax.bl_callers(data, FLOW_CONTROL, base, end):
        f = function_start(data, site, base)
        senders.setdefault(f, Sender(f)).sites.append(site)
    for w in (*EMIFA_WRITERS, *POKE_WRITERS):
        for site in ax.bl_callers(data, w, base, end):
            f = function_start(data, site, base)
            senders.setdefault(f, Sender(f)).wrapper_calls.append(site)
    for f, s in senders.items():
        last = max(s.sites + s.wrapper_calls)
        # the next prologue (or 0x600 bytes) bounds the function body
        stop = last + 0x200
        for a in range(f + 4, last + 0x200, 4):
            if is_push_lr(ax.word_at(data, a - base)) and a > last:
                stop = a
                break
        s.tables = table_literals(data, f - 0x10, stop, base)
        imms = [i for _, i in immediates_stored(data, f - 0x10, stop, base)]
        imms += [i for c in s.wrapper_calls if (i := wrapper_arg_imm(data, c, base)) is not None]
        s.offsets, s.other_adds = pair_offsets(imms)
    return senders


def read_flags(data: bytes, base: int = LOAD_BASE) -> list[dict[str, int]]:
    """Per-target path flag and comm-object index from the static data of the image."""
    return [
        {
            "target": t,
            "path2": u32(data, TARGET_FLAG_BASE + 4 * t, base),
            "comm": u32(data, TARGET_COMM_BASE + 4 * t, base),
        }
        for t in range(NUM_TARGETS)
    ]


def read_table(
    mem: bytes, table: int, count: int, base: int = LOAD_BASE
) -> list[tuple[int, int, int, int]]:
    """`count` 16-byte records `{target, base, size, tag}` of a channel table in a memory image."""
    return [struct.unpack_from("<4I", mem, table - base + 16 * k) for k in range(count)]


def memory_image(
    data: bytes, size: int = 0x01000000, base: int = LOAD_BASE
) -> bytes:  # pragma: no cover
    """Run the static initialiser with `unicorn` and return the memory after it (needs unicorn)."""
    try:
        from unicorn import UC_ARCH_ARM, UC_HOOK_CODE, UC_MODE_ARM, Uc, UcError  # type: ignore
        from unicorn.arm_const import UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_SP  # type: ignore
    except ImportError as exc:
        raise SystemExit(
            "emulate needs unicorn: uv run --with unicorn python tools/arm_params.py ..."
        ) from exc
    mu = Uc(UC_ARCH_ARM, UC_MODE_ARM)
    mu.mem_map(base, size)
    mu.mem_write(base, data)
    mu.mem_map(0x70000000, 0x200000)
    mu.mem_map(0x80000000, 0x100000)
    mu.reg_write(UC_ARM_REG_SP, 0x700F0000)
    mu.mem_write(0x70100000, b"\x00\x00\xa0\xe1" * 8)
    mu.reg_write(UC_ARM_REG_LR, 0x70100000)
    skip = [None]

    def hook(
        uc, address, _size, _user
    ):  # skip every external `bl`: the initialiser only stores constants
        insn = struct.unpack("<I", uc.mem_read(address, 4))[0]
        if (insn >> 24) == 0xEB:
            skip[0] = address + 4
            uc.emu_stop()

    mu.hook_add(UC_HOOK_CODE, hook)
    pc = INIT_ENTRY
    for _ in range(200000):
        try:
            mu.emu_start(pc, 0x70100000, count=20_000_000)
        except UcError as exc:
            raise SystemExit(
                f"emulation stopped: {exc} at {mu.reg_read(UC_ARM_REG_PC):#x}"
            ) from exc
        if skip[0] is None:
            break
        pc, skip[0] = skip[0], None
    return bytes(mu.mem_read(base, size))


def sender_rows(
    senders: dict[int, Sender], mem: bytes | None = None, base: int = LOAD_BASE
) -> list[dict[str, str]]:
    """CSV rows `sender -> table -> offsets`, one per (function, table)."""
    rows = []
    for f in sorted(senders):
        s = senders[f]
        tables = s.tables or [0]
        for t in tables:
            first = ""
            if mem is not None and t:
                rec = read_table(mem, t, 1, base)[0]
                first = f"target={rec[0]} base={rec[1]:#x}"
            rows.append(
                {
                    "function": f"{f:#010x}",
                    "flow_control_calls": str(len(s.sites)),
                    "wrapper_calls": str(len(s.wrapper_calls)),
                    "table": f"{t:#010x}" if t else "",
                    "word_offsets": " ".join(f"{o:#x}" for o in s.offsets),
                    "byte_offsets_path2": " ".join(f"{o * 4:#x}" for o in s.offsets),
                    "other_immediates": " ".join(f"{o:#x}" for o in s.other_adds),
                    "first_record": first,
                }
            )
    return rows


def table_extents(tables: list[int], cap: int = 48) -> dict[int, int]:
    """Record count per table: distance to the next table start (tables are contiguous), capped."""
    ordered = sorted(set(tables))
    out = {}
    for i, t in enumerate(ordered):
        nxt = ordered[i + 1] if i + 1 < len(ordered) else t + 16 * cap
        out[t] = min(cap, (nxt - t) // 16)
    return out


def record_rows(mem: bytes, tables: list[int], base: int = LOAD_BASE) -> list[dict[str, str]]:
    """CSV rows for every record of every table: `table, index, target, base, size, tag`."""
    rows = []
    for t, count in table_extents(tables).items():
        for k, (target, addr, size, tag) in enumerate(read_table(mem, t, count, base)):
            rows.append(
                {
                    "table": f"{t:#010x}",
                    "index": str(k),
                    "target": str(target),
                    "base": f"{addr:#010x}",
                    "size": f"{size:#x}",
                    "tag": f"{tag:#x}",
                }
            )
    return rows


def write_csv(rows: list[dict[str, str]], out: Path) -> None:
    with out.open("w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def summarize(senders: dict[int, Sender]) -> dict[str, int]:
    """Counters used by the research note and the tests."""
    by_table: dict[int, int] = defaultdict(int)
    for s in senders.values():
        for t in s.tables:
            by_table[t] += len(s.sites)
    return {
        "functions": len(senders),
        "flow_control_sites": sum(len(s.sites) for s in senders.values()),
        "wrapper_calls": sum(len(s.wrapper_calls) for s in senders.values()),
        "tables": len(by_table),
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=["senders", "tables", "flags", "emulate", "records"])
    ap.add_argument(
        "image",
        type=Path,
        help="bank0_CODE.bin (senders, flags, emulate) or a memory dump (tables)",
    )
    ap.add_argument("out", nargs="?", type=Path, help="output memory dump (emulate)")
    ap.add_argument("--csv", type=Path, help="write the sender table as CSV (senders)")
    ap.add_argument(
        "--mem", type=Path, help="memory dump from `emulate`, adds the first record per table"
    )
    ap.add_argument(
        "--table", action="append", default=[], help="TABLE:COUNT, e.g. 0xC05490B8:12 (tables)"
    )
    ns = ap.parse_args(argv)
    data = ns.image.read_bytes()
    if ns.command == "flags":
        for row in read_flags(data):
            print(row)
    elif ns.command == "emulate":
        if ns.out is None:
            raise SystemExit("emulate needs an output path")
        ns.out.write_bytes(memory_image(data))
    elif ns.command == "records":
        if ns.mem is None or ns.csv is None:
            raise SystemExit("records needs --mem DUMP and --csv OUT")
        used = [t for s in find_senders(data).values() for t in s.tables]
        write_csv(record_rows(ns.mem.read_bytes(), used), ns.csv)
    elif ns.command == "tables":
        for spec in ns.table:
            addr, count = (int(x, 0) for x in spec.split(":"))
            for k, rec in enumerate(read_table(data, addr, count)):
                print(
                    f"{addr:#x}[{k}]  target={rec[0]} base={rec[1]:#010x} "
                    f"size={rec[2]:#x} tag={rec[3]:#x}"
                )
    else:
        senders = find_senders(data)
        mem = ns.mem.read_bytes() if ns.mem else None
        rows = sender_rows(senders, mem)
        if ns.csv:
            write_csv(rows, ns.csv)
        print(summarize(senders))
        for r in rows[:40]:
            print(
                r["function"],
                r["table"],
                r["word_offsets"] or "-",
                r["flow_control_calls"],
                r["wrapper_calls"],
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
