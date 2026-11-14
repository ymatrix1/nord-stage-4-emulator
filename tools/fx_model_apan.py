#!/usr/bin/env python3
"""Reference model of the FX auto-pan routine `0x122D93` (module `0xC02BAF54`, `Fx1` types 0, 1).

Transliteration (see `tools/fx_pe.py`): an integer phase accumulator drives a triangle and an odd
polynomial (sine) LFO, which is smoothed and turned into a pair of channel gains
`g_L = P + v`, `g_R = P - v` (`P` a smoothed offset, `v` the clipped, scaled LFO); a loop of
`ctx[-3]` blocks of two stereo frames multiplies both channels by those gains.
`docs/research/fx-mod-types.md` s2 has the equations and the field table.
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
    fsub,
    lift,
    wd,
)

PC, END = 0x122D93, 0x122E1E
IN_ADDR, CTX, PHASE_PTR = 0xB1000, 0xB2040, 0xB6000
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}

isub = lift(lambda a, b: (wd(a) - wd(b)) & M32)
iadd = lift(lambda a, b: (wd(a) + wd(b)) & M32)
imul_uui = lift(lambda a, b: (wd(a) * wd(b)) & M32)  # `Rn = Rx * Ry (UUI)`: low word, unsigned


def apan_0x122d93(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the routine on `mem` (updated in place)."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step

    def b(off):  # broadcast load through I1
        return ldb(a("i1", off))

    step(0x122D93, d={"i3": m.ir["i1"]})
    step(0x122D95, w={4: b(-0x1B)})
    step(0x122D97, w={5: b(-7)})
    step(0x122D99, d={"i2": wd(m.x[4])})
    step(0x122D9B, w={15: b(-8)})
    step(0x122D9D, w={1: b(-0x18)})
    step(0x122D9F, w={2: b(-0x11)})
    step(0x122DA1, w={6: b(-0x17)})
    step(0x122DA3, w={2: fsub(r(2), r(1)), 7: b(-0xD)})
    step(0x122DA6, w={0: ld(a("i2"))})
    step(0x122DA8, w={2: fmul(r(2), r(5)), 3: b(-0xF)})
    step(0x122DAB, w={2: fadd(r(2), r(1)), 1: b(-0x16)})
    step(0x122DAE, w={7: fsub(r(7), r(6))})
    step(0x122DAF, w={3: fsub(r(3), r(1))}, st=[(a("i3", -0x15), r(2))])
    step(0x122DB2, w={7: fmul(r(7), r(5))})
    step(0x122DB3, w={3: fmul(r(3), r(5))}, st=[(a("i3", -0x11), r(2))])
    step(0x122DB6, w={7: fadd(r(7), r(6))})
    step(0x122DB7, w={3: fadd(r(3), r(1)), 1: b(-0x20)})
    step(0x122DBA, st=[(a("i3", -0xD), r(7))])
    step(0x122DBC, w={0: iadd(r(0), r(1))}, st=[(a("i3", -0x13), r(3))])
    step(0x122DBF, st=[(a("i3", -0x1F), r(0))])
    step(0x122DC1, w={1: b(-0x1A)})
    step(0x122DC3, w={0: imul_uui(r(0), r(1))})
    step(0x122DC5, st=[(a("i3", -0xF), r(3))])
    step(0x122DC7, w={1: b(-0x19)})
    step(0x122DC9, w={0: isub(r(0), r(1))})
    step(0x122DCA, w={0: ffloat(r(0), r(15))})
    step(0x122DCC, w={0: fabs(r(0))})
    step(0x122DCE, w={0: fadd(r(0), r(0)), 1: b(-5)})
    step(0x122DD1, w={0: fadd(r(0), r(1))})
    step(0x122DD2, w={1: fmul(r(0), r(0)), 8: b(-0xB)})
    step(0x122DD5, w={2: fmul(r(0), r(1)), 9: b(-0xA)})
    step(0x122DD8, w={3: fmul(r(1), r(2)), 10: b(-9)})
    step(0x122DDB, w={4: fmul(r(0), r(8))})
    step(0x122DDD, w={5: fmul(r(2), r(9))})
    step(0x122DDF, w={6: fmul(r(3), r(10))})
    step(0x122DE1, w={0: fadd(r(4), r(5))})
    step(0x122DE3, w={0: fadd(r(0), r(6))})
    step(0x122DE4, w={7: r(0)})
    step(0x122DE6, wx={7: m.y[0]}, wy={0: m.x[7]})
    step(0x122DE8, w={5: b(-7)})
    step(0x122DEA, w={2: ld(a("i3", -0x1D))})
    step(0x122DEC, w={2: fsub(r(2), r(0))})
    step(0x122DED, w={2: fmul(r(2), r(5))})
    step(0x122DEE, w={0: fadd(r(0), r(2))})
    step(0x122DEF, st=[(a("i3", -0x1D), r(0))])
    step(0x122DF1, w={1: b(-0x15)})
    step(0x122DF3, w={3: b(-0x1D)})
    step(0x122DF5, w={1: fmul(r(1), r(3)), 3: b(-6)})
    step(0x122DF8, w={1: fmul(r(1), r(3)), 13: b(-0xD)})
    step(0x122DFB, w={1: fclip(r(1), r(13)), 3: b(-4)})
    step(0x122DFE, w={1: fmul(r(1), r(3)), 3: b(-0x13)})
    step(0x122E01, w={5: fsub(r(3), r(1)), 0: b(-3)})
    step(0x122E04, w={4: fadd(r(1), r(3)), 7: r(5)})
    step(0x122E07, wx={7: m.y[4]}, wy={4: m.x[7]})
    step(0x122E09)
    for _ in range(wd(m.x[0])):
        step(0x122E0C, w={0: ld(a("i0", 0))})
        step(0x122E0F, w={8: ld(a("i0", 2))})
        step(0x122E12, w={0: fmul(r(0), r(4))})
        step(0x122E15, w={8: fmul(r(8), r(4))})
        step(0x122E18, st=[(a("i0"), r(0))], d={"i0": m.ir["i0"] + 2})
        step(0x122E1B, st=[(a("i0"), r(8))], d={"i0": m.ir["i0"] + 2})
    return m


def random_scenario(rng) -> dict[int, int]:
    """Context, phase word and 1 to 4 loop iterations of frames."""
    count = rng.randrange(1, 5)
    mem = {a: fb.bits(rng.uniform(-1.0, 1.0)) for a in range(CTX - 0x20, CTX + 0x7)}
    for a in range(IN_ADDR, IN_ADDR + 4 * count):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    mem.update({
        CTX - 0x1B: PHASE_PTR,
        CTX - 0x1F: rng.randrange(1 << 32),
        CTX - 0x20: rng.randrange(1 << 24),
        CTX - 0x1A: rng.randrange(1 << 12), CTX - 0x19: rng.randrange(1 << 32),
        CTX - 8: rng.choice([0xFFFFFFE1, 0xFFFFFFE0, 0xFFFFFFE2]),
        CTX - 3: count,
        PHASE_PTR: rng.randrange(1 << 32), PHASE_PTR + 1: rng.randrange(1 << 32),
        CTX - 7: fb.bits(rng.choice([0.9908, 0.5, 0.0, 0.99])),
    })  # fmt: skip
    return mem


def routine():
    """The harness description of this routine (`tools/fx_oracle.py`)."""
    import fx_oracle as fo

    return fo.Routine(
        "apan",
        PC,
        END,
        SETS,
        lambda mem, trace=None: apan_0x122d93(mem, IN_ADDR, CTX, trace),
        random_scenario,
    )
