#!/usr/bin/env python3
# ruff: noqa: E501
"""Generate the Rust port of the FX reverb core `0x123F61` from the instruction model.

`tools/fx_model_rvbcore.py` is the transliteration of the routine (one `step(pc, ...)` per executed
instruction, verified word for word against the `sharc-fx` oracle by `tools/fx_rvbreal.py check`).
This script reads that file's syntax tree (no firmware bytes, no listing) and writes:

* `crates/nord-core/src/fx/fw_rvb_steps.rs`: the straight-line Rust of `run` and `_body`, one line
  per instruction against the machine of `fw_rvb_core.rs` (`Core::run_steps`, `Core::pass`);
* `crates/nord-core/src/fx/fw_rvb_words.rs`: the derived numeric words of the instance
  (`docs/research/data/fx-reverb-instance.csv`, core 2) and the type words of types 0 and 1
  (`docs/research/data/fx-type-words.csv`, organ unit). Words equal to a firmware pointer (the
  delay lines, the all-pass buffer, the state array, the PM list) are emitted as region tags: the
  Rust port lays its own memory out.

    python tools/fx_rvbcore_gen.py            # rewrite both files
    python tools/fx_rvbcore_gen.py --check    # exit 1 when a committed file is stale
"""

from __future__ import annotations

import argparse
import ast
import csv
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tools" / "fx_model_rvbcore.py"
FX_DIR = ROOT / "crates" / "nord-core" / "src" / "fx"
DATA = ROOT / "docs" / "research" / "data"

#: firmware pointer words of the instance (core 2) and the region each one names.
REGIONS = {
    0x0802F000: "LINE_A",
    0x08030388: "LINE_B",
    0x000E24E7: "AP",
    0x000B2849: "STATE",
    0x0009454B: "PM",
}
REGION_IDS = {name: k + 1 for k, name in enumerate(["LINE_A", "LINE_B", "AP", "STATE", "PM"])}
MAX_PASSES = 64  # guard on the loop count word (the image says 21)

PAIR_FNS = {"fmul": 2, "fadd": 2, "fsub": 2, "favg": 2, "fpass": 1, "swap": 1}


class Emit:
    """Translator of the model's step expressions to Rust (`m` is the `Core`)."""

    def __init__(self) -> None:
        self.names: set[str] = set()

    # --- expressions -----------------------------------------------------------------------
    def num(self, node: ast.expr) -> int:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -self.num(node.operand)
        raise ValueError(f"not a constant: {ast.dump(node)}")

    def expr(self, node: ast.expr) -> str:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return f"{node.value:#x}u32"
        if isinstance(node, ast.Name):
            if node.id in self.names:
                return node.id.lower()
            raise ValueError(f"unknown name {node.id}")
        if isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name):
            who = node.value.id
            if who == "mem":
                return f"m.rd({self.expr(node.slice)})"
            return f"m.rg[{self.num(node.slice)}][{'xy'.index(who)}]"
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mult):
            assert isinstance(node.left, ast.List) and self.num(node.right) == 2
            return f"[V::W({self.expr(node.left.elts[0])}); 2]"
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            fn, args = node.func.id, node.args
            if fn in PAIR_FNS:
                return f"m.{fn}({', '.join(self.expr(a) for a in args)})"
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
            if fn in ("mod", "plain"):
                return (
                    f"m.{'modi' if fn == 'mod' else fn}({self.num(args[0])}, {self.num(args[1])})"
                )
        raise ValueError(f"unsupported expression: {ast.dump(node)}")

    # --- statements ------------------------------------------------------------------------
    def dict_items(self, node: ast.expr):
        assert isinstance(node, ast.Dict)
        for k, v in zip(node.keys, node.values, strict=True):
            assert k is not None
            yield (k.value if isinstance(k, ast.Constant) and isinstance(k.value, str)
                   else self.num(k)), v  # fmt: skip

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
        stores = kw["st"].elts if "st" in kw else []
        for k, tup in enumerate(stores):
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
        return f"{{ {' '.join(pre + post)} m.tick({pc:#x}); }}"

    def body(self, stmts: list[ast.stmt]) -> list[str]:
        out: list[str] = []
        for s in stmts:
            if isinstance(s, ast.Expr) and isinstance(s.value, ast.Call):
                fn = s.value.func
                assert isinstance(fn, ast.Name)
                if fn.id == "step":
                    out.append(self.step(s.value))
                    continue
            if (
                isinstance(s, ast.Assign)
                and isinstance(s.value, ast.Call)
                and isinstance(s.targets[0], ast.Name)
            ):
                tgt = s.targets[0]
                self.names.add(tgt.id)
                out.append(f"let {tgt.id.lower()} = {self.expr(s.value)};")
                continue
            if isinstance(s, ast.For):  # `for _ in range(wd(x[7])): _body(m)`
                call = s.iter
                assert isinstance(call, ast.Call) and isinstance(call.args[0], ast.Call)
                count = self.expr(call.args[0])
                out.append(f"for _ in 0..{count}.min({MAX_PASSES}) {{ m.pass(); }}")
                continue
            # docstring, op-name unpacking, `mem = m.mem`, `return m`: nothing to emit
        return out


def functions() -> dict[str, list[ast.stmt]]:
    tree = ast.parse(MODEL.read_text())
    return {n.name: n.body for n in tree.body if isinstance(n, ast.FunctionDef)}


def steps_rust() -> tuple[str, int, int]:
    """The Rust source of `fw_rvb_steps.rs` and the instruction counts `(static, per pass)`."""
    fns = functions()
    run = Emit().body(fns["run"])
    one = Emit().body(fns["_body"])
    n_run = sum(1 for line in run if "m.tick(" in line)
    n_pass = sum(1 for line in one if "m.tick(" in line)
    lines = [
        "//! The reverb core `0x123F61` as straight-line Rust: one block per executed instruction.",
        "//!",
        "//! GENERATED by `python tools/fx_rvbcore_gen.py` from `tools/fx_model_rvbcore.py`; do not edit.",
        "//! VERIFIED: the model matches the `sharc-fx` oracle word for word on the real instance",
        "//! (`tools/fx_rvbreal.py check`: 16000 of 16000 words per type), and `tests_fw_rvb_golden.rs`",
        "//! checks this port against oracle output (ext40 path). The semantics of each block are the",
        "//! register-pair machine of `fw_rvb_core.rs`: every right-hand side reads the state before the",
        "//! instruction, then the effects commit together.",
        "",
        "use super::fw_ext40::Fp;",
        "use super::fw_rvb_core::{Core, V};",
        "",
        "/// Instructions executed outside the loop (`0x123F61 .. 0x12417D`).",
        "#[cfg(test)]",
        f"pub(super) const STEPS_STATIC: usize = {n_run};",
        "/// Instructions of one pass of the 21-pass loop (`0x124033 .. 0x124060`).",
        "#[cfg(test)]",
        f"pub(super) const STEPS_PASS: usize = {n_pass};",
        "",
        "#[allow(clippy::too_many_lines, clippy::let_and_return)]",
        "impl<F: Fp> Core<F> {",
        "    /// One call of the core: run the instructions of `run` (the loop calls [`Self::pass`]).",
        "    #[rustfmt::skip]",
        "    pub(super) fn run_steps(&mut self) {",
        "        let m = self;",
        *[f"        {line}" for line in run],
        "    }",
        "",
        "    /// One pass of the 21-pass loop.",
        "    #[rustfmt::skip]",
        "    fn pass(&mut self) {",
        "        let m = self;",
        *[f"        {line}" for line in one],
        "    }",
        "}",
        "",
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "",
        "    #[test]",
        "    fn the_instruction_counts_match_the_model() {",
        "        let mut c = Core::<f32>::new_for_test();",
        "        c.run_steps();",
        "        let loops = c.pass_count_for_test();",
        "        assert_eq!(c.ticks_for_test(), STEPS_STATIC + loops * STEPS_PASS);",
        "        assert_eq!(c.first_last_pc_for_test(), (0x123F61, 0x12417D));",
        "    }",
        "}",
        "",
    ]
    return "\n".join(lines), n_run, n_pass


def words_rust() -> str:
    """The Rust source of `fw_rvb_words.rs` from the two CSV files."""
    inst = []
    with open(DATA / "fx-reverb-instance.csv", newline="") as f:
        for r in csv.DictReader(f):
            if r["core"] == "2":
                inst.append((int(r["ctx"]), int(r["word"], 16)))
    plain = [(c, w) for c, w in inst if w not in REGIONS]
    ptrs = [(c, REGION_IDS[REGIONS[w]]) for c, w in inst if w in REGIONS]
    types: dict[int, list[tuple[int, int]]] = {0: [], 1: []}
    with open(DATA / "fx-type-words.csv", newline="") as f:
        for r in csv.DictReader(f):
            if r["slot"] == "Rvb" and r["kinds"] == "organ" and int(r["type"]) in types:
                w = int(r["word"], 16)
                assert w not in REGIONS
                types[int(r["type"])].append((int(r["ctx"]), w))
    for t in types.values():
        t.sort()
    assert len({len(t) for t in types.values()}) == 1
    out = [
        "//! The derived numeric words of the firmware reverb instance and of types 0 and 1.",
        "//!",
        "//! GENERATED by `python tools/fx_rvbcore_gen.py`; do not edit. Offsets are `c[n]` = word",
        "//! `I1 + n` of the context `0xB27E8`. VERIFIED: the instance words are the initial data of the FX",
        "//! DSP image (`docs/research/data/fx-reverb-instance.csv`, core 2) and the 39 type words per type",
        "//! are the ARM's capture of the organ unit (`docs/research/data/fx-type-words.csv`); the oracle",
        "//! response with exactly these words is the golden data of `tests_fw_rvb_golden.rs`. The five",
        "//! pointer words of the firmware (delay lines, all-pass buffer, state array, PM list) are region",
        "//! tags here: the port lays its memory out itself (the firmware's absolute addresses never",
        "//! reach the audio).",
        "",
        "/// Region ids of [`INSTANCE_PTRS`].",
        *[f"pub(super) const REGION_{n}: u8 = {i};" for n, i in REGION_IDS.items()],
        "",
        "/// Plain instance words `(ctx offset, word)`.",
        f"pub(super) const INSTANCE: [(i8, u32); {len(plain)}] = [",
        *[f"    ({c}, {w:#010x})," for c, w in plain],
        "];",
        "",
        "/// Instance words that hold a firmware address: `(ctx offset, region id)`.",
        f"pub(super) const INSTANCE_PTRS: [(i8, u8); {len(ptrs)}] = [",
        *[f"    ({c}, {i})," for c, i in ptrs],
        "];",
        "",
        "/// The type words `(ctx offset, word)` of type 0 and type 1.",
        f"pub(super) const TYPE_WORDS: [[(i8, u32); {len(types[0])}]; 2] = [",
        *[
            line
            for t in (0, 1)
            for line in ("    [", *[f"        ({c}, {w:#010x})," for c, w in types[t]], "    ],")
        ],
        "];",
        "",
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "",
        "    #[test]",
        "    fn the_tables_have_the_documented_shape() {",
        "        assert!(INSTANCE.iter().any(|&(c, w)| c == 0x11 && w == 0x3F7D_C91D));",
        "        assert_eq!(INSTANCE_PTRS.len(), " + str(len(ptrs)) + ");",
        "        assert_eq!(TYPE_WORDS[0].len(), TYPE_WORDS[1].len());",
        "        let tap = |t: usize, c: i8| TYPE_WORDS[t].iter().find(|e| e.0 == c).map(|e| e.1);",
        '        assert_eq!(tap(0, 43), Some(0xFFFF_F59E), "line A tap -2658");',
        '        assert_eq!(tap(1, 48), Some(0xFFFF_F01C), "line B tap -4068");',
        "    }",
        "}",
        "",
    ]
    return "\n".join(out)


def outputs() -> dict[Path, str]:
    return {FX_DIR / "fw_rvb_steps.rs": steps_rust()[0], FX_DIR / "fw_rvb_words.rs": words_rust()}


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("--check", action="store_true")
    a = p.parse_args(argv)
    stale = 0
    for path, text in outputs().items():
        if a.check:
            if not path.exists() or path.read_text() != text:
                print(f"stale: {path.relative_to(ROOT)}")
                stale += 1
        else:
            path.write_text(text, newline="\n")
            print(f"wrote {path.relative_to(ROOT)}")
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
