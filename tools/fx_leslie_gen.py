#!/usr/bin/env python3
"""Generate `tools/fx_model_leslie.py` from an ADI listing of the Leslie processor.

The Leslie processor `0x1256AD..0x126366` of PNO core 1 and its callee `0x1237F8` (the cabinet
routine B) are straight-line SIMD code with two `do` loops in the callee. This tool turns each
listing line into one `step(...)` call of `tools/fx_pe.py` (the style of
`tools/fx_model_rvbcore.py`):
operands become pair reads, a multifunction line reads every register before any write, DAG
updates honour `Bn` / `Ln`. It emits instruction semantics only (no data words).

    python tools/adi_dis.py PNO_core1_sram.bin 0x240000 0x254000 > pno1.txt  # 0 = pc 0x120000
    python tools/fx_leslie_gen.py pno1.txt > tools/fx_model_leslie.py

Evidence and limits: `docs/research/fx-leslie.md` s7.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

BASE = 0x120000
#: routines: name -> (first, last) listing offsets (pc - BASE).
ROUTINES = {"callee": (0x37F8, 0x390E), "main": (0x56AD, 0x6366)}

LINE = re.compile(r"^([0-9a-f]{8})  ((?:[0-9a-f]{4} ?)+)  (\S.*)$")
MEMREF = re.compile(r"^(dm|pm)\(([^)]*)\)(\s*\(LW\))?$")
DREG = re.compile(r"^[rf](\d+)$")
SREG = re.compile(r"^s(\d+)$")
DAGREG = re.compile(r"^([imlb])(\d+)$")


#: Track AV / AC of fixed add and subtract (`tools/fx_listing_gen.py`; the Leslie model needs neither).
AVAC = False


class Unsupported(Exception):
    pass


def split_top(text: str) -> list[str]:
    """Split a multifunction line at the commas outside parentheses."""
    parts, depth, cur = [], 0, ""
    for ch in text:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    parts.append(cur.strip())
    return parts


def num(s: str) -> int:
    return int(s, 0)


def sgn(v: int) -> int:
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v & 0x80000000 else v


class Step:
    """The effects of one instruction, as source text."""

    def __init__(self) -> None:
        self.w: dict[int, str] = {}
        self.wx: dict[int, str] = {}
        self.wy: dict[int, str] = {}
        self.st: list[str] = []
        self.d: dict[str, str] = {}
        self.fl: str | None = None  # flags source `(an, az)` per element
        self.cond: str | None = None
        self.tf: str | None = None

    def src(self, pc: int) -> str:
        parts = [hex(pc)]
        for key, dct in (("w", self.w), ("wx", self.wx), ("wy", self.wy)):
            if dct:
                parts.append(f"{key}={{" + ",".join(f"{k}:{v}" for k, v in dct.items()) + "}")
        if self.st:
            parts.append("st=[" + ",".join(self.st) + "]")
        if self.d:
            parts.append("d={" + ",".join(f'"{k}":{v}' for k, v in self.d.items()) + "}")
        if self.fl:
            parts.append(f"fl={self.fl}")
        if self.tf:
            parts.append(f"tf={self.tf}")
        return f"step({','.join(parts)})"


def mem_addr(arg: str, stp: Step) -> str:
    """Address expression of a `dm(...)` argument; records the DAG update of a post-modify."""
    a, _, b = (t.strip() for t in arg.partition(","))
    if not b:  # absolute
        return str(num(a))
    ma = DAGREG.match(a)
    if ma and ma.group(1) == "i":  # post-modify: Ii, then Ii += Mm / imm
        k = int(ma.group(2))
        mb = DAGREG.match(b)
        stp.d[f"i{k}"] = f"mod({k},{int(mb.group(2))})" if mb else f"modi({k},{sgn(num(b))})"
        return f"I({k})"
    mb = DAGREG.match(b)  # pre-modify: `dm(m, i)` or `dm(imm, i)`, no write back
    if not mb or mb.group(1) != "i":
        raise Unsupported(arg)
    k = int(mb.group(2))
    if ma:
        return f"plain({k},{int(ma.group(2))})"
    return f"A({k},{sgn(num(a))})"


def read_reg(tok: str) -> str:
    """Pair expression of a data register operand."""
    m = DREG.match(tok)
    if m:
        return f"r({m.group(1)})"
    m = SREG.match(tok)
    if m:
        return f"swap(r({m.group(1)}))"
    raise Unsupported(tok)


def is_float(tok: str) -> bool:
    return tok.startswith("f")


def compute(rhs: str, dst: str, stp: Step) -> bool:
    """`dst = rhs` as a compute; with `AVAC` a float result also sets AZ / AN (`IF NE` reads it)."""
    done = _compute(rhs, dst, stp)
    md = DREG.match(dst)
    if AVAC and done and md and dst[0] == "f" and stp.fl is None and int(md.group(1)) in stp.w:
        stp.fl = f"flags_val({stp.w[int(md.group(1))]})"
    return done


def _compute(rhs: str, dst: str, stp: Step) -> bool:
    """Handle a compute expression `dst = rhs`; returns False when `rhs` is not one."""
    md = DREG.match(dst)
    if not md:
        return False
    n = int(md.group(1))
    f = dst[0] == "f"
    for pat, fn in (
        (r"^(\w+)\*(\w+)$", "fmul"),
        (r"^(\w+)\+(\w+)$", "fadd" if f else "iadd"),
        (r"^(\w+)-(\w+)$", "fsub" if f else "isub"),
        (r"^\((\w+)\+(\w+)\)/2$", "favg"),
        (r"^(\w+) and (\w+)$", "iand"),
        (r"^(\w+) or (\w+)$", "ior"),
    ):
        m = re.match(pat, rhs)
        if m:
            a, b = m.group(1), m.group(2)
            if not DREG.match(b) and re.match(r"^\d+$", b):
                b = None
            if b is None:
                stp.w[n] = f"{fn}(r({DREG.match(a).group(1)}),imm({int(m.group(2))}))"
            else:
                stp.w[n] = f"{fn}({read_reg(a)},{read_reg(b)})"
            if AVAC and fn in ("iadd", "isub"):
                stp.fl = f"iflags('{fn[1:]}',{read_reg(a)},{stp.w[n].split(',', 1)[1][:-1]})"
            elif fn.startswith("i"):
                stp.fl = f"flags_of(r({n}),{stp.w[n]})"
            return True
    m = re.match(r"^(max|min)\((\w+),(\w+)\)$", rhs)
    if m:
        fn = ("f" if f else "i") + m.group(1)
        stp.w[n] = f"{fn}({read_reg(m.group(2))},{read_reg(m.group(3))})"
        if not f:
            stp.fl = f"flags_val({stp.w[n]})"
        return True
    m = re.match(r"^abs (\w+)$", rhs)
    if m:
        stp.w[n] = f"fabs({read_reg(m.group(1))})"
        return True
    m = re.match(r"^pass (\w+)$", rhs)
    if m:
        stp.w[n] = f"{'fpass' if f else 'ipass'}({read_reg(m.group(1))})"
        if not f:
            stp.fl = f"flags_val({stp.w[n]})"
        return True
    m = re.match(r"^clip (\w+) by (\w+)$", rhs)
    if m:
        stp.w[n] = f"fclip({read_reg(m.group(1))},{read_reg(m.group(2))})"
        return True
    m = re.match(r"^lshift (\w+) by (\w+)$", rhs)
    if m:
        amount = m.group(2)
        if re.match(r"^(0x[0-9a-f]+|\d+)$", amount):
            stp.w[n] = f"ishl({read_reg(m.group(1))},imm({num(amount)}))"
        else:
            stp.w[n] = f"ishl({read_reg(m.group(1))},{read_reg(amount)})"
        return True
    m = re.match(r"^float (\w+) by (\w+)$", rhs)
    if m:
        stp.w[n] = f"ffloat({read_reg(m.group(1))},{read_reg(m.group(2))})"
        return True
    m = re.match(r"^float (\w+)$", rhs)
    if m:
        stp.w[n] = f"ffloat({read_reg(m.group(1))},imm(0))"
        return True
    m = re.match(r"^fix (\w+) by (\w+)$", rhs)
    if m:
        stp.w[n] = f"ffix({read_reg(m.group(1))},{read_reg(m.group(2))})"
        return True
    m = re.match(r"^recips (\w+)$", rhs)
    if m:
        stp.w[n] = f"frecips({read_reg(m.group(1))})"
        return True
    m = re.match(r"^trunc (\w+)$", rhs)
    if m:
        stp.w[n] = f"ftrunc({read_reg(m.group(1))})"
        return True
    m = re.match(r"^fext (\w+) by (0x[0-9a-f]+):(0x[0-9a-f]+)( \(se\))?$", rhs)
    if m:
        pos, ln, se = num(m.group(2)), num(m.group(3)), bool(m.group(4))
        stp.w[n] = f"fext_p({read_reg(m.group(1))},imm({pos}),imm({ln}),imm({int(se)}))"
        return True
    m = re.match(r"^fdep (\w+) by (0x[0-9a-f]+):(0x[0-9a-f]+)$", rhs)
    if m:
        stp.w[n] = f"fdep_p({read_reg(m.group(1))},imm({num(m.group(2))}),imm({num(m.group(3))}))"
        return True
    return False


def parse_part(part: str, stp: Step) -> None:
    """One `dst = src` or special part of an instruction."""
    if part.startswith("if "):
        m = re.match(r"^if (ne|ge|tf|eq|av|ac) (.*)$", part)
        sub = Step()
        parse_part(m.group(2), sub)
        # a conditional register move / pass: keep the old value where the condition fails
        for n, e in sub.w.items():
            stp.w[n] = f"cond_{m.group(1)}(m,r({n}),{e})"
        # a conditional exchange: each element follows its own flag
        for lane, dct in ((0, sub.wx), (1, sub.wy)):
            for n, e in dct.items():
                (stp.wx if lane == 0 else stp.wy)[n] = (
                    f"cond_lane(m,'{m.group(1)}',{lane},{'x' if lane == 0 else 'y'}[{n}],{e})"
                )
        return
    m = re.match(r"^bit tst astatx (0x[0-9a-f]+)$", part)
    if m:
        stp.tf = f"astat_bit(m,{num(m.group(1))})"
        return
    m = re.match(r"^comp\((\w+),(\w+)\)$", part)
    if m:
        stp.fl = f"comp({read_reg(m.group(1))},{read_reg(m.group(2))})"
        return
    m = re.match(r"^(\w+)<->(\w+)$", part)
    if m:
        a, b = m.groups()
        ra, sb = DREG.match(a), SREG.match(b)
        if not (ra and sb):
            raise Unsupported(part)
        stp.wx[int(ra.group(1))] = f"y[{sb.group(1)}]"
        stp.wy[int(sb.group(1))] = f"x[{ra.group(1)}]"
        return
    if "=" not in part:
        raise Unsupported(part)
    dst, _, rhs = (t.strip() for t in part.partition("="))
    # stores: dm(...)=reg
    mm = MEMREF.match(dst)
    if mm:
        addr = mem_addr(mm.group(2), stp)
        lw = bool(mm.group(3)) or rhs.endswith("(LW)")
        rhs = rhs.removesuffix("(LW)").strip()
        md = DAGREG.match(rhs)
        if md:
            reg = f"{md.group(1)}{md.group(2)}"
            if lw:
                k = int(md.group(2))
                stp.st.append(f"({addr},[ir['{md.group(1)}{k}'],ir['{md.group(1)}{k + 1}']])")
            else:
                stp.st.append(f"({addr},[ir['{reg}']]*2)")
            return
        if lw:
            n = int(DREG.match(rhs).group(1))
            stp.st.append(f"({addr},r({n}))")
            stp.st.append(f"(({addr})+2,r({n + 1}))")
            return
        stp.st.append(f"({addr},{read_reg(rhs)})")
        return
    # DAG destination
    md = DAGREG.match(dst)
    if md:
        kind, k = md.group(1), int(md.group(2))
        mm = MEMREF.match(rhs)
        if mm:
            addr = mem_addr(mm.group(2), stp)
            if mm.group(3):
                stp.d[f"{kind}{k}"] = f"mem[{addr}]"
                stp.d[f"{kind}{k + 1}"] = f"mem[({addr})+1]"
            else:
                stp.d[f"{dst}"] = f"mem[{addr}]"
            return
        mod = re.match(r"^modify ?\((i\d+),(\w+)\)$", rhs)
        if mod:
            k2 = int(mod.group(1)[1:])
            b = mod.group(2)
            mb = DAGREG.match(b)
            stp.d[dst] = f"mod({k2},{int(mb.group(2))})" if mb else f"modi({k2},{sgn(num(b))})"
            return
        if re.match(r"^0x[0-9a-f]+$", rhs) or re.match(r"^\d+$", rhs):
            stp.d[dst] = str(num(rhs) & 0xFFFFFFFF)
            return
        m2 = DAGREG.match(rhs)
        if m2:
            stp.d[dst] = f"ir['{rhs}']"
            return
        ms = SREG.match(rhs) or DREG.match(rhs)
        if ms:
            col = "y" if SREG.match(rhs) else "x"
            stp.d[dst] = f"wd({col}[{ms.group(1)}])"
            return
        raise Unsupported(part)
    # data destination
    ms = DREG.match(dst)
    if not ms:
        raise Unsupported(part)
    n = int(ms.group(1))
    mm = MEMREF.match(rhs)
    if mm:
        addr = mem_addr(mm.group(2), stp)
        wide = re.search(r"\bi(1|9)\b", mm.group(2)) is not None  # BDCST1 / BDCST9
        if mm.group(3):
            stp.w[n] = f"ld({addr})"
            stp.w[n + 1] = f"ld(({addr})+2)"
        else:
            stp.w[n] = f"{'ldb' if wide else 'ld'}({addr})"
        return
    if compute(rhs, dst, stp):
        return
    md = DAGREG.match(rhs)
    if md:
        stp.w[n] = f"[ir['{rhs}']]*2"
        return
    if re.match(r"^0x[0-9a-f]+$", rhs) or re.match(r"^\d+$", rhs):
        stp.w[n] = f"imm({num(rhs) & 0xFFFFFFFF})"
        return
    mr = DREG.match(rhs)
    if mr:
        stp.w[n] = f"r({mr.group(1)})"
        return
    mr = SREG.match(rhs)
    if mr:
        stp.w[n] = f"swap(r({mr.group(1)}))"
        return
    raise Unsupported(part)


def parse_listing(path: Path) -> dict[int, tuple[int, str]]:
    """`{offset: (length in words, text)}` of the listing."""
    out = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        m = LINE.match(line)
        if not m or "fail to disassemble" in line:
            continue
        parcels = m.group(2).split()
        out[int(m.group(1), 16)] = (len(parcels) * 2 // 3 if len(parcels) > 1 else 1, m.group(3))
    return out


def gen_routine(lst: dict[int, tuple[int, str]], first: int, last: int, name: str) -> list[str]:
    """Source lines of one routine function."""
    lines = [
        f"def {name}(m: Machine) -> None:",
        "    step, r, ld, ldb, A, I, mod, modi, plain, x, y, ir, mem = ops(m)",
    ]
    addrs = sorted(a for a in lst if first <= a <= last)
    loops: list[int] = []  # end offsets of the open do loops
    for a in addrs:
        while loops and a > loops[-1]:
            loops.pop()
        ind = "    " * (1 + len(loops))
        text = lst[a][1]
        if text.startswith(("rts", "call")):
            lines.append(f"{ind}step({a + BASE:#x})")
            if text.startswith("call"):
                lines.append(f"{ind}m.call()")
            continue
        if text == "nop":
            lines.append(f"{ind}step({a + BASE:#x})")
            continue
        m = re.match(r"^lcntr=(\w+), do \(pc,0x([0-9a-f]+)\) until lce$", text)
        if m:
            end = a + (int(m.group(2), 16) & 0xFFFF)
            cnt = m.group(1)
            lines.append(f"{ind}step({a + BASE:#x})")
            count = f"wd(x[{DREG.match(cnt).group(1)}])" if DREG.match(cnt) else str(num(cnt))
            lines.append(f"{ind}for _ in range({count}):")
            loops.append(end)
            continue
        stp = Step()
        try:
            for part in split_top(text):
                parse_part(part, stp)
        except Unsupported as exc:
            lines.append(f"{ind}# UNSUPPORTED {a + BASE:#x}: {text}  [{exc}]")
            lines.append(f"{ind}step({a + BASE:#x})")
            continue
        lines.append(f"{ind}{stp.src(a + BASE)}  # {text}")
    return lines


def main(argv: list[str] | None = None) -> int:
    lst = parse_listing(Path((argv or sys.argv[1:])[0]))
    out = [
        "# ruff: noqa: E741, F403, F405, E501",
        '"""Instruction-level model of the Leslie processor `0x1256AD` and its callee `0x1237F8`.',
        "",
        "Generated by `tools/fx_leslie_gen.py` from an ADI listing (instruction semantics only).",
        '"""',
        "",
        "from __future__ import annotations",
        "",
        "from fx_leslie_ops import *  # noqa: F403",
        "",
    ]
    for name, (a, b) in ROUTINES.items():
        out += gen_routine(lst, a, b, name)
        out.append("")
    sys.stdout.write("\n".join(out) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
