#!/usr/bin/env python3
# ruff: noqa: E501
"""Generate the Rust ports of the Mod 2 routines from `tools/fx_model_mod2_gen.py`.

The Python model (one `step(pc, ...)` per instruction; generated from the ADI listing by
`tools/fx_listing_gen.py` and verified against the `sharc-fx` oracle by `tools/fx_models.py`) is
read as a syntax tree, no firmware bytes and no listing, and turned into one `Pe::apply` per
executed instruction (`crates/nord-core/src/fx/fw_pe.rs`, `fw_pe_ops.rs`). Float condition flags
are emitted only where a later `IF cond` reads them.

    python tools/fx_mod2_rust_gen.py            # rewrite the routine files
    python tools/fx_mod2_rust_gen.py --check    # exit 1 when a committed file is stale
"""

from __future__ import annotations

import argparse
import ast
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tools" / "fx_model_mod2_gen.py"
FX_DIR = ROOT / "crates" / "nord-core" / "src" / "fx"
MAX_LOOP = 64  # guard on a loop count word
#: Python routine -> (Rust file, what it is).
FILES = {
    "vibe": ("fw_vibe.rs", "`0x123C84` (`kFx2Type` 6, 7 `VIBE`, module `0xC0279128`)"),
    "flang": ("fw_flang.rs", "`0x12348D` (`kFx2Type` 2, 3 `FLANG`, module `0xC02BB3A0`)"),
    "phas0": ("fw_phas0.rs", "`0x12379B` (`kFx2Type` 0 `PHAS`, module `0xC02BB1E8`)"),
    "phas1": ("fw_phas1.rs", "`0x1238FC` (`kFx2Type` 1 `PHAS`, module `0xC02BB2C4`)"),
    "ens": ("fw_ens.rs", "`0x123A7B` (`kFx2Type` 8, 9 `ENS`, module `0xC0441558`)"),
    "spin": ("fw_spin.rs", "`0x123557` (`kFx2Type` 10, 11 `SPIN`, module `0xC0278FAC`)"),
}
#: Pair functions taken over by name (all in `fw_pe.rs` / `fw_pe_ops.rs`).
PAIR_FNS = {
    "fmul", "fadd", "fsub", "fabs", "ffloat", "ftrunc", "iadd", "isub", "ishl", "fpass", "favg",
    "fclip", "frecips", "imax", "imin", "fext_p", "fdep_p", "swap",
}  # fmt: skip
CONDS = {"ne": "Ne", "eq": "Eq", "av": "Av", "ac": "Ac"}
PE_EXPORTS = [
    "Db", "Di", "Dl", "Dm", "St", "W", "Wx", "Wy", "fabs", "fadd", "ffloat", "fmul", "fsub",
    "ftrunc", "iadd", "imm",
]  # fmt: skip
OPS_EXPORTS = [
    "Cond", "favg", "fclip", "fdep_p", "fext_p", "fl_add_sub", "fl_float", "fl_int", "fpass",
    "frecips", "imax", "imin", "ishl", "isub", "swap",
]  # fmt: skip


class Emit:
    """Translator of the model's step expressions to Rust (`m` is the `Pe`)."""

    def num(self, node: ast.expr) -> int:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -self.num(node.operand)
        raise ValueError(f"not a constant: {ast.dump(node)}")

    def word(self, node: ast.expr) -> str:
        return f"{self.num(node) & 0xFFFFFFFF:#x}"

    def expr(self, node: ast.expr) -> str:
        if isinstance(node, ast.Constant | ast.UnaryOp):
            return f"{self.word(node)}u32"
        if isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name):
            who = node.value.id
            if who == "mem":
                return f"m.rd({self.expr(node.slice)})"
            if who == "ir":
                key = node.slice.value  # type: ignore[attr-defined]
                return f"m.{key[0]}[{int(key[1:])}]"
            return f"m.{who}[{self.num(node.slice)}]"
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mult):
            return f"[{self.expr(node.left.elts[0])}; 2]"  # type: ignore[attr-defined]
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            return self.call(node.func.id, node.args)
        raise ValueError(f"unsupported expression: {ast.dump(node)}")

    def call(self, fn: str, args: list[ast.expr]) -> str:
        if fn in PAIR_FNS:
            return f"{fn}({', '.join(self.expr(a) for a in args)})"
        simple = {
            "imm": lambda: f"imm({self.word(args[0])})",
            "wd": lambda: self.expr(args[0]),
            "ipass": lambda: self.expr(args[0]),
            "r": lambda: f"m.r({self.num(args[0])})",
            "ld": lambda: f"m.ld({self.expr(args[0])})",
            "ldb": lambda: f"m.ldb({self.expr(args[0])})",
            "A": lambda: f"m.a({self.num(args[0])}, {self.num(args[1])})",
            "I": lambda: f"m.i[{self.num(args[0])}]",
            "mod": lambda: f"m.dmod({self.num(args[0])}, {self.num(args[1])})",
            "modi": lambda: f"m.dmodi({self.num(args[0])}, {self.word(args[1])})",
            "plain": lambda: f"m.plain({self.num(args[0])}, {self.num(args[1])})",
            "cond_lane": lambda: (
                f"m.cond_lane(Cond::{CONDS[args[1].value]}, {self.num(args[2])}, "  # type: ignore[attr-defined]
                f"{self.expr(args[3])}, {self.expr(args[4])})"
            ),
        }
        if fn in simple:
            return simple[fn]()
        if fn.startswith("cond_"):
            return f"m.cond(Cond::{CONDS[fn[5:]]}, {self.expr(args[1])}, {self.expr(args[2])})"
        if fn == "iflags":
            return (
                f"fl_add_sub({self.expr(args[1])}, {self.expr(args[2])}, "
                f"{'true' if args[0].value == 'sub' else 'false'})"  # type: ignore[attr-defined]
            )
        if fn in ("flags_val", "flags_of"):
            inner = args[-1]
            name = inner.func.id if isinstance(inner, ast.Call) else "f"  # type: ignore[attr-defined]
            kind = "fl_int" if name.startswith("i") else "fl_float"
            return f"{kind}({self.expr(inner)})"
        raise ValueError(f"unsupported call: {fn}")

    def items(self, node: ast.expr):
        assert isinstance(node, ast.Dict)
        for k, v in zip(node.keys, node.values, strict=True):
            assert k is not None
            yield (k.value if isinstance(k, ast.Constant) and isinstance(k.value, str) else self.num(k)), v  # fmt: skip

    def step(self, call: ast.Call, keep_flags: bool) -> str:
        kw = {k.arg: k.value for k in call.keywords}
        effs: list[str] = []
        for key, name in (("w", "W"), ("wx", "Wx"), ("wy", "Wy")):
            effs += [f"{name}({n}, {self.expr(v)})" for n, v in self.items(kw[key])] if key in kw else []  # fmt: skip
        for tup in kw["st"].elts if "st" in kw else []:
            effs.append(f"St({self.expr(tup.elts[0])}, {self.expr(tup.elts[1])})")
        for key, v in self.items(kw["d"]) if "d" in kw else ():
            effs.append(f"D{key[0]}({int(key[1:])}, {self.expr(v)})")
        if "fl" in kw and keep_flags:
            effs.append(self.fl(kw["fl"]))
        return f"m.apply(&[{', '.join(effs)}]);" if effs else "m.apply(&[]);"

    def fl(self, node: ast.expr) -> str:
        return self.expr(node)  # `fl_*` constructors return the `Eff`


def reads_flags(node: ast.AST) -> bool:
    return any(isinstance(n, ast.Name) and n.id.startswith("cond_") for n in ast.walk(node))


def flat_steps(stmts: list[ast.stmt]) -> list[ast.Call]:
    out: list[ast.Call] = []
    for s in stmts:
        if isinstance(s, ast.Expr) and isinstance(s.value, ast.Call):
            out.append(s.value)
        elif isinstance(s, ast.For):
            out += flat_steps(s.body)
    return out


def kept_flag_steps(stmts: list[ast.stmt]) -> set[int]:
    """`id`s of the steps whose flags a later conditional in the routine reads."""
    keep: set[int] = set()
    last: ast.Call | None = None
    for call in flat_steps(stmts):
        if reads_flags(call):
            if last is not None:
                keep.add(id(last))
        elif any(k.arg == "fl" for k in call.keywords):
            last = call
    return keep


def body(emit: Emit, stmts: list[ast.stmt], keep: set[int], depth: int) -> list[str]:
    out: list[str] = []
    pad = "    " * depth
    for s in stmts:
        if isinstance(s, ast.Expr) and isinstance(s.value, ast.Call):
            out.append(pad + emit.step(s.value, id(s.value) in keep))
        elif isinstance(s, ast.For):
            count = emit.expr(s.iter.args[0])  # type: ignore[attr-defined]
            out.append(f"{pad}for _ in 0..{count}.min(MAX_LOOP) {{")
            out += body(emit, s.body, keep, depth + 1)
            out.append(f"{pad}}}")
    return out


def render(name: str, fn: ast.FunctionDef, what: str) -> str:
    keep = kept_flag_steps(fn.body)
    lines = body(Emit(), fn.body[1:], keep, 1)
    text = "\n".join(lines)
    pe_use = [n for n in PE_EXPORTS if re.search(rf"\b{n}\b", text)]
    ops_use = [n for n in OPS_EXPORTS if re.search(rf"\b{n}\b", text)]
    return "\n".join(
        [
            f"//! Instruction-level port of the FX routine {what}.",
            "//!",
            "//! GENERATED by `python tools/fx_mod2_rust_gen.py` from `tools/fx_model_mod2_gen.py`; do",
            "//! not edit. VERIFIED: the model reproduces the routine word for word against the",
            "//! `sharc-fx` oracle (`tools/fx_models.py check`, `docs/research/fx-mod2-types.md`) and the",
            "//! golden vectors of `tests_mod2_golden.rs` pin this port to the model.",
            "",
            "use super::fw_pe::{" + ", ".join(pe_use) + ", Mem, Pe};",
            *(["use super::fw_pe_ops::{" + ", ".join(ops_use) + "};"] if ops_use else []),
            "",
            "/// Longest loop count word the port honours (a block count is at most a few).",
            "const MAX_LOOP: u32 = 64;",
            "",
            "/// Runs the routine once.",
            "#[allow(clippy::too_many_lines, clippy::unreadable_literal)]",
            "#[rustfmt::skip]",
            "pub(super) fn run<M: Mem>(m: &mut Pe<M>) {",
            *lines,
            "}",
            "",
        ]
    ).replace("\n\n\n", "\n\n")


def rustfmt(text: str) -> str:
    """The text as `cargo fmt` leaves it (the body keeps its `#[rustfmt::skip]` layout); the
    committed files are formatted, so `--check` compares formatted output."""
    run = subprocess.run(
        ["rustfmt", "--edition", "2021", "--emit", "stdout"],
        input=text,
        capture_output=True,
        text=True,
        check=False,
    )
    if run.returncode != 0:
        return text
    return run.stdout


def files() -> dict[str, str]:
    tree = ast.parse(MODEL.read_text(encoding="utf-8"))
    fns = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
    return {FILES[n][0]: rustfmt(render(n, fns[n], FILES[n][1])) for n in FILES if n in fns}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--check", action="store_true")
    ns = ap.parse_args(argv)
    stale = []
    produced = files()
    for name, text in produced.items():
        path = FX_DIR / name
        if ns.check:
            if not path.exists() or path.read_text(encoding="utf-8") != text:
                stale.append(name)
        else:
            path.write_text(text, encoding="utf-8", newline="\n")
    print(f"{len(produced)} files")
    if stale:
        print("stale:", ", ".join(stale))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
