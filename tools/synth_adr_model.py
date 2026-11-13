#!/usr/bin/env python3
"""Models of the Core2 envelope record chain and of the strip-word-3 cutoff law.

Both are written from the ORG core 2 listing and checked against the SHARC oracle
(`tools/synth_adr_oracle.py`); see `docs/research/synth-adr-and-strip-law.md`.

**Record chain** (`pc 0x1274EB..0x127AFF`, per control pass, lane PEx of slot 0 shown). A record is
four words `[a, b, thr, link]`; the frame word `DM(F - 18)` holds the address of the current
record, `DM(F - 20)` the state `y` (the envelope level, float):

    y' = y a + b                       (two float32 operations, each rounded)
    if y' < thr:  y' = thr;  if thr < 0: y' = 0
    if y' > L:    y' = L;    pointer = link        (L = DM(F - 22) = 1.0)

The nine ADR coefficients of a channel (`0xC0DC9, DCC, DD0, DDD, DE0, DE4, DE9, DEC, DF0` for the
first one) are the `b` of the attack record (a = 1) and the `a` of the decay and release records
(b = 0, decay `thr` = 0.5 = the sustain level, release `thr` = 0), one set per envelope.

**Cutoff law** (pass control block `0x125F80..0x125FEE`, PEy lane of channel 0): the strip word 3
(after the pass smoother) is clipped to +-0x6A000000, its top byte is the table index `i` (signed,
+-106) and the low 24 bits interpolate linearly between table entries `i` and `i + 1`:

    R(i) = clamp(0.36083 + 0.0079301 i, 0, 1.0111)      table DM(0xB0BA2 + i)

`R` is the `tan(pi fc / fs)` coefficient of the filter stages (`tools/synth_filter_model.py`).
"""

from __future__ import annotations

import math
import struct
from dataclasses import dataclass

import sharc40 as s


def f2u(x: float) -> int:
    """float32 bits of a Python float."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def u2f(u: int) -> float:
    """float of float32 bits."""
    return struct.unpack("<f", struct.pack("<I", u & 0xFFFFFFFF))[0]


@dataclass(frozen=True)
class Record:
    """One chain record `[a, b, thr, link]` as float32 words (`link` is the next record address)."""

    a: int
    b: int
    thr: int
    link: int

    @staticmethod
    def of(a: float, b: float, thr: float, link: int) -> Record:
        """Record from Python floats (rounded to float32 words)."""
        return Record(f2u(a), f2u(b), f2u(thr), link)


ONE = 0x3F800000  # DM(F - 22): the level limit L = 1.0


def adr_step(y: int, rec: Record, limit: int = ONE) -> tuple[int, str]:
    """One control pass of the chain on the level word `y`; returns `(y', action)`.

    The arithmetic is the 40-bit register model of `tools/sharc40.py` (32-bit significand per
    operation, the store truncates to float32): `y a` and `+ b` are separate roundings.

    `action` is `"link"` (the level passed `limit`: the pointer moves to `rec.link`), `"restart"`
    (a negative `thr` fired: the fast fade of a retriggered note is done and the module code puts
    the pointer back on the attack record, EMPIRICAL, oracle scenario D) or `""`.
    """
    out = s.fadd(s.fmul(s.ld(y), s.ld(rec.a)), s.ld(rec.b))
    action = ""
    thr = s.ld(rec.thr)
    if out < thr:
        out = thr
        if thr < 0:  # the second predicated op `f12 = f4 - f4` fires on the AN flag of `thr`
            out = s.ZERO
            action = "restart"
    if out > s.ld(limit):
        out = s.ld(limit)
        action = "link"
    return s.st(out), action


def run_chain(
    records: dict[int, Record],
    ptr: int,
    y: int,
    passes: int,
    start: dict[int, int] | None = None,
    limit: int = ONE,
    attack: int | None = None,
) -> list[tuple[int, int]]:
    """`(y word, pointer)` after each of `passes` control passes.

    `start[p]` is a record address the pointer is set to before pass `p` (what a change of the
    controller trigger field does: sign 0 selects the release start `DM(F - 30)`, sign > 0
    `DM(F - 28)`, sign < 0 `DM(F - 26)`, EMPIRICAL, `docs/research/synth-adr-and-strip-law.md`).
    """
    out = []
    for p in range(passes):
        if start and p in start:
            ptr = start[p]
        y, action = adr_step(y, records[ptr], limit)
        if action == "link":
            ptr = records[ptr].link
        elif action == "restart" and attack is not None:
            ptr = attack
        out.append((y, ptr))
    return out


def rate_index(word: int, mod: int = 0) -> int:
    """Signed rate-table index of an ADR strip word: top byte of `clip(word + mod, +-0x40000000)`.

    VERIFIED (listing `0x120494..0x1204A8`, `0x126F1D..0x126F61`; table sweep
    `docs/research/data/synth-env-rate-table.csv`). `mod` is the modulation word the installer adds
    (`DM(i0 - 18 ..)`, zero when no controller moves it).
    """
    w = word & 0xFFFFFFFF
    w = w - (1 << 32) if w & 0x80000000 else w
    m = mod & 0xFFFFFFFF
    m = m - (1 << 32) if m & 0x80000000 else m
    s = max(-0x40000000, min(0x40000000, w + m))
    return s >> 24


# EMPIRICAL: closed form of the table DM(0xB0BA2 + i), i = -128..127 (dumped from the ORG core 2
# image): max abs error 1.02e-6 against the 152 float32 entries from i = -50 to 105; `check_r_table`
# in the test reproduces it from the oracle sweep.
R_BASE = 0.36083
R_STEP = 0.0079301
R_MAX = 1.0111
CLIP_WORD = 0x6A000000  # DM(0xC1479): clip of the PEy lane sum, +-106 table steps


def r_table(i: int) -> float:
    """`R` at integer table index `i`."""
    return min(max(R_BASE + R_STEP * i, 0.0), R_MAX)


def cutoff_r(word3: int) -> float:
    """`R` for the strip word 3 value `word3` as the pass sees it (Q31, signed or unsigned)."""
    w = word3 & 0xFFFFFFFF
    w = w - (1 << 32) if w & 0x80000000 else w
    w = max(-CLIP_WORD, min(CLIP_WORD, w))
    i, frac = w >> 24, (w & 0xFFFFFF) / float(1 << 24)
    lo = r_table(i)
    return lo + (r_table(i + 1) - lo) * frac


def cutoff_fc_over_fs(r: float) -> float:
    """Cutoff in cycles per sample for the coefficient `r` (bilinear warp, section 3.2 of
    `synth-stage-equations`): `R = tan(pi fc / fs)`."""
    return math.atan(r) / math.pi


def osc_ratio(i: float) -> float:
    """Oscillator cycles per sample at the strip word 2 index `i` (24 steps per octave).

    EMPIRICAL: table DM(0xB069B + i) = 0.0060279 2^(i / 24), max relative error 4.2e-6 over
    i = -60..99 (same top-byte index and 24-bit interpolation as `cutoff_r`).
    """
    return 0.00602790993 * 2.0 ** (i / 24.0)
