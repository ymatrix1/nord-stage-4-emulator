#!/usr/bin/env python3
"""Reference models of the FX nonlinear resonant filter pair `0x1243F4` / `0x124497`.

Transliterations (see `tools/fx_pe.py`): software-pipelined state-variable style sections with a
`CLIP` of the integrator in the feedback path and `(a + b) / 2` trapezoidal averaging.
`0x1243F4` runs two stereo frames, `0x124497` four. `docs/research/fx-routine-models.md` s5 has
the equations and the field tables.
"""

from __future__ import annotations

import fx_biquad_check as fb
from fx_pe import (
    Machine,
    fadd,
    favg,
    fclip,
    fmul,
    fneg,
    fsub,
)

PC1, END1 = 0x1243F4, 0x124496
IN_ADDR, CTX = 0xB1000, 0xB2040
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}


def resonant_0x1243f4(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the 2-frame routine on `mem` (updated in place)."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x1243F4, d={"i3": m.ir["i1"]})
    step(0x1243F6, w={1: ld(a("i3", -0x1C))})
    step(0x1243F8, w={2: ld(a("i3", -0x1A))})
    step(0x1243FA, w={2: fsub(r(2), r(1)), 6: ldb(a("i1", -0x11))})
    step(0x1243FD, w={9: ld(a("i3", -0x18))})
    step(0x1243FF, w={2: fmul(r(6), r(2)), 5: ld(a("i3", -0x16))})
    step(0x124402, w={15: fsub(r(5), r(9)), 14: ldb(a("i1", -0xE))})
    step(0x124405, w={0: fadd(r(1), r(2)), 7: ld(a("i3", -0x20))})
    step(0x124408, w={15: fmul(r(6), r(15)), 8: ld(a("i3", -0x1E))})
    step(0x12440B, w={3: fsub(r(8), r(7)), 1: ld(a("i0", 2))})
    step(0x12440E, w={2: fneg(r(0))}, st=[(a("i3", -0x1A), r(0))])
    step(0x124411, w={3: fmul(r(3), r(6)), 5: fadd(r(9), r(15)), 0: ld(a("i0", 0))})
    step(0x124414, w={10: favg(r(14), r(2)), 4: ldb(a("i1", -0x14))})
    step(0x124417, w={11: fadd(r(7), r(3))}, st=[(a("i3", -0x16), r(5))])
    step(0x12441A, w={12: fmul(r(0), r(4)), 8: ld(a("i3", -0xD))})
    step(0x12441D, w={4: fmul(r(8), r(4)), 5: r(4)})
    step(0x124420, w={2: fadd(r(8), r(12))}, st=[(a("i3", -0x1E), r(11))])
    step(0x124423, w={2: fsub(r(2), r(4)), 4: ldb(a("i1", -0x1E))})
    step(0x124426, w={13: fmul(r(1), r(5)), 9: r(2)})
    step(0x124429, w={12: fmul(r(2), r(4)), 14: ld(a("i3", -9))})
    step(0x12442C, w={5: fmul(r(9), r(5))}, st=[(a("i3", -0x13), r(10))])
    step(0x12442F, w={10: fadd(r(0), r(12)), 6: ldb(a("i1", -0x1D))})
    step(0x124432, w={2: ld(a("i3", -7))})
    step(0x124434, w={4: fmul(r(2), r(6)), 8: favg(r(10), r(14)), 12: ld(a("i3", -5))})
    step(0x124437, w={3: fadd(r(9), r(13))})
    step(0x124439, w={12: fadd(r(12), r(4)), 4: ldb(a("i1", -0x15))})
    step(0x12443C, w={3: fsub(r(3), r(5)), 5: ldb(a("i1", -0x1E))})
    step(0x12443F, w={4: fmul(r(2), r(4)), 8: fsub(r(8), r(12))}, st=[(a("i3", -9), r(0))])
    step(0x124442, w={13: fmul(r(3), r(5))}, st=[(a("i3", -0xD), r(3))])
    step(0x124445, w={8: fsub(r(8), r(4))}, st=[(a("i3", -5), r(12))])
    step(0x124448, w={7: ldb(a("i1", -0x1D))})
    step(0x12444A, w={12: fmul(r(8), r(6))})
    step(0x12444C, w={15: ld(a("i3", -9))})
    step(0x12444E, w={8: fadd(r(2), r(12)), 12: ldb(a("i1", -0xE))})
    step(0x124451, w={11: fadd(r(1), r(13))})
    step(0x124453, w={2: fclip(r(8), r(12)), 6: ldb(a("i1", -0x16))})
    step(0x124456, w={3: r(2)})
    step(0x124458, w={5: fmul(r(3), r(7)), 9: favg(r(11), r(15)), 13: ld(a("i3", -5))})
    step(0x12445B, w={12: fmul(r(2), r(6))})
    step(0x12445D, w={13: fadd(r(13), r(5)), 5: ldb(a("i1", -0x15))})
    step(0x124460, w={0: fadd(r(10), r(12)), 4: ldb(a("i1", -0x13))})
    step(0x124463, w={5: fmul(r(3), r(5)), 9: fsub(r(9), r(13))}, st=[(a("i3", -9), r(1))])
    step(0x124466, w={8: fmul(r(0), r(4)), 12: ld(a("i3", -0xB))})
    step(0x124469, w={9: fsub(r(9), r(5))}, st=[(a("i3", -5), r(13))])
    step(0x12446C, w={2: fsub(r(8), r(12)), 4: ldb(a("i1", -0x1A))})
    step(0x12446F, w={13: fmul(r(9), r(7))})
    step(0x124471, w={12: fmul(r(2), r(4)), 4: ldb(a("i1", -0x19))})
    step(0x124474, w={9: fadd(r(3), r(13)), 13: ldb(a("i1", -0xE))})
    step(0x124477, w={12: fmul(r(2), r(4)), 2: fadd(r(8), r(12))})
    step(0x124479, w={3: fclip(r(9), r(13)), 7: ldb(a("i1", -0x16))})
    step(0x12447C, w={0: fadd(r(0), r(12))}, st=[(a("i3", -0xB), r(2))])
    step(0x12447F, w={13: fmul(r(3), r(7))}, st=[(a("i3", -7), r(3))])
    step(0x124482, w={1: fadd(r(11), r(13))}, st=[(a("i0"), r(0))], d={"i0": m.ir["i0"] + 2})
    step(0x124485, w={5: ldb(a("i1", -0x13))})
    step(0x124487, w={9: fmul(r(1), r(5)), 13: ld(a("i3", -0xB))})
    step(0x12448A, w={3: fsub(r(9), r(13)), 5: ldb(a("i1", -0x1A))})
    step(0x12448D, w={13: fmul(r(3), r(5)), 5: ldb(a("i1", -0x19))})
    step(0x124490, w={13: fmul(r(3), r(5)), 3: fadd(r(9), r(13))})
    step(0x124492, w={1: fadd(r(1), r(13))}, st=[(a("i3", -0xB), r(3))])
    step(0x124495, st=[(a("i0"), r(1))], d={"i0": m.ir["i0"] + 2})
    return m


def random_scenario(rng) -> dict[int, int]:
    """Context floats (`CTX - 0x20 .. CTX - 4`) and the two frames."""
    mem = {a: fb.bits(rng.uniform(-1.0, 1.0)) for a in range(CTX - 0x20, CTX)}
    for a in range(IN_ADDR, IN_ADDR + 4):
        mem[a] = fb.bits(rng.uniform(-1.0, 1.0))
    return mem


def routine():
    """The harness description of the 2-frame routine (`tools/fx_oracle.py`)."""
    import fx_oracle as fo

    return fo.Routine(
        "resonant",
        PC1,
        END1,
        SETS,
        lambda mem, trace=None: resonant_0x1243f4(mem, IN_ADDR, CTX, trace),
        random_scenario,
    )
