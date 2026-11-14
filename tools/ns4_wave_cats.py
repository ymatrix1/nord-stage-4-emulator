#!/usr/bin/env python3
"""Synth waveform category membership from the firmware's oscillator-list builder.

The OS v1.66 ARM0 image (`bank0_CODE.bin`, load base `0xC0000000`) builds the oscillator
browser tree in one straight-line routine (`0xC01ADBC0`, literal pools at `0x1AEB3C` and
`0x1B07F8`): for every ANALOG and WAVE entry it allocates a message list (wave id), a 20-byte
node and calls the list append `0xC0020AF0`. The node holds `[+0x08]` the type string,
`[+0x0C]` the category string, `[+0x10]` the entry name, and the stack words `[sp+0x4C]` /
`[sp+0x50]` the category and entry index. This tool runs that routine on a small symbolic
register machine (immediates only; loops and conditions are evaluated) and prints the tree.

    uv run --with capstone python tools/ns4_wave_cats.py target/fw_unpacked/bank0_CODE.bin
    uv run --with capstone python tools/ns4_wave_cats.py target/fw_unpacked/bank0_CODE.bin \
        --csv docs/research/data/synth-wave-categories.csv

CSV columns: `type,cat_index,category,wave_index,name,wave_id`. `wave_id` is the first argument of
the message-list constructor: the ANALOG wave id (`type` column of
`docs/research/data/synth-wave-types.csv`) or the WAVE table id. The firmware is local only and
never committed; the CSV holds names and ids.
Findings: `docs/research/synth-wave-categories.md`.
"""

from __future__ import annotations

import argparse
import csv
import io
import struct
import sys
from pathlib import Path

BASE = 0xC0000000
FN_START = 0xC01ADBC0
# The prologue (zeroing loops over the list heads) is skipped; its only live register is r6 = 0.
BODY_START = 0xC01ADC50
APPEND = 0xC0020AF0
ALLOC = 0xC025D56C
# Constructors of a wave message list: (this, id), (this, id, aux), (this, table_id).
CTOR_ID = {0xC01ACCF4, 0xC01ACBF4, 0xC01ACCD8}
TYPES = ("Analog", "Wave")
MAX_STEPS = 200_000

Value = tuple  # ('imm', n) | ('obj', k) | ('sp', off) | ('?',)


def _imm(v: Value | None) -> int | None:
    return v[1] if v and v[0] == "imm" else None


def _cstr(fw: bytes, addr: int) -> str | None:
    off = addr - BASE
    if not 0 <= off < len(fw):
        return None
    end = fw.find(b"\0", off)
    raw = fw[off:end]
    if raw and all(32 <= c < 127 for c in raw):
        return raw.decode("ascii")
    return None


def _cond(flags: tuple[int, int] | None, cc: str) -> bool | None:
    """Evaluate an ARM condition for the last `cmp a, b` (unsigned and signed views)."""
    if cc in ("", "al"):
        return True
    if flags is None:
        return None
    a, b = flags
    sa, sb = (a ^ 0x80000000) - 0x80000000, (b ^ 0x80000000) - 0x80000000
    table = {
        "eq": a == b, "ne": a != b, "hs": a >= b, "lo": a < b, "hi": a > b, "ls": a <= b,
        "ge": sa >= sb, "lt": sa < sb, "gt": sa > sb, "le": sa <= sb,
    }  # fmt: skip
    return table.get(cc)


def walk(fw: bytes) -> list[dict]:
    """Run the builder routine and return one dict per appended tree node."""
    from capstone import CS_ARCH_ARM, CS_MODE_ARM, Cs  # lazy: optional dependency
    from capstone import arm as carm
    from capstone.arm import ARM_OP_IMM, ARM_OP_MEM, ARM_OP_REG, ARM_REG_PC

    _CC = {
        getattr(carm, f"ARM_CC_{c.upper()}"): c
        for c in ("eq", "ne", "hs", "lo", "hi", "ls", "ge", "lt", "gt", "le")
    }

    md = Cs(CS_ARCH_ARM, CS_MODE_ARM)
    md.detail = True
    regs: dict[str, Value] = {"sp": ("sp", 0)}
    mem: dict[tuple, Value] = {}
    ids: dict[int, int | None] = {}
    nodes: list[dict] = []
    flags: tuple[int, int] | None = None
    nobj = 0
    regs["r6"] = ("imm", 0)
    pc = BODY_START
    word = lambda a: struct.unpack_from("<I", fw, a - BASE)[0]  # noqa: E731
    reg = lambda r: regs.get(r, ("?",))  # noqa: E731

    for _ in range(MAX_STEPS):
        ins = next(md.disasm(fw[pc - BASE : pc - BASE + 4], pc), None)
        if ins is None:
            raise RuntimeError(f"cannot decode {pc:#x}")
        m, ops, nxt = ins.mnemonic, ins.operands, pc + 4
        name = lambda o: ins.reg_name(o.reg)  # noqa: E731
        mn, cond = m, _CC.get(ins.cc, "")
        if cond and m.endswith(cond):
            mn = m[: -len(cond)]
        run = _cond(flags, cond)
        if mn == "b":
            if run is None:
                raise RuntimeError(f"unknown flags at {pc:#x}")
            if run:
                nxt = ops[0].imm & 0xFFFFFFFF
        elif mn == "bl" and run is not False:
            t = ops[0].imm & 0xFFFFFFFF
            if t == ALLOC:
                nobj += 1
                regs["r0"] = ("obj", nobj)
            elif t in CTOR_ID:
                o = reg("r0")
                if o[0] == "obj":
                    ids[o[1]] = _imm(reg("r1"))
            elif t == APPEND:
                node = mem.get(("sp", 0x54))
                typ = mem.get((node[1], 8)) if node and node[0] == "obj" else None
                if typ and _imm(typ) is not None and _cstr(fw, _imm(typ)) in TYPES:
                    cat, nm = mem.get((node[1], 0xC)), mem.get((node[1], 0x10))
                    ctor = mem.get((node[1], 4))
                    nodes.append(
                        {
                            "type": _cstr(fw, _imm(typ)),
                            "category": _cstr(fw, _imm(cat)) if _imm(cat) else None,
                            "name": _cstr(fw, _imm(nm)) if _imm(nm) else None,
                            "wave_id": ids.get(ctor[1]) if ctor and ctor[0] == "obj" else None,
                            "cat_index": _imm(mem.get(("sp", 0x4C))),
                            "wave_index": _imm(mem.get(("sp", 0x50))),
                        }
                    )
            else:
                regs["r0"] = ("?",)
        elif mn in ("pop", "ldm") and "pc" in ins.op_str:
            break
        elif mn == "cmp" and run is not False:
            a = _imm(reg(name(ops[0])))
            b = ops[1].imm & 0xFFFFFFFF if ops[1].type == ARM_OP_IMM else _imm(reg(name(ops[1])))
            flags = (a, b) if a is not None and b is not None else None
        elif run is False:
            pass
        elif mn.startswith("mov") and ops[0].type == ARM_OP_REG:
            regs[name(ops[0])] = (
                ("imm", ops[1].imm & 0xFFFFFFFF) if ops[1].type == ARM_OP_IMM else reg(name(ops[1]))
            )
        elif mn == "ldr" and ops[1].type == ARM_OP_MEM:
            mo = ops[1].mem
            base = ins.reg_name(mo.base)
            if mo.base == ARM_REG_PC:
                regs[name(ops[0])] = ("imm", word(pc + 8 + mo.disp))
            elif base == "sp":
                regs[name(ops[0])] = mem.get(("sp", mo.disp), ("?",))
            else:
                bv = reg(base)
                regs[name(ops[0])] = mem.get((bv[1], mo.disp), ("?",)) if bv[0] == "obj" else ("?",)
        elif mn == "stm" or (mn in ("str", "strd") and any(o.type == ARM_OP_MEM for o in ops[1:])):
            _store(ins, mn, ops, regs, mem, ARM_OP_MEM)
        elif mn in ("add", "sub") and len(ops) == 3 and ops[0].type == ARM_OP_REG:
            a = reg(name(ops[1]))
            b = ops[2].imm if ops[2].type == ARM_OP_IMM else _imm(reg(name(ops[2])))
            sign = 1 if mn == "add" else -1
            if a[0] == "imm" and b is not None:
                regs[name(ops[0])] = ("imm", (a[1] + sign * b) & 0xFFFFFFFF)
            elif a[0] == "sp" and b is not None:
                regs[name(ops[0])] = ("sp", a[1] + sign * b)
            elif name(ops[0]) != "sp":
                regs[name(ops[0])] = ("?",)
        elif ops and ops[0].type == ARM_OP_REG and mn not in ("tst", "teq"):
            regs[name(ops[0])] = ("?",)
        pc = nxt
    else:
        raise RuntimeError("step limit reached")
    return nodes


def _store(ins, mn, ops, regs, mem, mem_op) -> None:
    """Record `str` / `strd` / `stm` stores into the stack or into allocated objects."""
    if mn == "stm":
        base_name, vals, disp0 = ins.reg_name(ops[0].reg), ops[1:], 0
    else:
        mo = next(o for o in ops if o.type == mem_op).mem
        base_name, vals, disp0 = ins.reg_name(mo.base), ops[:-1], mo.disp
    bv = ("sp", 0) if base_name == "sp" else regs.get(base_name, ("?",))
    for k, o in enumerate(vals):
        v = regs.get(ins.reg_name(o.reg), ("?",))
        if bv[0] == "sp":
            mem[("sp", disp0 + 4 * k)] = v
        elif bv[0] == "obj":
            mem[(bv[1], disp0 + 4 * k)] = v


def rows_from_nodes(nodes: list[dict]) -> list[dict]:
    """Deduplicate the nodes and order them by `(type, cat_index, wave_index)`.

    `wave_index` is the stack word `[sp+0x50]` of the append call; every category must hold a
    permutation `0..n-1` (checked, a mismatch means the walk lost a value).
    """
    seen: set[tuple] = set()
    out = []
    for n in nodes:
        key = (n["type"], n["category"], n["name"], n["wave_id"])
        if key not in seen:
            seen.add(key)
            out.append(dict(n))
    out.sort(key=lambda r: (r["type"] != "Analog", r["cat_index"], r["wave_index"]))
    cats: dict[tuple, list[int]] = {}
    for r in out:
        cats.setdefault((r["type"], r["cat_index"]), []).append(r["wave_index"])
    for key, idx in cats.items():
        if idx != list(range(len(idx))):
            raise RuntimeError(f"category {key}: wave indices {idx} are not 0..n-1")
    return out


# The FM-H / FM-I entries are built by two nested loops (group letter, item); their bounds are the
# immediates of four `cmp` instructions (addresses in the OS v1.66 image).
FM_LOOPS = {
    "FM-H": {"items": 0xC01AF240, "groups": 0xC01AF250, "fmt": 0xC02D7EC0},
    "FM-I": {"items": 0xC01AF3F4, "groups": 0xC01AF404, "fmt": 0xC02D7ED4},
}
HARMONIC_TABLE = 0xC02D823C + 0x6C  # 25 doubles 0.5, 1 .. 24 (partial ratios)


def _cmp_imm(fw: bytes, addr: int) -> int:
    """Immediate of the `cmp rN, #imm` word at `addr` (rotation 0 only)."""
    w = struct.unpack_from("<I", fw, addr - BASE)[0]
    if w & 0xFFF0F000 != 0xE3500000:
        raise ValueError(f"{addr:#x}: {w:#010x} is not cmp rN, #imm")
    return w & 0xFFF


def fm_shape(fw: bytes) -> dict[str, dict]:
    """FM tree shape from the loop bounds: groups (letters `A..`), items per group, name format."""
    out = {}
    for typ, loop in FM_LOOPS.items():
        fmt = _cstr(fw, loop["fmt"])
        out[typ] = {
            "groups": _cmp_imm(fw, loop["groups"]),
            "items": _cmp_imm(fw, loop["items"]),
            "category_format": fmt,
        }
    ratios = [struct.unpack_from("<d", fw, HARMONIC_TABLE - BASE + 8 * i)[0] for i in range(25)]
    out["FM-H"]["ratios"] = ratios
    return out


def to_csv(rows: list[dict]) -> str:
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    w.writerow(["type", "cat_index", "category", "wave_index", "name", "wave_id"])
    for r in rows:
        w.writerow(
            [r[k] for k in ("type", "cat_index", "category", "wave_index", "name", "wave_id")]
        )
    return buf.getvalue()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("image", type=Path, help="bank0_CODE.bin")
    ap.add_argument("--csv", type=Path, help="write the derived table here")
    ap.add_argument("--fm", action="store_true", help="print the FM-H / FM-I tree shape")
    args = ap.parse_args(argv)
    if args.fm:
        for typ, v in fm_shape(args.image.read_bytes()).items():
            print(typ, v)
        return 0
    rows = rows_from_nodes(walk(args.image.read_bytes()))
    text = to_csv(rows)
    if args.csv:
        args.csv.write_text(text, encoding="utf-8", newline="\n")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
