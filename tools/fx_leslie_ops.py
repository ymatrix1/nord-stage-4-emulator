#!/usr/bin/env python3
"""Operations the generated Leslie model (`tools/fx_model_leslie.py`) calls.

Extends `tools/fx_pe.py` with what the Leslie processor needs beyond the reverb core: a machine
with condition flags, integer subtract / and / pass, truncation, DAG immediates and a memory that
reads the static image under the boot words (`LazyMem`). Semantics follow `crates/nord-sharc`.
"""

from __future__ import annotations

import struct
import sys
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_pe
from fx_pe import (  # noqa: F401
    M32,
    Machine,
    dag_next,
    fabs,
    fadd,
    favg,
    fclip,
    fdep_p,
    fext_p,
    ffix,
    ffloat,
    fmul,
    fpass,
    frecips,
    fsub,
    iadd,
    imm,
    ishl,
    lift,
    signed32,
    swap,
    wd,
)
from fx_pe import (
    comp as _comp,
)


class LazyMem(dict):
    """Data memory: explicit words, else the word of a flat SRAM image (word `A` at byte `4 A`)."""

    def __init__(self, image: bytes, overlay: dict[int, int] | None = None):
        super().__init__(overlay or {})
        self.image = image

    def __missing__(self, key: int) -> int:
        k = key & M32
        if 4 * k + 4 > len(self.image):
            raise KeyError(f"read of address {key:#x} outside the image")
        return struct.unpack_from("<I", self.image, 4 * k)[0]

    def get(self, key, default=None):  # noqa: ANN001
        return self[key]


isub = lift(lambda a, b: (wd(a) - wd(b)) & M32)
iand = lift(lambda a, b: wd(a) & wd(b))
ior = lift(lambda a, b: wd(a) | wd(b))
ipass = lift(lambda a: wd(a))
ftrunc = lift(lambda a: _trunc(fx_pe.fl(a)))
TINY = Fraction(1, 2**126)


def _trunc(q: Fraction) -> int:
    v = int(q)  # toward zero
    return M32 if not -(1 << 31) <= v < (1 << 31) else v & M32


def flags_val(p) -> tuple[list[bool], list[bool]]:
    """`(AN, AZ)` per element of a result: integer words by sign / zero, floats by value."""
    an, az = [], []
    for v in p:
        if isinstance(v, int):
            an.append(bool(v & 0x80000000))
            az.append(v & M32 == 0)
        else:
            an.append(v < 0)
            az.append(v == 0)
    return an, az


def flags_of(_old, new):
    return flags_val(new)


def iflags(kind, a, b):
    """`(AN, AZ, AV, AC)` of a fixed add or subtract per element (`core/fixed_alu.rs`: AV is the
    signed overflow, AC the carry out, `x + !y + 1` for a subtract so `x - x` carries)."""
    out = ([], [], [], [])
    for x, y in zip(a, b, strict=True):
        x, y = wd(x), wd(y)
        exact = signed32(x) + signed32(y) if kind == "add" else signed32(x) - signed32(y)
        carry = x + y > M32 if kind == "add" else x >= y
        r = (x + y if kind == "add" else x - y) & M32
        for lst, v in zip(
            out,
            (bool(r & 0x80000000), r == 0, not -(1 << 31) <= exact < 1 << 31, carry),
            strict=True,
        ):
            lst.append(v)
    return out


def imax(a, b):
    return lift(lambda x, y: max(signed32(wd(x)), signed32(wd(y))) & M32)(a, b)


def imin(a, b):
    return lift(lambda x, y: min(signed32(wd(x)), signed32(wd(y))) & M32)(a, b)


def comp(a, b):
    return _comp(a, b)


def astat_bit(_m, _bit):
    """`BIT TST ASTATx`: the invalid-operation flag is assumed clear (checked by the trace)."""
    return [False, False]


def cond_ne(m, old, new):
    """`IF NE`: the new value where the last result was not zero."""
    return [new[c] if not m.az[c] else old[c] for c in (0, 1)]


def cond_ge(m, old, new):
    """`IF GE`: the new value where the last `COMP` was not less."""
    return [old[c] if m.an[c] else new[c] for c in (0, 1)]


def cond_eq(m, old, new):
    """`IF EQ`: the new value where the last result was zero."""
    return [new[c] if m.az[c] else old[c] for c in (0, 1)]


def cond_av(m, old, new):
    return [new[c] if m.av[c] else old[c] for c in (0, 1)]


def cond_ac(m, old, new):
    return [new[c] if m.ac[c] else old[c] for c in (0, 1)]


def cond_lane(m, kind, lane, old, new):
    """One element of a conditional exchange follows its own element's flag."""
    return cond_of(m, kind)(m, [old, old], [new, new])[lane]


def cond_of(_m, kind):
    return {
        "ne": cond_ne,
        "eq": cond_eq,
        "ge": cond_ge,
        "tf": cond_tf,
        "av": cond_av,
        "ac": cond_ac,
    }[kind]


def cond_tf(m, old, new):
    return [new[c] if m.tf[c] else old[c] for c in (0, 1)]


class LMachine(Machine):
    """`fx_pe.Machine` with condition flags and the `call (m8,i8)` of the Leslie processor."""

    def __init__(self, *a, **k):
        super().__init__(*a, **k)
        self.an, self.az, self.tf = [False, False], [False, False], [False, False]
        self.av, self.ac = [False, False], [False, False]
        self.callees: dict[int, object] = {}

    def step(self, pc, *a, fl=None, tf=None, **k):
        super().step(pc, *a, **k)
        if fl is not None:
            self.an, self.az = fl[0], fl[1]
            if len(fl) == 4:
                self.av, self.ac = fl[2], fl[3]
        if tf is not None:
            self.tf = tf

    def call(self):
        target = self.ir["i8"]
        self.callees[target](self)


def ops(m: Machine):
    """The short names of the generated steps."""
    return (
        m.step, m.r, m.ld, m.ldb, lambda k, o: m.a(f"i{k}", o),
        lambda k: m.ir[f"i{k}"], m.mod,
        lambda k, imm_: dag_next(
            m.ir[f"i{k}"], imm_ & M32, m.ir.get(f"l{k}", 0), m.ir.get(f"b{k}", 0)
        ),
        lambda k, j: (m.ir[f"i{k}"] + m.ir[f"m{j}"]) & M32, m.x, m.y, m.ir, m.mem,
    )  # fmt: skip


_ops = ops
