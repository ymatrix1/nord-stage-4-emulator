#!/usr/bin/env python3
"""Reference model of the FX table LFO + modulated filter routine `0x1228CF`.

Transliteration (see `tools/fx_pe.py`) of the routine that the LFO class of `fx-type-map.md`
installs: a smoothed phase is turned into an index into two interpolated tables (`-cos` quarter
wave and its companion), the result and a block loop of two biquad-like sections whose
coefficients are interpolated from a third table by the filtered signal. `docs/research/
fx-routine-models.md` s3 has the equations and the field table.
"""

from __future__ import annotations

import math

import fx_biquad_check as fb
from fx_pe import (
    M32,
    Machine,
    fadd,
    fclip,
    fdep_p,
    fext_p,
    ffix,
    ffloat,
    fmul,
    fpass,
    fsub,
    imm,
    wd,
)

PC, END = 0x1228CF, 0x122A1C
IN_ADDR, CTX, TAB_A, TAB_B, TAB_C = 0xB1000, 0xB2040, 0xB6000, 0xB6200, 0xB6800
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}
K = 0x3F7DA3C2  # 0.9908, the smoothing coefficient of the ORG / delay / LFO routines


def lfo_0x1228cf(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the routine on `mem` (updated in place)."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    mm = lambda n: m.ir[n]  # noqa: E731
    s = lambda hi, lo: dict(wx={hi: m.y[lo]}, wy={lo: m.x[hi]})  # noqa: E731  Rhi <-> Slo
    # --- phase smoothing, table index and fraction ------------------------------------------
    step(0x1228CF, d={"i3": mm("i1")})
    step(0x1228D1, w={8: imm(K)})
    step(0x1228D4, w={1: ldb(a("i1", -0x20))})
    step(0x1228D6, w={2: ldb(a("i1", -0x1F))})
    step(0x1228D8, w={2: fsub(r(2), r(1)), 14: ldb(a("i1", -2))})
    step(0x1228DB, d={"i6": mem[a("i3", -0x15)]})
    step(0x1228DD, w={2: fmul(r(2), r(8)), 15: ldb(a("i1", -1))})
    step(0x1228E0, d={"i2": mem[a("i3", -0x14)]})
    step(0x1228E2, w={1: fadd(r(1), r(2))})
    step(0x1228E3, w={2: ffix(r(1), r(14))}, st=[(a("i3", -0x1F), r(1))])
    step(0x1228E6, w={3: fext_p(r(2), [24, 24], [8, 8], [False, False])})
    step(0x1228E9, d={"m4": wd(m.x[3])})
    step(0x1228EB, d={"i7": mm("i0")})
    step(0x1228ED)
    step(0x1228EE, w={4: fdep_p(r(2), [7, 7], [24, 24])})
    step(0x1228F1, w={4: ffloat(r(4), r(15)), 12: imm(mm("m0"))})
    step(0x1228F4, w={11: ld((mm("i6") + mm("m4")) & M32)})
    step(0x1228F6, **s(12, 11))
    step(0x1228F8, w={6: fmul(r(4), r(12))})
    step(0x1228FA, w={5: fmul(r(4), r(11))})
    step(0x1228FC, w={8: fadd(r(11), r(6)), 12: imm(mm("m0"))})
    step(0x1228FF, w={8: fsub(r(8), r(5)), 11: ld((mm("i2") + mm("m4")) & M32)})
    step(0x122902, **s(12, 11))
    step(0x122904, w={6: fmul(r(4), r(12)), 7: r(8)})
    step(0x122907, w={5: fmul(r(4), r(11)), 2: ldb(a("i1", -0xD))})
    step(0x12290A, w={9: fadd(r(11), r(6)), 3: ldb(a("i1", -0xC))})
    step(0x12290D, w={9: fsub(r(9), r(5))}, **s(7, 8))
    step(0x122910, w={7: fpass(r(9))})
    step(0x122912, **s(7, 9))
    step(0x122914, w={8: fmul(r(8), r(2)), 10: ldb(a("i1", -0x1F))})
    step(0x122917, w={1: fmul(r(10), r(3)), 2: ldb(a("i1", -0xB))})
    step(0x12291A, w={1: fadd(r(1), r(2))}, st=[(a("i3", -0xA), r(8))])
    step(0x12291D, w={4: fmul(r(9), r(1))})
    step(0x12291F, w={0: ldb(a("i1", 0x15))})
    step(0x122921, st=[(a("i3", -8), r(4))])
    step(0x122923, w={2: imm(0x41A00000)})
    step(0x122926, w={3: ldb(a("i1", 0))})
    step(0x122928, w={1: fmul(r(10), r(2))})
    step(0x12292A, w={4: fclip(r(1), r(3))})
    step(0x12292C, w={5: fsub(r(3), r(4))})
    step(0x12292E, st=[(a("i3", -0x1D), r(4))])
    step(0x122930, st=[(a("i3", -0x1B), r(5))])
    count = wd(m.x[0])
    step(0x122932)
    for _ in range(count):
        _body(m)
    return m


def _body(m: Machine) -> None:
    """One iteration of the block loop (`0x122935 .. 0x122A19`): 2 stereo frames."""
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    mm = lambda n: m.ir[n]  # noqa: E731
    s = lambda hi, lo: dict(wx={hi: m.y[lo]}, wy={lo: m.x[hi]})  # noqa: E731
    step(0x122935, w={0: ld(a("i0", 0))})
    step(0x122937, w={1: ld(a("i0", 2))})
    step(0x122939, w={4: ldb(a("i1", 7))})
    step(0x12293B, w={8: fmul(r(0), r(4)), 5: ldb(a("i1", 8))})
    step(0x12293E, w={9: fmul(r(0), r(5)), 12: ld(a("i3", 3))})
    step(0x122941, w={6: ldb(a("i1", 9))})
    step(0x122943, w={10: fmul(r(0), r(6)), 0: fadd(r(8), r(12)), 13: ld(a("i3", 5))})
    step(0x122946, w={9: fadd(r(9), r(13))})
    step(0x122947, w={2: ldb(a("i1", 0xA))})
    step(0x122949, w={12: fmul(r(2), r(0)), 3: ldb(a("i1", 0xB))})
    step(0x12294C, w={13: fmul(r(3), r(0))}, st=[(a("i3", -0x19), r(0))])
    step(0x12294F, w={8: fmul(r(1), r(4)), 12: fsub(r(9), r(12))})
    step(0x122951, w={9: fmul(r(1), r(5)), 13: fsub(r(10), r(13))})
    step(0x122953, w={10: fmul(r(1), r(6)), 1: fadd(r(8), r(12))})
    step(0x122955, w={9: fadd(r(9), r(13))})
    step(0x122956, w={12: fmul(r(2), r(1))})
    step(0x122958, w={13: fmul(r(3), r(1))}, st=[(a("i3", -0x17), r(1))])
    step(0x12295B, w={12: fsub(r(9), r(12))})
    step(0x12295D, w={13: fsub(r(10), r(13))}, st=[(a("i3", 3), r(12))])
    step(0x122960, st=[(a("i3", 5), r(13))])
    step(0x122962, w={2: ldb(a("i1", -0xA))})
    step(0x122964, w={0: fmul(r(0), r(2)), 5: ldb(a("i1", -0x13))})
    step(0x122967, w={1: fmul(r(1), r(2)), 2: ld(a("i3", -0x11))})
    step(0x12296A, w={12: fmul(r(2), r(5)), 4: ldb(a("i1", -0x12))})
    step(0x12296D, w={10: fmul(r(0), r(5)), 2: ld(a("i3", -0xF))})
    step(0x122970, w={12: fmul(r(2), r(4)), 8: fsub(r(10), r(12)), 13: r(10)})
    step(0x122973, w={9: fmul(r(1), r(5)), 0: fsub(r(8), r(12))})
    step(0x122975, w={13: fmul(r(0), r(4)), 9: fsub(r(9), r(13))})
    step(0x122977, st=[(a("i3", -0x11), r(1))])
    step(0x122979, w={1: fsub(r(9), r(13))})
    step(0x12297B, st=[(a("i3", -0xF), r(1))])
    step(0x12297D, w={10: ldb(a("i1", 1))})
    step(0x12297F, w={0: fclip(r(0), r(10))})
    step(0x122981, w={1: fclip(r(1), r(10))})
    step(0x122983, w={2: ffix(r(0), r(14)), 10: ldb(a("i1", 2))})
    step(0x122986, w={3: ffix(r(1), r(14))})
    step(0x122988, w={4: fext_p(r(2), [22, 22], [10, 10], [True, True])})
    step(0x12298B, w={5: fext_p(r(3), [22, 22], [10, 10], [True, True])})
    step(0x12298E, d={"i2": wd(m.x[10])})
    step(0x122990, d={"m4": wd(m.x[4])})
    step(0x122992, d={"m3": wd(m.x[5])})
    step(0x122994, d={"m6": wd(m.y[4])})
    step(0x122996, d={"m5": wd(m.y[5])})
    step(0x122998, w={10: fdep_p(r(2), [9, 9], [22, 22])})
    step(0x12299B, w={11: fdep_p(r(3), [9, 9], [22, 22])})
    step(0x12299E, w={0: ffloat(r(10), r(15))})
    step(0x1229A0, w={1: ffloat(r(11), r(15))})
    step(0x1229A2, w={2: ld((mm("i2") + mm("m4")) & M32)})
    step(0x1229A4, w={3: ld((mm("i2") + mm("m3")) & M32)})
    step(0x1229A6, w={4: ld((mm("i2") + mm("m6")) & M32)})
    step(0x1229A8, w={5: ld((mm("i2") + mm("m5")) & M32)})
    step(0x1229AA, **s(4, 2))
    step(0x1229AC, **s(5, 3))
    step(0x1229AE, w={6: fmul(r(0), r(2))})
    step(0x1229B0, w={7: fmul(r(1), r(3))})
    step(0x1229B2, w={4: fmul(r(0), r(4))})
    step(0x1229B4, w={5: fmul(r(1), r(5))})
    step(0x1229B6, w={0: fsub(r(2), r(6))})
    step(0x1229B8, w={1: fsub(r(3), r(7))})
    step(0x1229BA, w={0: fadd(r(0), r(4))})
    step(0x1229BB, w={1: fadd(r(1), r(5))})
    step(0x1229BC, w={5: ldb(a("i1", -8))})
    step(0x1229BE, w={0: fmul(r(0), r(5)), 3: ldb(a("i1", -0x1D))})
    step(0x1229C1, w={1: fmul(r(1), r(5)), 4: ldb(a("i1", -0x1B))})
    step(0x1229C4, w={0: fmul(r(0), r(3)), 8: ld(a("i3", -0x19))})
    step(0x1229C7, w={1: fmul(r(1), r(3)), 9: ld(a("i3", -0x17))})
    step(0x1229CA, w={8: fmul(r(8), r(4))})
    step(0x1229CB, w={9: fmul(r(9), r(4))})
    step(0x1229CC, w={0: fadd(r(0), r(8))})
    step(0x1229CD, w={1: fadd(r(1), r(9))})
    _section(m, 0x1229CE, 0x10, 0xC, 0xE, 0x13, 0x14)


def _section(m: Machine, pc: int, c0: int, s0: int, s1: int, c3: int, c4: int) -> None:
    """The recursive section `0x1229CE ..` (coefficients `ctx + c0 ..`, states `ctx + s0, s1`)."""
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(pc, w={4: ldb(a("i1", c0))})
    step(pc + 2, w={8: fmul(r(0), r(4)), 5: ldb(a("i1", c0 + 1))})
    step(pc + 5, w={9: fmul(r(0), r(5)), 12: ld(a("i3", s0))})
    step(pc + 8, w={6: ldb(a("i1", c0 + 2))})
    step(pc + 0xA, w={10: fmul(r(0), r(6)), 0: fadd(r(8), r(12)), 13: ld(a("i3", s1))})
    step(pc + 0xD, w={9: fadd(r(9), r(13))})
    step(pc + 0xE, w={2: ldb(a("i1", c3))})
    step(pc + 0x10, w={12: fmul(r(2), r(0)), 3: ldb(a("i1", c4))})
    step(pc + 0x13, w={13: fmul(r(3), r(0))})
    step(pc + 0x15, w={8: fmul(r(1), r(4)), 12: fsub(r(9), r(12))})
    step(pc + 0x17, w={9: fmul(r(1), r(5)), 13: fsub(r(10), r(13))})
    step(pc + 0x19, w={10: fmul(r(1), r(6)), 1: fadd(r(8), r(12))})
    step(pc + 0x1B, w={9: fadd(r(9), r(13))})
    step(pc + 0x1C, w={12: fmul(r(2), r(1))})
    step(pc + 0x1E, w={13: fmul(r(3), r(1))})
    step(pc + 0x20, w={12: fsub(r(9), r(12))})
    step(pc + 0x22, w={13: fsub(r(10), r(13))}, st=[(a("i3", s0), r(12))])
    step(pc + 0x25, st=[(a("i3", s1), r(13))])
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x1229F5, w={5: ldb(a("i1", -0x13))})
    step(0x1229F7, w={2: ld(a("i3", -6))})
    step(0x1229F9, w={4: imm(0x40000000)})
    step(0x1229FC, w={0: fmul(r(0), r(4))})
    step(0x1229FD, w={1: fmul(r(1), r(4))})
    step(0x1229FE, w={12: fmul(r(2), r(5)), 4: ldb(a("i1", -0x12))})
    step(0x122A01, w={10: fmul(r(0), r(5)), 2: ld(a("i3", -4))})
    step(0x122A04, w={12: fmul(r(2), r(4)), 8: fsub(r(10), r(12))}, st=[(a("i3", -6), r(1))])
    step(0x122A07, w={9: fmul(r(1), r(5)), 0: fsub(r(8), r(12))})
    step(0x122A0A, w={12: r(10)})
    step(0x122A0D, w={13: fmul(r(0), r(4)), 9: fsub(r(9), r(12))})
    step(0x122A10, st=[(a("i0"), r(0))], d={"i0": m.ir["i0"] + 2})
    step(0x122A13, w={1: fsub(r(9), r(13))})
    step(0x122A16, st=[(a("i3", -4), r(1))])
    step(0x122A19, st=[(a("i0"), r(1))], d={"i0": m.ir["i0"] + 2})


def random_scenario(rng) -> dict[int, int]:
    """Context, tables and 1 to 4 loop iterations of frames."""

    def fl_(lo: float, hi: float) -> int:
        return fb.bits(math.exp(rng.uniform(math.log(lo), math.log(hi))))

    count = rng.randrange(1, 5)
    mem = {a: fl_(0.05, 1.0) for a in range(CTX - 0x20, CTX + 0x16)}
    for a in range(IN_ADDR, IN_ADDR + 4 * count):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    scale = rng.choice([31, 31, 30, 29])
    mem.update({
        CTX - 2: scale, CTX - 1: (-31) & M32, CTX + 0x15: count,
        CTX - 0x15: TAB_A, CTX - 0x14: TAB_B, CTX + 2: TAB_C,
        CTX - 0x20: fb.bits(rng.uniform(0.0, 0.9)), CTX - 0x1F: fb.bits(rng.uniform(0.0, 0.9)),
        CTX + 0: fb.bits(rng.choice([0.5, 2.0, 20.0])),
        CTX + 1: fb.bits(rng.choice([4.0, 100.0, 400.0])),
    })  # fmt: skip
    return mem


def tables(rng) -> dict[int, int]:
    """Two 257-entry tables (index `0 .. 255` plus one) and the signed-10-bit coefficient table."""
    t = {TAB_A + k: fb.bits(rng.uniform(-1, 1)) for k in range(260)}
    t.update({TAB_B + k: fb.bits(rng.uniform(-1, 1)) for k in range(260)})
    t.update({TAB_C - 520 + k: fb.bits(rng.uniform(-1, 1)) for k in range(1044)})
    return t


def routine():
    """The harness description of this routine (`tools/fx_oracle.py`)."""
    import random

    import fx_oracle as fo

    return fo.Routine(
        "lfo",
        PC,
        END,
        SETS,
        lambda mem, trace=None: lfo_0x1228cf(mem, IN_ADDR, CTX, trace),
        random_scenario,
        tables(random.Random(11)),
    )
