#!/usr/bin/env python3
"""Reference model of the FX LFO-modulated all-pass routine `0x122C39` (module `0xC02BB030`).

Transliteration (see `tools/fx_pe.py`): an integer phase accumulator drives a triangle and an odd
polynomial (sine) LFO, a smoothed coefficient is derived from it, then a block loop runs two
pairs of lattice all-pass sections per channel (a stereo phaser / vibe) and mixes wet and dry.
`docs/research/fx-routine-models.md` s4 has the equations and the field table.
"""

from __future__ import annotations

import fx_biquad_check as fb
from fx_pe import (
    M32,
    Machine,
    fabs,
    fadd,
    fclip,
    ffloat,
    fmul,
    fpass,
    fsub,
    imm,
    lift,
    wd,
)

PC, END = 0x122C39, 0x122D2B
IN_ADDR, CTX, PHASE_PTR = 0xB1000, 0xB2040, 0xB6000
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}

isub = lift(lambda a, b: (wd(a) - wd(b)) & M32)
iadd = lift(lambda a, b: (wd(a) + wd(b)) & M32)
imul_uui = lift(lambda a, b: (wd(a) * wd(b)) & M32)  # `Rn = Rx * Ry (UUI)`: low word, unsigned


def allpass_0x122c39(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the routine on `mem` (updated in place)."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x122C39, d={"i3": m.ir["i1"]})
    step(0x122C3B, d={"i2": mem[a("i1", -0x1D)]})
    step(0x122C3D, w={0: ldb(a("i1", -0x1F))})
    step(0x122C3F, w={1: ldb(a("i1", -0x20))})
    step(0x122C41, w={7: ldb(a("i1", -0x1C))})
    step(0x122C43, w={0: iadd(r(0), r(1))})
    step(0x122C44, w={15: imm(0xFFFFFFE1)})
    step(0x122C46, st=[(a("i3", -0x1F), r(0))])
    step(0x122C48, w={0: ld(a("i2"))})
    step(0x122C4A, w={0: imul_uui(r(0), r(7))})
    step(0x122C4C, w={1: ldb(a("i1", -0x1B))})
    step(0x122C4E, w={0: isub(r(0), r(1))})
    step(0x122C4F, w={0: ffloat(r(0), r(15))})
    step(0x122C51, w={0: fabs(r(0)), 14: ldb(a("i1", 3))})
    step(0x122C54, w={1: imm(0xBF800000)})
    step(0x122C57, w={0: fadd(r(0), r(0)), 11: ld(a("i3", -0x1A))})
    step(0x122C5A, w={0: fadd(r(0), r(1)), 12: ld(a("i3", -0x18))})
    step(0x122C5D, w={12: fsub(r(12), r(11))})
    step(0x122C5E, w={1: fmul(r(0), r(0)), 8: ldb(a("i1", -0x13))})
    step(0x122C61, w={12: fmul(r(12), r(14))})
    step(0x122C62, w={2: fmul(r(0), r(1)), 9: ldb(a("i1", -0x12))})
    step(0x122C65, w={12: fadd(r(12), r(11))})
    step(0x122C66, w={3: fmul(r(1), r(2)), 10: ldb(a("i1", -0x11))})
    step(0x122C69, w={4: fmul(r(0), r(8))}, st=[(a("i3", -0x18), r(12))])
    step(0x122C6C, w={5: fmul(r(2), r(9))})
    step(0x122C6E, w={6: fmul(r(3), r(10))})
    step(0x122C70, w={0: fadd(r(4), r(5)), 2: ld(a("i3", -0x15))})
    step(0x122C73, w={0: fadd(r(0), r(6)), 7: ldb(a("i1", -0x16))})
    step(0x122C76, wx={7: m.y[0]}, wy={0: m.x[7]})
    step(0x122C78, w={2: fsub(r(2), r(0))})
    step(0x122C79, w={2: fmul(r(2), r(14)), 6: ldb(a("i1", -0x18))})
    step(0x122C7C, w={0: fadd(r(0), r(2)), 5: ldb(a("i1", 4))})
    step(0x122C7F, st=[(a("i3", -0x15), r(0))])
    step(0x122C81, w={2: ldb(a("i1", -0x15))})
    step(0x122C83, w={6: fmul(r(2), r(6)), 4: ldb(a("i1", -0x14))})
    step(0x122C86, w={6: fclip(r(6), r(4)), 3: ldb(a("i1", -0x17))})
    step(0x122C89, w={14: fadd(r(6), r(3))})
    step(0x122C8B)
    for _ in range(wd(m.x[5])):
        _body(m)
    return m


def _stage(m: Machine, pcs, x: int, y: int, c: tuple, s: tuple) -> None:
    """One lattice all-pass section on input register `x`, working register `y` (8 or 9).

    Coefficients `c = (a, b, c, d, e)` are broadcast context words, states `s = (sa, sb)` pairs.
    """
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(pcs[0], w={y: ldb(a("i1", c[0]))})
    step(pcs[1], w={y: fmul(r(x), r(y)), 2: ldb(a("i1", c[1]))})
    step(pcs[2], w={2: fmul(r(x), r(2)), 3: ld(a("i3", s[0]))})
    step(pcs[3], w={y: fadd(r(y), r(3)), 3: ld(a("i3", s[1]))})
    step(pcs[4], w={10: fadd(r(2), r(3)), 3: ldb(a("i1", c[2]))})
    step(pcs[5], w={12: fmul(r(y), r(3)), 3: ldb(a("i1", c[3]))})
    step(pcs[6], w={3: fmul(r(y), r(3)), 4: ldb(a("i1", c[4]))})
    step(pcs[7], w={4: fmul(r(x), r(4)), 2: fadd(r(10), r(12))})
    step(pcs[8], w={3: fadd(r(3), r(4))}, st=[(a("i3", s[0]), r(2))])
    step(pcs[9], st=[(a("i3", s[1]), r(3))])


def _body(m: Machine) -> None:
    """One loop iteration (`0x122C8E .. 0x122D28`): 2 stereo frames, 2 + 2 all-pass sections."""
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x122C8E, w={0: ld(a("i0", 0))})
    step(0x122C90, w={1: ld(a("i0", 2))})
    near, far = (-7, -6, -4, -3, -5), (-0x10, -0xF, -0xD, -0xC, -0xE)
    _stage(
        m,
        (
            0x122C92,
            0x122C94,
            0x122C97,
            0x122C9A,
            0x122C9D,
            0x122CA0,
            0x122CA3,
            0x122CA6,
            0x122CA8,
            0x122CAB,
        ),
        0,
        8,
        near,
        (-2, 0),
    )  # noqa: E501  fmt: skip
    _stage(
        m,
        (
            0x122CAD,
            0x122CAF,
            0x122CB2,
            0x122CB5,
            0x122CB8,
            0x122CBB,
            0x122CBE,
            0x122CC1,
            0x122CC3,
            0x122CC6,
        ),
        1,
        9,
        near,
        (-2, 0),
    )  # noqa: E501  fmt: skip
    step(0x122CC8, w={7: fpass(r(8))})
    step(0x122CCA, w={15: fpass(r(9)), 13: ldb(a("i1", 2))})
    _stage(
        m,
        (
            0x122CCD,
            0x122CCF,
            0x122CD2,
            0x122CD5,
            0x122CD8,
            0x122CDB,
            0x122CDE,
            0x122CE1,
            0x122CE3,
            0x122CE6,
        ),
        0,
        8,
        far,
        (-0xB, -9),
    )  # noqa: E501  fmt: skip
    _stage(
        m,
        (
            0x122CE8,
            0x122CEA,
            0x122CED,
            0x122CF0,
            0x122CF3,
            0x122CF6,
            0x122CF9,
            0x122CFC,
            0x122CFE,
            0x122D01,
        ),
        1,
        9,
        far,
        (-0xB, -9),
    )  # noqa: E501  fmt: skip
    step(0x122D03, w={8: fmul(r(13), r(8))})
    step(0x122D05, w={9: fmul(r(13), r(9))})
    step(0x122D07, w={2: fmul(r(13), r(0))})
    step(0x122D09, w={10: fmul(r(13), r(1))})
    step(0x122D0B, w={0: fsub(r(0), r(2))})
    step(0x122D0C, w={1: fsub(r(1), r(10))})
    step(0x122D0D, w={0: fadd(r(0), r(8))})
    step(0x122D10, w={1: fadd(r(1), r(9))})
    step(0x122D13, w={7: fmul(r(7), r(13))})
    step(0x122D16, w={15: fmul(r(15), r(13))})
    step(0x122D19, w={0: fmul(r(0), r(14))})
    step(0x122D1C, w={1: fmul(r(1), r(14))})
    step(0x122D1F, w={0: fadd(r(0), r(7))})
    step(0x122D22, w={1: fadd(r(1), r(15))})
    step(0x122D25, st=[(a("i0"), r(0))], d={"i0": m.ir["i0"] + 2})
    step(0x122D28, st=[(a("i0"), r(1))], d={"i0": m.ir["i0"] + 2})


def random_scenario(rng) -> dict[int, int]:
    """Context, phase word and 1 to 4 loop iterations of frames."""
    count = rng.randrange(1, 5)
    mem = {a: fb.bits(rng.uniform(-1.0, 1.0)) for a in range(CTX - 0x20, CTX + 0x7)}
    for a in range(IN_ADDR, IN_ADDR + 4 * count):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    mem.update({
        CTX - 0x1D: PHASE_PTR,
        CTX - 0x1F: rng.randrange(1 << 32),
        CTX - 0x20: rng.randrange(1 << 24),
        CTX - 0x1C: rng.randrange(1 << 12), CTX - 0x1B: rng.randrange(1 << 32),
        CTX + 4: count,
        PHASE_PTR: rng.randrange(1 << 32), PHASE_PTR + 1: rng.randrange(1 << 32),
        CTX + 3: fb.bits(rng.choice([0.9908, 0.5, 0.0, 0.99])),
    })  # fmt: skip
    return mem


def routine():
    """The harness description of this routine (`tools/fx_oracle.py`)."""
    import fx_oracle as fo

    return fo.Routine(
        "allpass",
        PC,
        END,
        SETS,
        lambda mem, trace=None: allpass_0x122c39(mem, IN_ADDR, CTX, trace),
        random_scenario,
    )
