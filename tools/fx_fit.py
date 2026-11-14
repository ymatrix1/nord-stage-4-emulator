#!/usr/bin/env python3
"""Closed forms for the words the ARM sends as a function of an edit value.

`fit_series(values, words)` takes the samples `(value, 32-bit word)` of one DM word and returns the
simplest description that reproduces them: a constant, a linear map (`a + b v`), an exponential
map (`a 2^(b v)`, how frequencies and times are usually scaled) or the table itself. Words are read
as IEEE floats when their exponent is plausible and as integers otherwise (state pointers, bit
fields, sample counts). Used by `docs/research/fx-contexts.md` (`tools/fx_contexts.py`).
"""

from __future__ import annotations

import math
import struct
from dataclasses import dataclass

# EMPIRICAL (all words captured in `fx-contexts.md`): a float word of an effect context has an
# exponent between 2^-60 and 2^60; integers below 2^23 look like denormals and are not floats.
MIN_EXP, MAX_EXP = 67, 187
REL_TOL = 1e-5


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def is_float_word(word: int) -> bool:
    """True for 0, `-0.0` and words whose IEEE exponent is in the plausible range."""
    if word in (0, 0x80000000):
        return True
    return MIN_EXP <= (word >> 23) & 0xFF <= MAX_EXP


def signed(word: int) -> int:
    return word - (1 << 32) if word & 0x80000000 else word


@dataclass(frozen=True)
class Fit:
    """`kind` is `const`, `linear`, `quad`, `exp` or `table`; `a`, `b`, `c` the parameters (`a + b v
    + c v^2` for `quad`); `err` the worst error over the samples relative to the sample range;
    `is_float` the word type."""

    kind: str
    a: float
    b: float
    err: float
    is_float: bool
    c: float = 0.0

    def describe(self) -> str:
        if self.kind == "const":
            return f"{self.a:.9g}"
        if self.kind == "linear":
            return f"{self.a:.9g} + {self.b:.9g} v"
        if self.kind == "quad":
            return f"{self.a:.9g} + {self.b:.9g} v + {self.c:.9g} v^2"
        if self.kind == "exp":
            return f"{self.a:.9g} 2^({self.b:.9g} v)"
        return "table"


def _worst(pred: list[float], ys: list[float]) -> float:
    span = max(ys) - min(ys)
    scale = span if span > 0 else max(abs(y) for y in ys) or 1.0
    return max(abs(p - y) for p, y in zip(pred, ys, strict=True)) / scale


def _line(xs: list[float], ys: list[float]) -> tuple[float, float]:
    """Least squares `y = a + b x`."""
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx == 0:
        return my, 0.0
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys, strict=True)) / sxx
    return my - b * mx, b


def _quad(xs: list[float], ys: list[float]) -> tuple[float, float, float]:
    """Least squares `y = a + b x + c x^2` (centred and scaled for conditioning)."""
    import numpy as np

    scale = max(abs(x) for x in xs) or 1.0
    c2, c1, c0 = np.polyfit([x / scale for x in xs], ys, 2)
    return float(c0), float(c1 / scale), float(c2 / scale**2)


def fit_series(values: list[int], words: list[int], tol: float = REL_TOL) -> Fit:
    """Simplest closed form of `words` as a function of `values` (one sample per value)."""
    is_float = all(is_float_word(w) for w in words)
    ys = [f32(w) if is_float else float(signed(w)) for w in words]
    xs = [float(v) for v in values]
    if len(set(words)) == 1:
        return Fit("const", ys[0], 0.0, 0.0, is_float)
    if len(xs) >= 3:
        a, b = _line(xs, ys)
        err = _worst([a + b * x for x in xs], ys)
        if err <= tol:
            return Fit("linear", a, b, err, is_float)
        if len(xs) >= 4:
            qa, qb, qc = _quad(xs, ys)
            err = _worst([qa + qb * x + qc * x * x for x in xs], ys)
            if err <= tol:
                return Fit("quad", qa, qb, err, is_float, qc)
        if all(y > 0 for y in ys):
            la, lb = _line(xs, [math.log2(y) for y in ys])
            err = _worst([2 ** (la + lb * x) for x in xs], ys)
            if err <= tol:
                return Fit("exp", 2**la, lb, err, is_float)
    return Fit("table", 0.0, 0.0, 1.0, is_float)
