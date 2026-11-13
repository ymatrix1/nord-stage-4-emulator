#!/usr/bin/env python3
"""Symbolic run of an FX routine model: the words it stores as expressions of its inputs.

The models are straight-line transliterations, so running one on symbols instead of numbers gives
the closed form of every stored word (`docs/research/fx-routine-models.md` was written from these
listings). Float memory words are symbols named by their offset from `I1` (`c[-0x1e]`) or the
input frame (`x0`); the integer words (pointers, counts, shift controls) keep the value of a
seeded scenario so that addresses and table indices stay concrete. Float to integer conversions,
field extracts and deposits and table loads at computed addresses become opaque nodes.

    python tools/fx_symbolic.py resonant
    python tools/fx_symbolic.py lfo --seed 1
"""

from __future__ import annotations

import argparse
import contextlib
import importlib
import random
import sys
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_pe

#: routine -> (module, function, scenario addresses that stay concrete as `CTX + offset`)
ROUTINES = {
    "resonant": ("fx_model_resonant", "resonant_0x1243f4", ()),
    "resonant4": ("fx_model_resonant4", "resonant_0x124497", ()),
    "allpass": ("fx_model_allpass", "allpass_0x122c39", (-0x1D, -0x1F, -0x20, -0x1C, -0x1B, 4)),
    "lfo": ("fx_model_lfo", "lfo_0x1228cf", (-2, -1, 0x15, -0x15, -0x14, 2)),
    "delay": (
        "fx_model_delay",
        "delay_0x1206b0",
        (6, 0xF, 0x10, 0x11, 0x12, 0x13, -0x1E, -0x1D, -0x1C),
    ),
}
LOOP_COUNT = {"lfo": 0x15, "allpass": 4}  # context word holding the block-loop count
ABSOLUTE = (0xC03D4, 0xC03D5, 0xC03D6, 0xB6000, 0xB6001)  # pass pointers and the phase counter


class Sym:
    """An expression node, interned so that equal sub-expressions are shared."""

    cache: dict = {}

    def __init__(self, op, args=(), name=None):
        self.op, self.args, self.name = op, args, name

    @classmethod
    def mk(cls, op, *args, name=None):
        key = (op, tuple(id(a) if isinstance(a, Sym) else a for a in args), name)
        if key not in cls.cache:
            cls.cache[key] = cls(op, args, name)
        return cls.cache[key]

    @staticmethod
    def lift(v):
        return v if isinstance(v, Sym) else Sym.mk("const", Fraction(v))

    def __add__(s, o):
        return Sym.mk("+", s, Sym.lift(o))  # noqa: E704

    def __radd__(s, o):
        return Sym.mk("+", Sym.lift(o), s)  # noqa: E704

    def __sub__(s, o):
        return Sym.mk("-", s, Sym.lift(o))  # noqa: E704

    def __rsub__(s, o):
        return Sym.mk("-", Sym.lift(o), s)  # noqa: E704

    def __mul__(s, o):
        return Sym.mk("*", s, Sym.lift(o))  # noqa: E704

    def __rmul__(s, o):
        return Sym.mk("*", Sym.lift(o), s)  # noqa: E704

    def __truediv__(s, o):
        return Sym.mk("/", s, Sym.lift(o))  # noqa: E704

    def __neg__(s):
        return Sym.mk("neg", s)  # noqa: E704

    def __abs__(s):
        return Sym.mk("abs", s)  # noqa: E704

    def __and__(s, o):
        return Sym.mk("and", s, Sym.lift(o))  # noqa: E704

    def __rand__(s, o):
        return Sym.mk("and", Sym.lift(o), s)  # noqa: E704

    def __bool__(s):
        raise TypeError("symbolic condition")


class SymMem(dict):
    """Memory whose unset words are float symbols named relative to `base`."""

    def __init__(self, base: int, names: dict[int, str]):
        super().__init__()
        self.base, self.names = base, names

    def __missing__(self, k):
        name = self.names.get(k) or (
            f"c[{k - self.base:+#x}]" if abs(k - self.base) < 0x100 else f"m[{k:#x}]"
        )
        self[k] = Sym.mk("in", name=name)
        return self[k]


def _opaque(name, orig):
    """A lifted pair function that returns a node when any element argument is symbolic."""

    def f(*ops):
        out = []
        for c in (0, 1):
            args = [o[c] for o in ops]
            if any(isinstance(x, Sym) for x in args):
                out.append(Sym.mk(name, *[Sym.lift(x) for x in args]))
            else:
                out.append(orig(*[[x, x] for x in args])[0])
        return out

    return f


@contextlib.contextmanager
def symbolic(modules: tuple[str, ...]):
    """Patch `fx_pe` for symbols and reload `modules` against it; restores everything on exit."""
    saved = dict(fx_pe.__dict__)
    fl, lift = saved["fl"], fx_pe.lift

    def lf(v):
        return Sym.lift(fl(v)) if not isinstance(v, Sym) else v

    patches = {
        "fl": lambda v: v if isinstance(v, Sym) else fl(v),
        "wd": lambda v: v if isinstance(v, Sym) else saved["wd"](v),
        "ext40": lambda x: x,
        "fadd": lift(lambda a, b: lf(a) + lf(b)),
        "fsub": lift(lambda a, b: lf(a) - lf(b)),
        "fmul": lift(lambda a, b: lf(a) * lf(b)),
        "favg": lift(lambda a, b: (lf(a) + lf(b)) / 2),
        "fneg": lift(lambda a: -lf(a)),
        "fabs": lift(lambda a: abs(lf(a))),
        "fmax": lift(lambda a, b: Sym.mk("max", lf(a), lf(b))),
        "fmin": lift(lambda a, b: Sym.mk("min", lf(a), lf(b))),
        "fclip": lift(lambda a, b: Sym.mk("clip", lf(a), lf(b))),
        "frecips": lift(lambda a: Sym.mk("recips", lf(a))),
        "comp": lambda a, b: ([Sym.mk("lt", lf(a[c]), lf(b[c])) for c in (0, 1)], None),
        "cond_ge": lambda fl_, new, old: [
            Sym.mk("if_ge", fl_[0][c], lf(new[c]), lf(old[c])) for c in (0, 1)
        ],
    }
    for nm in ("ffix", "fext_p", "fdep_p", "fdep_r", "fdep_or", "ishl", "iadd", "ffloat"):
        patches[nm] = _opaque(nm, saved[nm])
    patches["fpack_p"] = _opaque("fpack", saved["fpack_p"])
    patches["funpack_p"] = _opaque("funpack", saved["funpack_p"])
    orig_ld = fx_pe.Machine.ld

    def ld(self, addr):
        if isinstance(addr, Sym):
            return [Sym.mk("load", addr), Sym.mk("load", addr + 1)]
        return orig_ld(self, addr)

    fx_pe.__dict__.update(patches)
    fx_pe.Machine.ld = ld
    Sym.cache.clear()
    try:
        yield [importlib.reload(importlib.import_module(m)) for m in modules]
    finally:
        fx_pe.__dict__.update(saved)
        fx_pe.Machine.ld = orig_ld
        for m in modules:
            importlib.reload(importlib.import_module(m))


def render(roots: dict[str, Sym]) -> tuple[list[str], dict[str, str]]:
    """Name the shared or long sub-expressions `t0, t1, ...`; returns (definitions, roots)."""
    uses: dict[int, int] = {}

    def count(n):
        if isinstance(n, Sym):
            uses[id(n)] = uses.get(id(n), 0) + 1
            if uses[id(n)] == 1:
                for x in n.args:
                    count(x)

    for r in roots.values():
        count(r)
    names: dict[int, str] = {}
    defs: list[str] = []

    def emit(n):
        if not isinstance(n, Sym):
            return str(n)
        if id(n) in names:
            return names[id(n)]
        if n.op == "in":
            return n.name
        if n.op == "const":
            return str(n.args[0])
        a = [emit(x) for x in n.args]
        if n.op in ("+", "-", "*", "/"):
            e = f"{a[0]} {n.op} {a[1]}"
        elif n.op == "neg":
            e = f"-{a[0]}"
        else:
            e = f"{n.op}({', '.join(a)})"
        if uses.get(id(n), 0) > 1 or len(e) > 70:
            names[id(n)] = f"t{len(names)}"
            defs.append(f"{names[id(n)]} = {e}")
            return names[id(n)]
        return f"({e})"

    return defs, {k: emit(v) for k, v in roots.items()}


def run(routine: str, seed: int = 1) -> tuple[list[str], dict[str, str]]:
    """Symbolic stores of `routine` as `(definitions, {word: expression})`."""
    module, fn, concrete = ROUTINES[routine]
    with symbolic((module,)) as (mod,):
        case = mod.random_scenario(random.Random(seed))
        names = {mod.IN_ADDR + k: f"x{k}" for k in range(8)}
        mem = SymMem(mod.CTX, names)
        for off in concrete:
            mem[mod.CTX + off] = case[mod.CTX + off]
        for a in ABSOLUTE:
            if a in case:
                mem[a] = case[a]
        if routine in LOOP_COUNT:
            mem[mod.CTX + LOOP_COUNT[routine]] = 1  # one loop iteration
        machine = getattr(mod, fn)(mem, mod.IN_ADDR, mod.CTX)
        roots = {}
        for a in sorted(machine.written):
            if abs(a - mod.CTX) < 0x100:
                key = f"c[{a - mod.CTX:+#x}]'"
            elif 0 <= a - mod.IN_ADDR < 8:
                key = f"x{a - mod.IN_ADDR}'"
            else:
                key = f"m[{a:#x}]'"
            roots[key] = mem[a]
        return render(roots)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("routine", choices=sorted(ROUTINES))
    p.add_argument("--seed", type=int, default=1)
    args = p.parse_args(argv)
    defs, roots = run(args.routine, args.seed)
    print("\n".join(defs) + "\n")
    print("\n".join(f"{k} = {v}" for k, v in roots.items()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
