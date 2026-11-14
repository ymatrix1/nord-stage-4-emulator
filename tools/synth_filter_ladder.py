#!/usr/bin/env python3
"""Bit-exact Python model of the Core2 synth filter stage `0x12549D` (word 1, table index 2).

Written from the listing of ORG core 2 (`0x12549D..0x1256AD`, `0x24A93A..0x24ACDA`) and checked
against `sharc-org` (`tools/synth_filter_ladder_oracle.py`):
`docs/research/synth-stage-models-9.md`.
The stage is a 4 section cascade with a table-shaped (soft-clipped) high-passed feedback; per call
it filters 4 samples (SIMD pair: the model is one lane) and **computes the coefficient words of the
next call** (the frame words `-18, -16, -14, -12` it reads are the ones it stored the call before).

Frame words (`DM(I3 + k)`, lane y one word above lane x):

    -30  cutoff position p (float 0..1)       -28  resonance word: q = FLOAT(word, -31), idx.fr
    -18 b, -16 a, -14 c, -12 kf   coefficients of this call (stored by the previous call)
    -10 -8 -6 -4   section states   -2  last output   0  shaper output   2  feedback state

per sample (constants of the common block `I5 = 0xC28FA`: `h0 = -0.99946` at `-4`, `h1 = 0.99946`
at `-6`, `h2 = -0.99891` at `-2`; the shaper `T` at `DM(0xC02DE + i)`, 1025 words):

    P  = FIX(kf y, 31),  i = P >> 22 (10 bit signed),  fr = (P & 0x3FFFFF) / 2^22
    t  = T[i] + (T[i+1] - T[i]) fr
    fb = (h0 s0 + h1 t) - h2 s2,      s0 <- t, s2 <- fb
    u  = x - fb,                       n0 = u
    n(j+1) = (a o(j) + b n(j)) - c o(j+1)   j = 0..3,   y = n4,   o = old states, o4 = last y

and after the 4 samples, for the next call (`pos = p`, `q`):

    -18 = row0(p),  -16 = row1(p),  -14 = row2(p),  -12 = row4(p) (q 0.25)

rows of the 129 entry table at `DM(0xC08E3 + 129 r)` (pointer word `DM(0xC1360)`), interpolated
`(1 - fr) T[i] + fr T[i+1]`; the cutoff position of lane x serves both lanes.
"""

from __future__ import annotations

import csv
from fractions import Fraction
from pathlib import Path

import sharc40 as s

DATA = Path(__file__).resolve().parents[1] / "docs/research/data"
# VERIFIED (image words `DM(0xC28FA + k)`, listing operands): common constants.
H0 = s.ld(0xBF7FDC71)  # DM(-4, I5)
H1 = s.ld(0x3F7FDC71)  # DM(-6, I5)
H2 = s.ld(0xBF7FB8E2)  # DM(-2, I5)
QUARTER = s.ld(0x3E800000)  # DM(-32, I5)
ONE = Fraction(1)
STAGE = 0x12549D


def read_tables() -> tuple[list[list[int]], list[int]]:
    """Coefficient rows `[row0, row1, row2, row4]` (129 words each) and the shaper (1025 words)."""
    rows: list[list[int]] = [[], [], [], []]
    with (DATA / "synth-ladder-coef-table.csv").open() as fh:
        for r in csv.DictReader(fh):
            for k, name in enumerate(("row0", "row1", "row2", "row4")):
                rows[k].append(int(r[name], 16))
    with (DATA / "synth-ladder-shaper-table.csv").open() as fh:
        shaper = [int(r["word"], 16) for r in csv.DictReader(fh)]
    return rows, shaper


ROWS, SHAPER = read_tables()


def position(word_float: int) -> tuple[int, Fraction]:
    """Table index and fraction of a cutoff word (`FIX BY 31`, `FEXT 24:7`, `FDEP 7:24`)."""
    p = s.fix(s.ld(word_float), 31)
    return (p >> 24) & 0x7F, Fraction(p & 0xFFFFFF, 1 << 24)


def lerp_row(row: list[int], idx: int, fr: Fraction) -> Fraction:
    """`(1 - fr) T[i] + fr T[i+1]` (two multiplies and an add, 40 bit registers)."""
    lo, hi = s.ld(row[idx]), s.ld(row[idx + 1])
    return s.fadd(s.fmul(lo, s.fsub(ONE, fr)), s.fmul(hi, fr))


def coefficients(w30: int, w28: int) -> tuple[int, int, int, int]:
    """Words `(-18, -16, -14, -12)` stored for the next call from the cutoff and resonance words."""
    idx, fr = position(w30)
    c18, c16, c14 = (s.st(lerp_row(ROWS[k], idx, fr)) for k in range(3))
    q = s.float_int(w28, -31)
    c12 = s.st(s.fmul(lerp_row(ROWS[3], idx, fr), s.fmul(q, QUARTER)))
    return c18, c16, c14, c12


class Lane:
    """States of one lane: words `-10, -8, -6, -4` (sections), `-2` (output), `0`, `2`."""

    def __init__(self) -> None:
        self.sect = [0, 0, 0, 0]
        self.y = 0
        self.t = 0
        self.fb = 0


def shaper(y: Fraction, kf: Fraction) -> Fraction:
    """`T(kf y)` interpolated; `P = FIX(kf y, 31)` saturates at the 32-bit range."""
    prod = s.fmul(kf, y)
    p = max(-(1 << 31), min((1 << 31) - 1, int(s._nearest_even(prod * (1 << 31)))))
    i = p >> 22
    fr = Fraction(p & 0x3FFFFF, 1 << 22)
    lo, hi = s.ld(SHAPER[i + 512]), s.ld(SHAPER[i + 513])
    return s.fadd(lo, s.fmul(s.fsub(hi, lo), fr))


def process(lane: Lane, coefs: tuple[int, int, int, int], xs: list[int]) -> list[int]:
    """One call: the four output words for the input words `xs`.

    `coefs` are the frame words `(-18, -16, -14, -12)`.
    """
    b, a, c, kf = (s.ld(w) for w in coefs)
    y_reg = s.ld(lane.y)
    out = []
    for xw in xs:
        lane.y = s.st(y_reg)
        t = shaper(y_reg, kf)
        h = s.fadd(s.fmul(H0, s.ld(lane.t)), s.fmul(H1, t))
        fb = s.fsub(h, s.fmul(H2, s.ld(lane.fb)))
        lane.t, lane.fb = s.st(t), s.st(fb)
        u = s.fsub(s.ld(xw), fb)
        old = [s.ld(w) for w in lane.sect] + [s.ld(lane.y)]
        new = [u]
        for j in range(4):
            acc = s.fadd(s.fmul(a, old[j]), s.fmul(b, new[j]))
            new.append(s.fsub(acc, s.fmul(c, old[j + 1])))
        lane.sect = [s.st(v) for v in new[:4]]
        y_reg = new[4]
        out.append(s.st(y_reg))
    lane.y = s.st(y_reg)
    return out
