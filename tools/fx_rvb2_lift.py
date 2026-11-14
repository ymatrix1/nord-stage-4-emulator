#!/usr/bin/env python3
# ruff: noqa: E501
"""Lift the SHARC listing of the FX reverb routines of types 2..11 to a step model.

The reverb of types 2..11 is the module `0xC02BB44C`: the 4-frame wrapper `0x120FDB` (three
fs/4 biquad stages, hold, second bank, mix), which calls the packed multi-tap core `0x120D8A`
(`docs/research/fx-reverb-leslie.md` s11, s12). This tool reads the `sharc-disasm` listing of the
FX Core 2 image (a local file, never committed), translates every instruction form into the
register-pair step of `tools/fx_pe.py` (`Machine.step`) and writes

* `tools/fx_model_rvb2.py`: the Python model, one `step(pc, ...)` per executed instruction
  (verified against `sharc-fx --trace` by `tools/fx_rvb2_check.py`);
* `crates/nord-core/src/fx/fw_rvb2_steps.rs`: the same steps as straight-line Rust for the
  register machine of `fw_rvb2_core.rs`.

    python tools/fx_rvb2_lift.py SRAM/core2_sram.bin [--sharc-disasm PATH]

The instruction semantics are those of the existing models (SIMD pair registers, `BDCST1` / `BDCST9`
broadcast loads through `I1` / `I9`, circular buffers, `[pre-mod]` = `I + M` without write-back);
unknown forms stop the translation.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL_OUT = ROOT / "tools" / "fx_model_rvb2.py"
WRAP_OUT = ROOT / "tools" / "fx_model_rvb2_wrap.py"
RUST_OUT = ROOT / "crates" / "nord-core" / "src" / "fx" / "fw_rvb2_steps.rs"
DISASM = Path("C:/dev/nord-stage-4-emulator/target/release/sharc-disasm.exe")

#: (name, first pc, end pc exclusive) of the lifted routines (FX Core 2 image).
ROUTINES = (("core", 0x120D8A, 0x120F7E), ("wrap4", 0x120FDB, 0x1211C1))
BROADCAST = {1, 9}  # VERIFIED (`fx-reverb-leslie.md` s6): BDCST1; BDCST9 is set by the same pass


@dataclass
class Ins:
    pc: int
    parcels: int
    text: str


@dataclass
class Step:
    """Effects of one instruction as expression trees (tuples)."""

    pc: int
    w: dict = field(default_factory=dict)
    wx: dict = field(default_factory=dict)
    wy: dict = field(default_factory=dict)
    st: list = field(default_factory=list)
    d: dict = field(default_factory=dict)
    after: list = field(default_factory=list)  # flag updates after the commit
    call: str | None = None


def listing(image: Path, start: int, end: int) -> list[Ins]:
    out = subprocess.run(
        [str(DISASM), str(image), hex(2 * start), hex(2 * end)], capture_output=True, text=True
    ).stdout
    res = []
    for line in out.splitlines():
        m = re.match(r"0x([0-9a-f]+)\s+(\d+)[a-z/]*\s+((?:[0-9a-f]{4}\s+)+)(.*)", line)
        if not m:
            continue
        res.append(Ins(int(m.group(1), 16) // 2, len(m.group(3).split()), m.group(4).strip()))
    return res


def split_top(text: str) -> list[str]:
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
    return int(s, 16) if s.lower().lstrip("-").startswith("0x") else int(s)


MEM = re.compile(r"(DM|PM)\((.+?)\)(\s*\[pre-mod\])?$")


def mem_operand(text: str):
    """`(addr_expr, update, broadcast_reg)` of `DM(a, b)` / `PM(a, b)`."""
    m = MEM.match(text.strip())
    assert m, text
    a, b = (s.strip() for s in m.group(2).split(","))
    pre = bool(m.group(3))
    if a.startswith("I") and not re.match(r"I\d+$", b):  # DM(Ik, Mm|imm)
        k = int(a[1:])
        if re.match(r"M\d+$", b):
            mm = int(b[1:])
            if pre:
                return ("plain", k, mm), None, k
            return ("I", k), ("mod", k, mm), k
        n = num(b)
        if pre:
            return ("plainn", k, n), None, k
        return ("I", k), ("modn", k, n), k
    if re.match(r"I\d+$", b):  # DM(imm, Ik)
        k = int(b[1:])
        return ("A", k, num(a)), None, k
    raise ValueError(text)


REG = re.compile(r"([RFS])(\d+)$")


def rd(tok: str):
    m = REG.match(tok.strip())
    assert m, tok
    return ("r", int(m.group(2))) if m.group(1) != "S" else ("s", int(m.group(2)))


def operand_val(tok: str):
    tok = tok.strip()
    if REG.match(tok):
        return rd(tok)
    if re.match(r"I\d+$", tok):
        return ("pairI", int(tok[1:]))
    return ("imm", num(tok))


FOPS = {"+": "fadd", "-": "fsub", "*": "fmul"}
IOPS = {"+": "iadd", "-": "isub"}


def lift_sub(st: Step, t: str, cond: str | None = None) -> None:  # noqa: C901, PLR0912, PLR0915
    t = t.strip()
    m = re.match(r"IF (NE|EQ|NOT_SZ) (.+)$", t)
    if m:
        lift_sub(st, m.group(2), m.group(1))
        return
    # stores
    m = re.match(r"((?:DM|PM)\(.+?\)(?:\s*\[pre-mod\])?)\s*=\s*(.+)$", t)
    if m and not t.startswith(("R", "F", "I", "M", "L", "B")):
        addr, upd, _ = mem_operand(m.group(1))
        src = m.group(2).strip()
        assert REG.match(src), t
        st.st.append((addr, rd(src)))
        if upd:
            st.d[f"i{addr[1]}"] = upd
        return
    # loads
    m = re.match(r"([RFIMLB]\d+)\s*=\s*((?:DM|PM)\(.+)$", t)
    if m:
        addr, upd, k = mem_operand(m.group(2))
        dst = m.group(1)
        if dst[0] in "RF":
            ld = ("ldb" if k in BROADCAST else "ld", addr)
            val = ("cnd", cond, ld, ("r", int(dst[1:]))) if cond else ld
            st.w[int(dst[1:])] = val
        else:
            assert not cond or cond == "NOT_SZ", t
            val = ("mem", addr)
            if cond:
                val = ("cndw", cond, val, ("dag", dst.lower()))
            st.d[dst.lower()] = val
        if upd:
            st.d[f"i{k}"] = upd
        return
    # swaps
    m = re.match(r"([RF])(\d+)\s*<->\s*S(\d+)$", t)
    if m:
        assert not cond
        st.wx[int(m.group(2))] = ("y", int(m.group(3)))
        st.wy[int(m.group(3))] = ("x", int(m.group(2)))
        return
    m = re.match(r"([RF])(\d+)\s*=\s*S(\d+)$", t)
    if m:
        v = ("swap", ("r", int(m.group(3))))
        st.w[int(m.group(2))] = ("cnd", cond, v, ("r", int(m.group(2)))) if cond else v
        return
    # DAG operations
    m = re.match(r"(I\d+)\s*=\s*MODIFY\((I\d+),\s*(.+)\)$", t)
    if m:
        k = int(m.group(1)[1:])
        a = m.group(3).strip()
        if m.group(2) != m.group(1):  # MODIFY(I1, 0x12) into another register
            st.d[f"i{k}"] = ("plainimm", int(m.group(2)[1:]), num(a))
        elif re.match(r"M\d+$", a):
            st.d[f"i{k}"] = ("mod", k, int(a[1:]))
        else:
            st.d[f"i{k}"] = ("modn", k, num(a))
        return
    m = re.match(r"([IMLB]\d+)\s*=\s*(.+)$", t)
    if m:
        dst, src = m.group(1).lower(), m.group(2).strip()
        if re.match(r"[IMLB]\d+$", src):
            st.d[dst] = ("dag", src.lower())
        elif REG.match(src):
            kind, n = REG.match(src).group(1), int(REG.match(src).group(2))
            st.d[dst] = ("wx", n) if kind != "S" else ("wy", n)
        else:
            st.d[dst] = ("const", num(src))
        return
    m = re.match(r"R(\d+)\s*=\s*(I\d+)$", t)
    if m:
        st.w[int(m.group(1))] = ("pairI", int(m.group(2)[1:]))
        return
    # ALU
    m = re.match(r"([RF])(\d+)\s*=\s*(.+)$", t)
    assert m, t
    kind, n, rhs = m.group(1), int(m.group(2)), m.group(3).strip()
    mm = re.match(r"([RF]\d+)\s*([-+*])\s*([RF]\d+)(\s*\(UUI\))?$", rhs)
    if mm:
        op = (FOPS if kind == "F" else IOPS)[mm.group(2)]
        st.w[n] = (op, rd(mm.group(1)), rd(mm.group(3)))
        return
    mm = re.match(r"PASS ([RF]\d+)$", rhs)
    if mm:
        st.w[n] = rd(mm.group(1))
        st.after.append(("az", n))
        return
    mm = re.match(r"ABS (F\d+)$", rhs)
    if mm:
        st.w[n] = ("fabs", rd(mm.group(1)))
        return
    mm = re.match(r"FLOAT (R\d+)(?: BY (R\d+))?$", rhs)
    if mm:
        st.w[n] = ("ffloat", rd(mm.group(1)), rd(mm.group(2)) if mm.group(2) else ("imm", 0))
        return
    mm = re.match(r"TRUNC (F\d+)$", rhs)
    if mm:
        st.w[n] = ("ftrunc", rd(mm.group(1)))
        return
    mm = re.match(r"FPACK (F\d+)$", rhs)
    if mm:
        st.w[n] = ("fpack", rd(mm.group(1)))
        return
    mm = re.match(r"FUNPACK (R\d+)$", rhs)
    if mm:
        st.w[n] = ("funpack", rd(mm.group(1)))
        return
    mm = re.match(r"FEXT (R\d+) BY (\d+):(\d+)$", rhs)
    if mm:
        st.w[n] = ("fext", rd(mm.group(1)), int(mm.group(3)), int(mm.group(2)))
        return
    mm = re.match(r"(R\d+) OR FDEP (R\d+) BY (\d+):(\d+)$", rhs)
    if mm:
        st.w[n] = ("fdep_or", rd(mm.group(1)), rd(mm.group(2)), int(mm.group(4)), int(mm.group(3)))
        return
    mm = re.match(r"BTGL (R\d+) BY (\d+)$", rhs)
    if mm:
        st.w[n] = ("btgl", rd(mm.group(1)), int(mm.group(2)))
        st.after.append(("sz", n))
        return
    if rhs == "EMUCLK":
        st.w[n] = ("imm", 0)
        return
    if REG.match(rhs):
        v = rd(rhs)
        st.w[n] = ("cnd", cond, v, ("r", n)) if cond else v
        return
    st.w[n] = ("imm", num(rhs))


def lift_ins(ins: Ins) -> Step:
    st = Step(ins.pc)
    t = ins.text
    if t == "NOP":
        return st
    if t.startswith("CALL"):
        st.call = "core"
        return st
    for sub in split_top(t):
        lift_sub(st, sub)
    return st


# --- rendering -------------------------------------------------------------------------------
def py(e) -> str:  # noqa: C901, PLR0911, PLR0912
    k = e[0]
    simple = {"r": lambda: f"r({e[1]})", "s": lambda: f"r({e[1]})"}
    if k in simple:
        return simple[k]()
    if k == "imm":
        return f"imm({e[1]:#x})"
    if k == "const":
        return f"{e[1] & 0xFFFFFFFF:#x}"
    if k == "pairI":
        return f"[I({e[1]})] * 2"
    if k == "I":
        return f"I({e[1]})"
    if k == "A":
        return f"A({e[1]}, {e[2]})"
    if k == "plain":
        return f"plain({e[1]}, {e[2]})"
    if k == "plainn":
        return f"plainn({e[1]}, {e[2]})"
    if k == "plainimm":
        return f"plainn({e[1]}, {e[2]})"
    if k == "mod":
        return f"mod({e[1]}, {e[2]})"
    if k == "modn":
        return f"modn({e[1]}, {e[2]})"
    if k in ("ld", "ldb"):
        return f"{k}({py(e[1])})"
    if k == "mem":
        return f"mem[{py(e[1])}]"
    if k == "dag":
        return f"ir[{e[1]!r}]"
    if k == "wx":
        return f"wd(x[{e[1]}])"
    if k == "wy":
        return f"wd(y[{e[1]}])"
    if k == "x":
        return f"x[{e[1]}]"
    if k == "y":
        return f"y[{e[1]}]"
    if k == "swap":
        return f"swap({py(e[1])})"
    if k == "cnd":
        return f"cnd({e[1]!r}, flags, {py(e[2])}, {py(e[3])})"
    if k == "cndw":
        return f"cndw({e[1]!r}, flags, {py(e[2])}, {py(e[3])})"
    if k == "fext":
        return f"fext_p({py(e[1])}, {e[2]}, {e[3]}, False)"
    if k == "fdep_or":
        return f"fdep_or({py(e[1])}, {py(e[2])}, {e[3]}, {e[4]})"
    if k == "btgl":
        return f"btgl({py(e[1])}, {e[2]})"
    names = {"fpack": "fpack_p", "funpack": "funpack_p"}
    return f"{names.get(k, k)}({', '.join(py(a) for a in e[1:])})"


def py_step(st: Step) -> list[str]:
    parts = [f"{st.pc:#x}"]
    for key in ("w", "wx", "wy"):
        dct = getattr(st, key)
        if dct:
            parts.append(f"{key}={{{', '.join(f'{n}: {py(v)}' for n, v in dct.items())}}}")
    if st.st:
        parts.append(f"st=[{', '.join(f'({py(a)}, {py(v)})' for a, v in st.st)}]")
    if st.d:
        parts.append(f"d={{{', '.join(f'{k!r}: {py(v)}' for k, v in st.d.items())}}}")
    lines = [f"step({', '.join(parts)})"]
    for kind, n in st.after:
        lines.append(f"flags[{kind!r}] = flag_{kind}(r({n}))")
    if st.call:
        lines.append(f"{st.call}(m)")
    return lines


def rs(e) -> str:  # noqa: C901, PLR0911, PLR0912
    k = e[0]
    if k in ("r", "s"):
        return f"m.r({e[1]})"
    if k == "imm":
        return f"m.imm({e[1]:#x}u32)"
    if k == "const":
        return f"{e[1] & 0xFFFFFFFF:#x}u32"
    if k == "pairI":
        return f"[V::W(m.i[{e[1]}]); 2]"
    if k == "I":
        return f"m.i[{e[1]}]"
    if k == "A":
        return f"m.a({e[1]}, {e[2]})"
    if k == "plain":
        return f"m.plain({e[1]}, {e[2]})"
    if k in ("plainn", "plainimm"):
        return f"m.plainn({e[1]}, {e[2]})"
    if k == "mod":
        return f"m.modi({e[1]}, {e[2]})"
    if k == "modn":
        return f"m.modn({e[1]}, {e[2]})"
    if k in ("ld", "ldb"):
        return f"m.{k}({rs(e[1])})"
    if k == "mem":
        return f"m.rd({rs(e[1])})"
    if k == "dag":
        return f"m.{e[1][0]}[{e[1][1:]}]"
    if k == "wx":
        return f"m.rg[{e[1]}][0].wd()"
    if k == "wy":
        return f"m.rg[{e[1]}][1].wd()"
    if k == "x":
        return f"m.rg[{e[1]}][0]"
    if k == "y":
        return f"m.rg[{e[1]}][1]"
    if k == "swap":
        return f"m.swap({rs(e[1])})"
    if k == "cnd":
        return f"m.cnd({e[1]!r}, {rs(e[2])}, {rs(e[3])})"
    if k == "cndw":
        return f"m.cndw({e[1]!r}, {rs(e[2])}, {rs(e[3])})"
    if k == "fext":
        return f"m.fext({rs(e[1])}, {e[2]}, {e[3]})"
    if k == "fdep_or":
        return f"m.fdep_or({rs(e[1])}, {rs(e[2])}, {e[3]}, {e[4]})"
    if k == "btgl":
        return f"m.btgl({rs(e[1])}, {e[2]})"
    return f"m.{k}({', '.join(rs(a) for a in e[1:])})"


def rs_step(st: Step) -> str:
    pre, post = [], []
    for n, v in st.w.items():
        pre.append(f"let w{n} = {rs(v)};")
        post.append(f"m.rg[{n}] = w{n};")
    for key, lane in (("wx", 0), ("wy", 1)):
        for n, v in getattr(st, key).items():
            pre.append(f"let {key}{n} = {rs(v)};")
            post.append(f"m.rg[{n}][{lane}] = {key}{n};")
    for k, (a, v) in enumerate(st.st):
        pre.append(f"let (sa{k}, sv{k}) = ({rs(a)}, {rs(v)});")
        post.append(f"m.wr2(sa{k}, sv{k});")
    for key, v in st.d.items():
        pre.append(f"let d_{key} = {rs(v)};")
        post.append(f"m.{key[0]}[{key[1:]}] = d_{key};")
        if key[0] == "b":
            post.append(f"m.i[{key[1:]}] = d_{key};")
    tail = []
    for kind, n in st.after:
        tail.append(f"m.flag_{kind}({n});")
    if st.call:
        tail.append("m.core();")
    return f"{{ {' '.join(pre + post)} m.tick({st.pc:#x}); {' '.join(tail)} }}".replace("  ", " ")


# --- structure: loops, delayed return ----------------------------------------------------------
def build(instrs: list[Ins], i0: int = 0):
    """Nested list: `Step`, or `('loop', count_expr, [items])`."""
    items, i = [], i0
    while i < len(instrs):
        ins = instrs[i]
        m = re.match(r"DO \(PC, (\d+)\) UNTIL LCE, LCNTR = (\S+)$", ins.text)
        if m:
            last = ins.pc + int(m.group(1))
            j = i + 1
            while instrs[j].pc != last:
                j += 1
            body, _ = build(instrs[i + 1 : j + 1])
            cnt = ("imm", num(m.group(2))) if not m.group(2).startswith("R") else rd(m.group(2))
            items.append(Step(ins.pc))
            items.append(("loop", cnt, body))
            i = j + 1
            continue
        if ins.text.startswith("RTS"):
            delayed = "(DB)" in ins.text
            items.append(Step(ins.pc))
            if delayed:
                for extra in instrs[i + 1 : i + 3]:
                    items.append(lift_ins(extra))
            return items, i + (3 if delayed else 1)
        items.append(lift_ins(ins))
        i += 1
    return items, i


def emit_py(items, ind: str) -> list[str]:
    out = []
    for it in items:
        if isinstance(it, Step):
            out += [ind + ln for ln in py_step(it)]
        else:
            _, cnt, body = it
            out.append(f"{ind}for _ in range({cnt[1]:#x}):")
            out += emit_py(body, ind + "    ")
    return out


def emit_rs(items, ind: str) -> list[str]:
    out = []
    for it in items:
        if isinstance(it, Step):
            out.append(ind + rs_step(it))
        else:
            _, cnt, body = it
            out.append(f"{ind}for _ in 0..{cnt[1]:#x} {{")
            out += emit_rs(body, ind + "    ")
            out.append(f"{ind}}}")
    return out


PY_HEAD = '''#!/usr/bin/env python3
# ruff: noqa: E501, E701, E702, F403, F405, F841
"""Step model of the FX reverb routines of types 2..11 (module `0xC02BB44C`).

GENERATED by `python tools/fx_rvb2_lift.py` from the `sharc-disasm` listing of the FX Core 2 image
(`core` = `0x120D8A`, `wrap4` = `0x120FDB`); do not edit. Same step semantics as
`tools/fx_model_rvbcore.py`; `tools/fx_rvb2_check.py` compares every instruction with the oracle.
"""

from __future__ import annotations

from fractions import Fraction

from fx_pe import *  # noqa: F403
from fx_pe import M32, dag_next, deposit, extract, wd


def _int(v):
    return Fraction(int(v))


@lift
def isub(a, b):
    return (wd(a) - wd(b)) & M32


@lift
def ftrunc(a):
    q = fl(a)
    t = q.numerator // q.denominator if q >= 0 else -((-q.numerator) // q.denominator)
    return t & M32 if -(1 << 31) <= t < (1 << 31) else (M32 if t > 0 else 0x80000000)


def btgl(p, bit):
    return [wd(v) ^ (1 << bit) for v in p]


def fext_p(p, pos, ln, se):
    return [extract(wd(v), pos, ln, se) for v in p]


def fdep_or(old, p, pos, ln):
    return [wd(o) | deposit(wd(v), pos, ln) for o, v in zip(old, p, strict=True)]


def flag_az(p):
    return [wd(v) == 0 for v in p]


flag_sz = flag_az


def cnd(c, flags, new, old):
    f = flags["az"] if c in ("NE", "EQ") else flags["sz"]
    keep = [(not z) if c in ("NE", "NOT_SZ") else z for z in f]
    return [new[k] if keep[k] else old[k] for k in (0, 1)]


def cndw(c, flags, new, old):
    f = flags["sz"] if c == "NOT_SZ" else flags["az"]
    return new if ((not f[0]) if c in ("NE", "NOT_SZ") else f[0]) else old


def _ops(m):
    return (
        m.step, m.r, m.ld, m.ldb, lambda k, o: m.a(f"i{k}", o),
        lambda k: m.ir[f"i{k}"], m.mod, m.mem, m.x, m.y, m.ir,
        lambda k, j: (m.ir[f"i{k}"] + m.ir[f"m{j}"]) & M32,
        lambda k, n: (m.ir[f"i{k}"] + n) & M32,
        lambda k, n: dag_next(m.ir[f"i{k}"], n & M32, m.ir.get(f"l{k}", 0), m.ir.get(f"b{k}", 0)),
    )


'''


WRAP_HEAD = '''#!/usr/bin/env python3
# ruff: noqa: E501, E701, E702, F403, F405, F841
"""Step model of the 4-frame wrapper `0x120FDB` of the reverb types 2..11 (calls `core`).

GENERATED by `python tools/fx_rvb2_lift.py`; do not edit. See `tools/fx_model_rvb2.py`.
"""

from __future__ import annotations

from fx_model_rvb2 import _ops, core
from fx_pe import *  # noqa: F403


'''


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("image", type=Path)
    ap.add_argument("--rust", action="store_true", help="also write fw_rvb2_steps.rs")
    a = ap.parse_args(argv)
    py_lines = [PY_HEAD.rstrip("\n"), ""]
    wrap_lines = [WRAP_HEAD.rstrip("\n"), ""]
    rs_blocks = []
    for name, lo, hi in ROUTINES:
        instrs = listing(a.image, lo, hi)
        items, _ = build(instrs)
        (py_lines if name == "core" else wrap_lines).extend(
            [
                "# fmt: off",
                f"def {name}(m):",
                "    flags = m.__dict__.setdefault('flags', {'az': [False, False], 'sz': [False, False]})",
                "    step, r, ld, ldb, A, I, mod, mem, x, y, ir, plain, plainn, modn = _ops(m)  # noqa: E741",
                *emit_py(items, "    "),
                "    return m",
                "# fmt: on",
                "",
            ]
        )
        rs_blocks.append((name, emit_rs(items, "        ")))
    MODEL_OUT.write_text("\n".join(py_lines), encoding="utf-8")
    WRAP_OUT.write_text("\n".join(wrap_lines), encoding="utf-8")
    print("wrote", MODEL_OUT, WRAP_OUT)
    if not a.rust:  # the Rust machine of the port does not exist yet (fx-reverb-leslie.md s13)
        return 0
    RUST_OUT.write_text(
        "\n".join(
            [
                "//! GENERATED by `python tools/fx_rvb2_lift.py`; do not edit.",
                "use super::fw_ext40::Fp;",
                "use super::fw_rvb2_core::{Core2, V};",
                "",
                "#[allow(clippy::too_many_lines, clippy::let_and_return)]",
                "impl<F: Fp> Core2<F> {",
                *[
                    ln
                    for name, body in rs_blocks
                    for ln in [
                        "    #[rustfmt::skip]",
                        f"    pub(super) fn run_{name}(&mut self) {{",
                        "        let m = self;",
                        *body,
                        "    }",
                    ]
                ],
                "}",
                "",
            ]
        ),
        encoding="utf-8",
    )
    print("wrote", RUST_OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
