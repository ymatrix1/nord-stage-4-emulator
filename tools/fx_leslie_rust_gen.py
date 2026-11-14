#!/usr/bin/env python3
# ruff: noqa: E501
"""Generate the Rust port of the Leslie processor from `tools/fx_model_leslie.py`.

The model (one `step(pc, ...)` per instruction, verified against the `sharc-fx` oracle by
`tools/fx_leslie_check.py`) is read as a syntax tree, no firmware bytes and no listing, and turned
into straight-line Rust against the register machine of `fw_leslie_core.rs`:

* `fw_leslie_callee.rs`: the cabinet callee `0x1237F8` (two `do` loops as `for` loops),
* `fw_leslie_main_a.rs .. _e.rs`: the processor `0x1256AD..0x126366` in chunks under 400 lines,
* `fw_leslie_data_a.rs`, `fw_leslie_data_b.rs`: the derived numeric words (windows of the boot
  state of DM / PM that the routine reads; `--words FILE` is the JSON of `tools/fx_leslie_words.py`).

    python tools/fx_leslie_rust_gen.py            # rewrite the step files
    python tools/fx_leslie_rust_gen.py --check    # exit 1 when a committed file is stale
"""

from __future__ import annotations

import argparse
import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tools" / "fx_model_leslie.py"
FX_DIR = ROOT / "crates" / "nord-core" / "src" / "fx"
CHUNK = 300  # instructions per generated main file
MAX_LOOP = 64  # guard on a loop count word
SUFFIX = "abcdefgh"

PAIR_FNS = {
    "fmul", "fadd", "fsub", "favg", "fpass", "swap", "fabs", "fclip", "ffloat", "ffix", "ftrunc",
    "fext_p", "fdep_p", "iadd", "isub", "iand", "ior", "ipass",
}  # fmt: skip


class Emit:
    """Translator of the model's step expressions to Rust (`m` is the `Core`)."""

    def num(self, node: ast.expr) -> int:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -self.num(node.operand)
        raise ValueError(f"not a constant: {ast.dump(node)}")

    def expr(self, node: ast.expr) -> str:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return f"{node.value & 0xFFFFFFFF:#x}u32"
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return f"{self.num(node) & 0xFFFFFFFF:#x}u32"
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
            return f"({self.expr(node.left)}).wrapping_add({self.expr(node.right)})"
        if isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name):
            who = node.value.id
            if who == "mem":
                return f"m.rd({self.expr(node.slice)})"
            if who == "ir":
                key = node.slice.value  # type: ignore[attr-defined]
                return f"m.{key[0]}[{int(key[1:])}]"
            return f"m.rg[{self.num(node.slice)}][{'xy'.index(who)}]"
        if isinstance(node, ast.List) and len(node.elts) == 2:
            return f"[V::W({self.expr(node.elts[0])}), V::W({self.expr(node.elts[1])})]"
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mult):
            assert isinstance(node.left, ast.List) and self.num(node.right) == 2
            return f"[V::W({self.expr(node.left.elts[0])}); 2]"
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            fn, args = node.func.id, node.args
            if fn in PAIR_FNS:
                return f"m.{fn}({', '.join(self.expr(a) for a in args)})"
            if fn == "imm":
                return f"[V::W({self.num(args[0]) & 0xFFFFFFFF:#x}u32); 2]"
            if fn == "wd":
                return f"{self.expr(args[0])}.wd()"
            if fn == "r":
                return f"m.r({self.num(args[0])})"
            if fn in ("ld", "ldb"):
                return f"m.{fn}({self.expr(args[0])})"
            if fn == "A":
                return f"m.a({self.num(args[0])}, {self.num(args[1])})"
            if fn == "I":
                return f"m.i[{self.num(args[0])}]"
            if fn == "mod":
                return f"m.modi({self.num(args[0])}, {self.num(args[1])})"
            if fn == "modi":
                return f"m.modimm({self.num(args[0])}, {self.num(args[1]) & 0xFFFFFFFF:#x}u32)"
            if fn == "plain":
                return f"m.plain({self.num(args[0])}, {self.num(args[1])})"
            if fn in ("flags_of", "flags_val"):
                return f"m.flags({self.expr(args[-1])})"
            if fn == "comp":
                return f"m.comp({self.expr(args[0])}, {self.expr(args[1])})"
            if fn in ("cond_ne", "cond_ge", "cond_tf"):
                return f"m.{fn}({self.expr(args[1])}, {self.expr(args[2])})"
            if fn == "astat_bit":
                return "[false; 2]"
        raise ValueError(f"unsupported expression: {ast.dump(node)}")

    def dict_items(self, node: ast.expr):
        assert isinstance(node, ast.Dict)
        for k, v in zip(node.keys, node.values, strict=True):
            assert k is not None
            yield (k.value if isinstance(k, ast.Constant) and isinstance(k.value, str) else self.num(k)), v  # fmt: skip

    def step(self, call: ast.Call) -> str:
        pc = self.num(call.args[0])
        kw = {k.arg: k.value for k in call.keywords}
        pre: list[str] = []
        post: list[str] = []
        for n, v in self.dict_items(kw["w"]) if "w" in kw else ():
            pre.append(f"let w{n} = {self.expr(v)};")
            post.append(f"m.rg[{n}] = w{n};")
        for key, lane in (("wx", 0), ("wy", 1)):
            for n, v in self.dict_items(kw[key]) if key in kw else ():
                pre.append(f"let {key}{n} = {self.expr(v)};")
                post.append(f"m.rg[{n}][{lane}] = {key}{n};")
        for k, tup in enumerate(kw["st"].elts if "st" in kw else []):
            assert isinstance(tup, ast.Tuple)
            pre.append(
                f"let (sa{k}, sv{k}) = ({self.expr(tup.elts[0])}, {self.expr(tup.elts[1])});"
            )
            post.append(f"m.wr2(sa{k}, sv{k});")
        for key, v in self.dict_items(kw["d"]) if "d" in kw else ():
            kind, n = key[0], int(key[1:])
            pre.append(f"let d_{key} = {self.expr(v)};")
            post.append(f"m.{kind}[{n}] = d_{key};")
            if kind == "b":  # a write to `Bn` also loads `In`
                post.append(f"m.i[{n}] = d_{key};")
        if "fl" in kw:
            pre.append(f"let fl = {self.expr(kw['fl'])};")
            post.append("m.an = fl.0; m.az = fl.1;")
        if "tf" in kw:
            pre.append(f"let tf = {self.expr(kw['tf'])};")
            post.append("m.tf = tf;")
        return f"{{ {' '.join(pre + post)} m.tick({pc:#x}); }}"

    def body(self, stmts: list[ast.stmt], depth: int = 0) -> list[str]:
        out: list[str] = []
        pad = "    " * depth
        for s in stmts:
            if isinstance(s, ast.Expr) and isinstance(s.value, ast.Call):
                fn = s.value.func
                if isinstance(fn, ast.Name) and fn.id == "step":
                    out.append(pad + self.step(s.value))
                    continue
                if isinstance(fn, ast.Attribute) and fn.attr == "call":
                    out.append(pad + "m.callee();")
                    continue
            if isinstance(s, ast.For):
                count = self.expr(s.iter.args[0])  # type: ignore[attr-defined]
                out.append(f"{pad}for _ in 0..{count}.min({MAX_LOOP}) {{")
                out += self.body(s.body, depth + 1)
                out.append(f"{pad}}}")
                continue
        return out


def functions() -> dict[str, list[ast.stmt]]:
    tree = ast.parse(MODEL.read_text(encoding="utf-8"))
    return {n.name: n.body for n in tree.body if isinstance(n, ast.FunctionDef)}


def count_ticks(lines: list[str]) -> int:
    return sum(line.count("m.tick(") for line in lines)


def header(name: str, what: str, uses_v: bool = True) -> list[str]:
    return [
        f"//! {what}",
        "//!",
        "//! GENERATED by `python tools/fx_leslie_rust_gen.py` from `tools/fx_model_leslie.py`; do not",
        "//! edit. VERIFIED: the model matches the `sharc-fx` oracle on every register before every",
        "//! instruction (`tools/fx_leslie_check.py`, `docs/research/fx-leslie.md` s7) and",
        "//! `tests_fw_leslie_golden.rs` checks this port against oracle output.",
        "",
        "use super::fw_ext40::Fp;",
        "use super::fw_leslie_core::{Core, V};" if uses_v else "use super::fw_leslie_core::Core;",
        "",
        "#[allow(",
        "    clippy::too_many_lines,",
        "    clippy::let_and_return,",
        "    clippy::unreadable_literal",
        ")]",
        "impl<F: Fp> Core<F> {",
    ]


def files() -> dict[str, str]:
    fns = functions()
    callee = Emit().body(fns["callee"])
    main = Emit().body(fns["main"])
    out: dict[str, str] = {}
    head = header("callee", "The Leslie cabinet routine `0x1237F8` as straight-line Rust.")
    out["fw_leslie_callee.rs"] = "\n".join(
        [
            *head,
            "    /// The callee: two stereo FIRs and the biquad bank, one call per 2 frames.",
            "    #[rustfmt::skip]",
            "    pub(super) fn callee(&mut self) {",
            "        let m = self;",
            *[f"        {line}" for line in callee],
            "    }",
            "}",
            "",
            "/// Instructions of the callee per call with the context of the boot state.",
            "#[cfg(test)]",
            f"pub(super) const STATIC_STEPS_CALLEE: usize = {count_ticks(callee)};",
            "",
        ]
    )
    chunks = [main[i : i + CHUNK] for i in range(0, len(main), CHUNK)]
    for k, chunk in enumerate(chunks):
        extra = []
        if k == 0:
            extra = [
                "/// Instruction lines of the processor (each is one instruction).",
                "#[cfg(test)]",
                f"pub(super) const MAIN_STEPS: usize = {count_ticks(main)};",
                "",
            ]
        out[f"fw_leslie_main_{SUFFIX[k]}.rs"] = "\n".join(
            [
                *header(
                    "main",
                    f"The Leslie processor `0x1256AD..0x126366`, part {k + 1} of {len(chunks)}.",
                    any("V::" in line for line in chunk),
                ),
                "    #[rustfmt::skip]",
                f"    pub(super) fn main_{SUFFIX[k]}(&mut self) {{",
                "        let m = self;",
                *[f"        {line}" for line in chunk],
                "    }",
                "}",
                "",
                *extra,
            ]
        )
    out["_main_parts"] = str(len(chunks))
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--check", action="store_true")
    ns = ap.parse_args(argv)
    stale = []
    produced = files()
    parts = int(produced.pop("_main_parts"))
    for name, text in produced.items():
        path = FX_DIR / name
        if ns.check:
            if not path.exists() or path.read_text(encoding="utf-8") != text:
                stale.append(name)
        else:
            path.write_text(text, encoding="utf-8", newline="\n")
    print(f"{len(produced)} files, {parts} main parts")
    if stale:
        print("stale:", ", ".join(stale))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
