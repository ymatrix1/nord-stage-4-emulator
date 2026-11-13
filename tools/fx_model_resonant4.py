#!/usr/bin/env python3
"""Reference model of the 4-frame FX resonant filter `0x124497` (partner of `0x1243F4`).

Transliteration (see `tools/fx_pe.py`) of the fully unrolled, software-pipelined routine that
ends with a delayed `RTS`; the two delay-slot instructions are part of the model. The harness
returns to its stop pc through a pushed return address (`sharc-fx --until`).
`docs/research/fx-routine-models.md` s5 has the equations and the field tables.
"""

from __future__ import annotations

import fx_biquad_check as fb
from fx_pe import Machine, fadd, favg, fclip, fmul, fneg, fsub

PC, END = 0x124497, 0x124585
IN_ADDR, CTX = 0xB1000, 0xB2040
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}


def resonant_0x124497(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the 4-frame routine on `mem` (updated in place; `I0` is not advanced)."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x124497, d={"i3": m.ir["i1"]})
    step(0x124499, w={0: ldb(a("i1", -0x13))})
    step(0x12449B, w={9: ld(a("i0", 0))})
    step(0x12449D, w={10: fmul(r(9), r(0))})
    step(0x12449F, w={1: ld(a("i3", -0xB))})
    step(0x1244A1, w={1: fsub(r(10), r(1)), 0: ldb(a("i1", -0x19))})
    step(0x1244A4, w={7: ldb(a("i1", -0x1D))})
    step(0x1244A6, w={2: fmul(r(1), r(0)), 0: ld(a("i3", -7))})
    step(0x1244A9, w={8: fmul(r(0), r(7)), 5: ldb(a("i1", -0x1A))})
    step(0x1244AC, w={13: fadd(r(9), r(2)), 12: ld(a("i3", -5))})
    step(0x1244AF, w={1: fmul(r(1), r(5)), 12: fadd(r(8), r(12)), 9: ld(a("i3", -9))})
    step(0x1244B2, w={8: favg(r(13), r(9))}, st=[(a("i3", -5), r(12))])
    step(0x1244B5, w={1: fadd(r(10), r(1)), 5: ldb(a("i1", -0x15))})
    step(0x1244B8, w={5: fmul(r(0), r(5)), 8: fsub(r(8), r(12))})
    step(0x1244BA, w={8: fsub(r(8), r(5))})
    step(0x1244BB, w={6: ldb(a("i1", -0x13))})
    step(0x1244BD, w={8: fmul(r(8), r(7)), 9: ld(a("i0", 2))})
    step(0x1244C0, w={10: fmul(r(9), r(6))})
    step(0x1244C2, w={0: fadd(r(0), r(8))})
    step(0x1244C3, w={1: fsub(r(10), r(1)), 4: ldb(a("i1", -0xE))})
    step(0x1244C6, w={0: fclip(r(0), r(4)), 4: ldb(a("i1", -0x19))})
    step(0x1244C9, w={12: fmul(r(1), r(4)), 4: ldb(a("i1", -0x16))})
    step(0x1244CC, w={8: fmul(r(0), r(7)), 5: ldb(a("i1", -0x1A))})
    step(0x1244CF, w={4: fmul(r(0), r(4)), 11: fadd(r(9), r(12)), 12: ld(a("i3", -5))})
    step(0x1244D2, w={1: fmul(r(1), r(5)), 12: fadd(r(8), r(12))})
    step(0x1244D4, w={8: favg(r(11), r(13))}, st=[(a("i3", -5), r(12))])
    step(0x1244D7, w={1: fadd(r(10), r(1)), 5: ldb(a("i1", -0x15))})
    step(0x1244DA, w={5: fmul(r(0), r(5)), 8: fsub(r(8), r(12)), 2: ldb(a("i1", -0x19))})
    step(0x1244DD, w={14: fadd(r(13), r(4))})
    step(0x1244DF, w={8: fsub(r(8), r(5)), 5: ldb(a("i1", -0x14))})
    step(0x1244E2, w={13: fmul(r(14), r(5)), 9: ld(a("i0", 4))})
    step(0x1244E5, w={12: fmul(r(8), r(7)), 8: r(0)})
    step(0x1244E8, w={10: fmul(r(9), r(6)), 0: ld(a("i3", -0xD))})
    step(0x1244EB, w={2: fmul(r(0), r(5)), 0: fadd(r(8), r(12)), 4: ldb(a("i1", -0xE))})
    step(0x1244EE, w={1: fsub(r(10), r(1)), 5: ldb(a("i1", -0x19))})
    step(0x1244F1, w={0: fclip(r(0), r(4)), 4: ldb(a("i1", -0x16))})
    step(0x1244F4, w={12: fmul(r(1), r(5)), 8: ld(a("i3", -0xD))})
    step(0x1244F7, w={8: fmul(r(0), r(7)), 6: fadd(r(8), r(13)), 5: ldb(a("i1", -0x1A))})
    step(0x1244FA, w={15: fmul(r(0), r(4)), 13: fadd(r(9), r(12)), 12: ld(a("i3", -5))})
    step(0x1244FD, w={1: fmul(r(1), r(5)), 12: fadd(r(8), r(12))})
    step(0x1244FF, w={8: favg(r(13), r(11))}, st=[(a("i3", -5), r(12))])
    step(0x124502, w={1: fadd(r(10), r(1)), 5: ldb(a("i1", -0x15))})
    step(0x124505, w={5: fmul(r(0), r(5)), 12: fsub(r(8), r(12)), 8: r(0)})
    step(0x124508, w={2: fsub(r(6), r(2)), 4: ldb(a("i1", -0x1E))})
    step(0x12450B, w={12: fsub(r(12), r(5)), 0: ldb(a("i1", -0x13))})
    step(0x12450E, w={12: fmul(r(12), r(7)), 9: ld(a("i0", 6))})
    step(0x124511, w={10: fmul(r(9), r(0)), 6: ldb(a("i1", -0xE))})
    step(0x124514, w={8: fmul(r(2), r(4)), 0: fadd(r(8), r(12)), 5: ldb(a("i1", -0x19))})
    step(0x124517, w={1: fsub(r(10), r(1)), 4: ldb(a("i1", -0x16))})
    step(0x12451A, w={0: fclip(r(0), r(6))})
    step(0x12451C, w={12: fmul(r(1), r(5)), 14: fadd(r(8), r(14)), 5: ldb(a("i1", -0x1A))})
    step(0x12451F, w={8: fmul(r(0), r(7))}, st=[(a("i0", 0), r(14))])
    step(0x124522, w={14: fmul(r(0), r(4)), 9: fadd(r(9), r(12)), 12: ld(a("i3", -5))})
    step(0x124525, w={1: fmul(r(1), r(5)), 12: fadd(r(8), r(12))}, st=[(a("i3", -9), r(9))])
    step(0x124528, w={8: favg(r(9), r(13))}, st=[(a("i3", -5), r(12))])
    step(0x12452B, w={1: fadd(r(10), r(1)), 5: ldb(a("i1", -0x15))})
    step(0x12452E, w={5: fmul(r(0), r(5)), 1: fsub(r(8), r(12))}, st=[(a("i3", -0xB), r(1))])
    step(0x124531, w={6: fadd(r(11), r(15)), 15: ldb(a("i1", -0x11))})
    step(0x124534, w={1: fsub(r(1), r(5)), 5: ldb(a("i1", -0x14))})
    step(0x124537, w={12: fmul(r(6), r(5)), 11: ld(a("i3", -0x1C))})
    step(0x12453A, w={1: fmul(r(1), r(7)), 8: r(2)})
    step(0x12453D, w={12: fmul(r(2), r(5)), 8: fadd(r(8), r(12)), 10: ld(a("i3", -0x1A))})
    step(0x124540, w={0: fadd(r(0), r(1)), 3: ld(a("i3", -0x18))})
    step(0x124543, w={10: fsub(r(10), r(11)), 7: ld(a("i3", -0x16))})
    step(0x124546, w={7: fsub(r(7), r(3))})
    step(0x124547, w={2: fadd(r(13), r(14)), 1: ldb(a("i1", -0xE))})
    step(0x12454A, w={10: fmul(r(15), r(10))})
    step(0x12454C, w={7: fmul(r(15), r(7))})
    step(0x12454E, w={0: fclip(r(0), r(1)), 4: ldb(a("i1", -0x1E))})
    step(0x124551, w={7: fadd(r(3), r(7))})
    step(0x124553, w={12: fmul(r(2), r(5)), 1: fsub(r(8), r(12)), 13: ldb(a("i1", -0x16))})
    step(0x124556, w={7: fadd(r(11), r(10))}, st=[(a("i3", -0x16), r(7))])
    step(0x124559, w={13: fmul(r(0), r(13)), 8: r(1)})
    step(0x12455C, w={3: fneg(r(7)), 11: ldb(a("i1", -0xE))})
    step(0x12455F, w={12: fmul(r(1), r(5)), 8: fadd(r(8), r(12)), 14: ld(a("i3", -0x20))})
    step(0x124562, w={3: favg(r(11), r(3))})
    step(0x124564, w={9: fmul(r(1), r(4)), 1: fadd(r(9), r(13)), 10: ld(a("i3", -0x1E))})
    step(0x124567, w={10: fsub(r(10), r(14))}, st=[(a("i3", -7), r(0))])
    step(0x12456A, w={12: fmul(r(1), r(5)), 0: fsub(r(8), r(12))}, st=[(a("i3", -0x1A), r(7))])
    step(0x12456D, w={9: fadd(r(6), r(9)), 8: r(0)})
    step(0x124570, w={5: fmul(r(0), r(5)), 8: fadd(r(8), r(12))}, st=[(a("i3", -0x13), r(3))])
    step(0x124573, w={10: fmul(r(10), r(15))}, st=[(a("i0", 2), r(9))])
    step(0x124576, w={8: fsub(r(8), r(5))})
    step(0x124577, w={0: fmul(r(0), r(4)), 14: fadd(r(10), r(14))}, st=[(a("i3", -0xD), r(8))])
    step(0x12457A, w={8: fmul(r(8), r(4))}, st=[(a("i3", -0x1E), r(14))])
    step(0x12457D, w={0: fadd(r(2), r(0))})
    step(0x12457F)  # rts (db): the two instructions below run before the return
    step(0x124580, w={1: fadd(r(1), r(8))}, st=[(a("i0", 4), r(0))])
    step(0x124583, st=[(a("i0", 6), r(1))])
    return m


def random_scenario(rng) -> dict[int, int]:
    """Context floats (`CTX - 0x20 .. CTX - 4`) and the four frames."""
    mem = {a: fb.bits(rng.uniform(-1.0, 1.0)) for a in range(CTX - 0x20, CTX)}
    for a in range(IN_ADDR, IN_ADDR + 8):
        mem[a] = fb.bits(rng.uniform(-1.0, 1.0))
    return mem


def routine():
    """The harness description of the 4-frame routine (`tools/fx_oracle.py`)."""
    import fx_oracle as fo

    return fo.Routine(
        "resonant4",
        PC,
        END,
        SETS,
        lambda mem, trace=None: resonant_0x124497(mem, IN_ADDR, CTX, trace),
        random_scenario,
    )
