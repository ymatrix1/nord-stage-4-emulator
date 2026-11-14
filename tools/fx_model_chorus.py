#!/usr/bin/env python3
"""Reference models of the Mod 2 chorus routines `0x122E24` (type 4) and `0x122F77` (type 5).

Transliteration (see `tools/fx_pe.py`) of the two FX Core 2 routines that `kFx2Type` 4 and 5
(`CHOR`, `CHOR variation`) install in the modules `0xC0443740` and `0xC0443694`: a phase
accumulator drives an odd-polynomial sine, smoothed delay times are read from a circular stereo
delay line by linear interpolation and mixed with the dry signal. `docs/research/fx-mod2-types.md`
has the equations and the field table. The listing is the output of `tools/adi_dis.py`.
"""

from __future__ import annotations

import fx_biquad_check as fb
from fx_pe import (
    M32,
    Machine,
    fabs,
    fadd,
    ffloat,
    fl,
    fmul,
    fsub,
    iadd,
    imm,
    lift,
    wd,
)

CHOR4_PC, CHOR4_END = 0x122E24, 0x122F53
CHOR5_PC, CHOR5_END = 0x122F77, 0x1230AB
IN_ADDR, CTX, BUF = 0xB1000, 0xB2040, 0xB4000
#: Registers the pass leaves set that the routines read before writing (`M1`, ...).
REGS = {"m0": 0, "m1": 1, "m2": 2}

isub = lift(lambda a, b: (wd(a) - wd(b)) & M32)
ineg = lift(lambda a: (-wd(a)) & M32)
#: `Rn = TRUNC Fx`: round toward zero (the routines only see in-range values).
ftrunc = lift(lambda a: int(fl(a)) & M32)


def ifeq(az, new, old):
    """`IF EQ Rn = Rm`: per element the new value where the last compare set `AZ`."""
    return [new[c] if az[c] else old[c] for c in (0, 1)]


def chor4_0x122e24(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run `0x122E24` on `mem` (updated in place)."""
    m = Machine(mem, {**REGS, "i0": i0, "i1": i1}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x122E24, d={"i3": m.ir["i1"]})
    step(0x122E26, d={"i2": mem[a("i1", -0x16)]})
    step(0x122E28, w={2: ldb(a("i1", -0x18))})
    step(0x122E2A, w={0: ldb(a("i1", -0x20))})
    step(0x122E2C, w={7: ldb(a("i1", 0x14))})
    step(0x122E2E, w={3: iadd(r(2), r(0)), 1: ld(a("i3", 0x12))})
    step(0x122E31, w={11: ld(a("i3", -0x3))})
    step(0x122E33, st=[(a("i3", -0x18), r(3))])
    step(0x122E35, w={4: ld(m.mod(2, 0))})
    step(0x122E37, w={4: iadd(r(4), r(1))})
    step(0x122E38, w={14: imm(0xFFFFFFE1)})
    step(0x122E3A, w={1: ffloat(r(4), r(14)), 12: ld(a("i3", -0x1))})
    step(0x122E3D, d={"l6": 0x200})
    step(0x122E3F, w={1: fabs(r(1))})
    step(0x122E41, w={12: fsub(r(12), r(11))})
    step(0x122E42, w={15: r(1)})
    step(0x122E44, w={0: fadd(r(1), r(1)), 2: ldb(a("i1", 0x10))})
    step(0x122E47, w={12: fmul(r(12), r(7)), 13: ldb(a("i1", 0x1))})
    step(0x122E4A, w={0: fadd(r(0), r(2)), 14: ld(a("i3", 0x2))})
    step(0x122E4D, w={11: fadd(r(11), r(12))})
    step(0x122E4E, w={1: fmul(r(0), r(0))})
    step(0x122E50, w={14: fsub(r(14), r(13))})
    step(0x122E51, w={2: fmul(r(0), r(1))}, st=[(a("i3", -0x1), r(11))])
    step(0x122E54, w={14: fmul(r(14), r(7))})
    step(0x122E55, w={3: fmul(r(1), r(2)), 8: ldb(a("i1", 0xC))})
    step(0x122E58, w={4: fmul(r(0), r(8)), 9: ldb(a("i1", 0xD))})
    step(0x122E5B, w={5: fmul(r(2), r(9)), 10: ldb(a("i1", 0xE))})
    step(0x122E5E, w={6: fmul(r(3), r(10))})
    step(0x122E60, w={0: fadd(r(4), r(5)), 11: ld(a("i3", -0x1E))})
    step(0x122E63, w={13: fadd(r(13), r(14)), 12: ld(a("i3", -0x1B))})
    step(0x122E66, w={0: fadd(r(0), r(6)), 1: ldb(a("i1", 0xF))})
    step(0x122E69, w={12: fsub(r(12), r(11))})
    step(0x122E6A, w={0: fadd(r(0), r(1))}, st=[(a("i3", 0x2), r(13))])
    step(0x122E6D, w={12: fmul(r(12), r(7)), 1: ldb(a("i1", 0x11))})
    step(0x122E70, w={1: fmul(r(0), r(1)), 6: ldb(a("i1", 0xA))})
    step(0x122E73, w={11: fadd(r(11), r(12))})
    step(0x122E74, w={2: ldb(a("i1", 0xB))})
    step(0x122E76, st=[(a("i3", -0x1B), r(11))])
    step(0x122E78, w={2: r(2)})
    az = [wd(v) == 0 for v in r(2)]
    step(0x122E79, w={1: ifeq(az, r(15), r(1))})
    step(0x122E7C, w={3: ldb(a("i1", -0x1F))})
    step(0x122E7E, w={8: ldb(a("i1", 0xF))})
    step(0x122E80, w={2: fsub(r(8), r(1)), 4: ldb(a("i1", -0x1B))})
    step(0x122E83, d={"m5": 0x200})
    step(0x122E85, wx={2: m.y[1]}, wy={1: m.x[2]})
    step(0x122E87, w={0: r(1)})
    step(0x122E89, w={1: fmul(r(1), r(4)), 4: ldb(a("i1", -0x1A))})
    step(0x122E8C, d={"m6": 0x202})
    step(0x122E8E, w={1: fadd(r(1), r(3)), 3: ldb(a("i1", -0x19))})
    step(0x122E91, w={6: r(6)})
    az = [wd(v) == 0 for v in r(6)]
    step(0x122E92, w={2: ifeq(az, r(0), r(2))})
    step(0x122E95, w={2: fmul(r(2), r(4))})
    step(0x122E96, w={9: fadd(r(2), r(3))})
    step(0x122E98, w={0: ftrunc(r(1))})
    step(0x122E9A, w={10: ftrunc(r(9))})
    step(0x122E9C, w={4: ineg(r(0))})
    step(0x122E9E, w={13: ineg(r(10))})
    step(0x122EA0, w={0: ffloat(r(0), imm(0))}, st=[(a("i3", -0x15), r(4))])
    step(0x122EA3, w={10: ffloat(r(10), imm(0))}, st=[(a("i3", -0xF), r(13))])
    step(0x122EA6, w={1: fsub(r(1), r(0))})
    step(0x122EA7, w={11: fsub(r(9), r(10))})
    step(0x122EA9, w={2: fsub(r(8), r(1)), 13: ld(a("i3", -0x9))})
    step(0x122EAC, w={12: fsub(r(8), r(11)), 15: ldb(a("i1", 0x9))})
    step(0x122EAF, wx={2: m.y[1]}, wy={1: m.x[2]})
    step(0x122EB1, wx={12: m.y[11]}, wy={11: m.x[12]})
    step(0x122EB3, st=[(a("i3", -0x13), r(1))])
    step(0x122EB5, st=[(a("i3", -0x11), r(2))])
    step(0x122EB7, st=[(a("i3", -0xD), r(11))])
    step(0x122EB9, st=[(a("i3", -0xB), r(12))])
    step(0x122EBB, d={"b6": wd(m.x[13])})
    step(0x122EBD)
    for _ in range(wd(m.x[15])):
        step(0x122EC0, w={0: ld(m.mod(0, 0))})
        step(0x122EC2, w={8: ld(m.mod(0, 2))})
        step(0x122EC4, d={"i4": wd(m.x[13])})
        step(0x122EC6, d={"i6": mem[a("i3", -0x7)]})
        step(0x122EC8, w={6: r(0)})
        step(0x122ECA, w={14: r(8)})
        step(0x122ECC, w={1: ld(a("i3", 0x5))})
        step(0x122ECE, w={0: fadd(r(0), r(1)), 9: ld(a("i3", 0x7))})
        step(0x122ED1, w={8: fadd(r(8), r(9))})
        step(0x122ED2, st=[(m.mod(6, 6), r(0))])
        step(0x122ED4, wx={0: m.y[0]}, wy={0: m.x[0]})
        step(0x122ED6, st=[(a("i6"), r(0))], d={"i6": m.mod(6, 1)})
        step(0x122ED7, st=[(m.mod(6, 6), r(8))])
        step(0x122ED9, wx={8: m.y[8]}, wy={8: m.x[8]})
        step(0x122EDB, st=[(a("i6"), r(8))], d={"i6": m.mod(6, 1)})
        step(0x122EDC, w={1: imm(m.ir["i6"])})
        step(0x122EDE, st=[(a("i3", -0x7), r(1))])
        step(0x122EE0, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 5)})
        step(0x122EE1, st=[(a("i4"), r(0))], d={"i4": m.mod(4, 2)})
        step(0x122EE2, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 5)})
        step(0x122EE3, st=[(m.mod(4, 0), r(0))])
        step(0x122EE5, w={1: ld(a("i3", -0x15))})
        step(0x122EE7, d={"m3": wd(m.x[1])})
        step(0x122EE9, d={"m4": wd(m.y[1])})
        step(0x122EEB, d={"i4": m.mod(6, 3)})
        step(0x122EED, d={"i4": m.mod(4, 6)})
        step(0x122EEF, d={"i5": m.mod(6, 4)})
        step(0x122EF1, w={3: ld(a("i3", -0x13))})
        step(0x122EF3, w={4: ld(a("i3", -0x11))})
        step(0x122EF5, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 1)})
        step(0x122EF6, w={8: ld(m.mod(4, 0))})
        step(0x122EF8, w={0: fmul(r(3), r(0)), 1: ld(a("i5"))}, d={"i5": m.mod(5, 1)})
        step(0x122EFB, w={8: fmul(r(3), r(8)), 9: ld(m.mod(5, 0))})
        step(0x122EFE, w={1: fmul(r(4), r(1)), 3: ldb(a("i1", 0x2))})
        step(0x122F01, w={9: fmul(r(4), r(9))})
        step(0x122F03, w={6: fmul(r(6), r(3))}, wx={1: m.y[0]}, wy={0: m.x[1]})
        step(0x122F06, w={14: fmul(r(14), r(3))}, wx={9: m.y[8]}, wy={8: m.x[9]})
        step(0x122F09, w={7: fadd(r(0), r(1)), 4: ldb(a("i1", -0x1))})
        step(0x122F0C, w={15: fadd(r(8), r(9)), 1: ld(a("i3", -0xF))})
        step(0x122F0F, w={7: fmul(r(7), r(4)), 11: ld(a("i3", -0xD))})
        step(0x122F12, w={15: fmul(r(15), r(4)), 12: ld(a("i3", -0xB))})
        step(0x122F15, d={"m3": wd(m.x[1])})
        step(0x122F17, d={"m4": wd(m.y[1])})
        step(0x122F19, d={"i4": m.mod(6, 3)})
        step(0x122F1B, d={"i4": m.mod(4, 6)})
        step(0x122F1D, d={"i5": m.mod(6, 4)})
        step(0x122F1F, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 1)})
        step(0x122F20, w={8: ld(m.mod(4, 0))})
        step(0x122F22, w={0: fmul(r(11), r(0)), 1: ld(a("i5"))}, d={"i5": m.mod(5, 1)})
        step(0x122F25, w={8: fmul(r(11), r(8)), 9: ld(m.mod(5, 0))})
        step(0x122F28, w={1: fmul(r(12), r(1))})
        step(0x122F2A, w={9: fmul(r(12), r(9))})
        step(0x122F2C, wx={1: m.y[0]}, wy={0: m.x[1]})
        step(0x122F2E, wx={9: m.y[8]}, wy={8: m.x[9]})
        step(0x122F30, w={0: fadd(r(0), r(1)), 4: ldb(a("i1"))})
        step(0x122F33, w={8: fadd(r(8), r(9))})
        step(0x122F34, w={0: fmul(r(0), r(4))})
        step(0x122F37, w={8: fmul(r(8), r(4))})
        step(0x122F3A, w={0: fadd(r(0), r(7)), 4: ldb(a("i1", 0x4))})
        step(0x122F3D, w={8: fadd(r(8), r(15))})
        step(0x122F40, w={1: fmul(r(0), r(4))})
        step(0x122F43, w={9: fmul(r(8), r(4))})
        step(0x122F46, w={0: fadd(r(0), r(6))}, st=[(a("i3", 0x5), r(1))])
        step(0x122F49, w={8: fadd(r(8), r(14))}, st=[(a("i3", 0x7), r(9))])
        step(0x122F4C, st=[(a("i0"), r(0))], d={"i0": m.mod(0, 2)})
        step(0x122F4F, st=[(a("i0"), r(8))], d={"i0": m.mod(0, 2)})
    step(0x122F52, d={"l6": 0x0})
    return m


def chor5_0x122f77(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run `0x122F77` on `mem` (updated in place)."""
    m = Machine(mem, {**REGS, "i0": i0, "i1": i1}, trace)
    r, ld, ldb, a, step = m.r, m.ld, m.ldb, m.a, m.step
    step(0x122F77, d={"i3": m.ir["i1"]})
    step(0x122F79, d={"i2": mem[a("i1", -0x16)]})
    step(0x122F7B, w={2: ldb(a("i1", -0x18))})
    step(0x122F7D, w={0: ldb(a("i1", -0x1D))})
    step(0x122F7F, w={7: ldb(a("i1", 0x6))})
    step(0x122F81, w={3: iadd(r(2), r(0))})
    step(0x122F83, w={11: ld(a("i3", -0x9))})
    step(0x122F85, st=[(a("i3", -0x18), r(3))])
    step(0x122F87, w={4: ld(m.mod(2, 0))})
    step(0x122F89, w={14: imm(0xFFFFFFE1)})
    step(0x122F8B, w={1: ffloat(r(4), r(14)), 12: ld(a("i3", -0x8))})
    step(0x122F8E, d={"l6": 0x200})
    step(0x122F90, w={15: fabs(r(1))})
    step(0x122F92, w={12: fsub(r(12), r(11))})
    step(0x122F93, w={12: fmul(r(12), r(7)), 13: ldb(a("i1", -0x6))})
    step(0x122F96, w={14: ld(a("i3", -0x5))})
    step(0x122F98, w={11: fadd(r(11), r(12))})
    step(0x122F99, w={14: fsub(r(14), r(13))})
    step(0x122F9A, st=[(a("i3", -0x8), r(11))])
    step(0x122F9C, w={14: fmul(r(14), r(7))})
    step(0x122F9D, w={11: ld(a("i3", -0x1C))})
    step(0x122F9F, w={13: fadd(r(13), r(14)), 12: ld(a("i3", -0x1A))})
    step(0x122FA2, w={12: fsub(r(12), r(11))})
    step(0x122FA3, st=[(a("i3", -0x5), r(13))])
    step(0x122FA5, w={12: fmul(r(12), r(7))})
    step(0x122FA6, w={11: fadd(r(11), r(12))})
    step(0x122FA7, st=[(a("i3", -0x1A), r(11))])
    step(0x122FA9, w={1: r(15)})
    step(0x122FAB, w={2: ld(a("i3", -0x20))})
    step(0x122FAD, w={3: ldb(a("i1", -0x1E))})
    step(0x122FAF, w={2: fsub(r(2), r(1))})
    step(0x122FB0, w={2: fmul(r(2), r(3))})
    step(0x122FB1, w={1: fadd(r(1), r(2))})
    step(0x122FB2, st=[(a("i3", -0x20), r(1))])
    step(0x122FB4, w={3: ldb(a("i1", -0x1A))})
    step(0x122FB6, w={8: ldb(a("i1", 0x3))})
    step(0x122FB8, w={4: ldb(a("i1", -0x19))})
    step(0x122FBA, d={"m5": 0x200})
    step(0x122FBC, w={1: fmul(r(1), r(4))})
    step(0x122FBD, d={"m6": 0x202})
    step(0x122FBF, w={1: fadd(r(1), r(3))})
    step(0x122FC0, w={0: ftrunc(r(1))})
    step(0x122FC2, w={4: ineg(r(0))})
    step(0x122FC4, w={0: ffloat(r(0), imm(0))}, st=[(a("i3", -0x15), r(4))])
    step(0x122FC7, w={1: fsub(r(1), r(0))})
    step(0x122FC8, w={2: fsub(r(8), r(1)), 13: ld(a("i3", -0xF))})
    step(0x122FCB, w={15: ldb(a("i1", 0x2))})
    step(0x122FCD, wx={2: m.y[1]}, wy={1: m.x[2]})
    step(0x122FCF, st=[(a("i3", -0x13), r(1))])
    step(0x122FD1, st=[(a("i3", -0x11), r(2))])
    step(0x122FD3, d={"b6": wd(m.x[13])})
    step(0x122FD5)
    for _ in range(wd(m.x[15])):
        step(0x122FD8, w={0: ld(m.mod(0, 0))})
        step(0x122FDA, w={8: ld(m.mod(0, 2))})
        step(0x122FDC, d={"i4": mem[a("i3", -0xF)]})
        step(0x122FDE, d={"i6": mem[a("i3", -0xD)]})
        step(0x122FE0, w={6: r(0)})
        step(0x122FE2, w={14: r(8)})
        step(0x122FE4, st=[(0xC285D, r(6))])
        step(0x122FE7, st=[(0xC285F, r(14))])
        step(0x122FEA, st=[(m.mod(6, 6), r(0))])
        step(0x122FEC, wx={0: m.y[0]}, wy={0: m.x[0]})
        step(0x122FEE, st=[(a("i6"), r(0))], d={"i6": m.mod(6, 1)})
        step(0x122FEF, st=[(m.mod(6, 6), r(8))])
        step(0x122FF1, wx={8: m.y[8]}, wy={8: m.x[8]})
        step(0x122FF3, st=[(a("i6"), r(8))], d={"i6": m.mod(6, 1)})
        step(0x122FF4, w={1: imm(m.ir["i6"])})
        step(0x122FF6, st=[(a("i3", -0xD), r(1))])
        step(0x122FF8, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 5)})
        step(0x122FF9, st=[(a("i4"), r(0))], d={"i4": m.mod(4, 2)})
        step(0x122FFA, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 5)})
        step(0x122FFB, st=[(m.mod(4, 0), r(0))])
        step(0x122FFD, w={1: ld(a("i3", -0x15))})
        step(0x122FFF, d={"m3": wd(m.x[1])})
        step(0x123001, d={"m4": wd(m.y[1])})
        step(0x123003, d={"i4": m.mod(6, 3)})
        step(0x123005, d={"i4": m.mod(4, 6)})
        step(0x123007, d={"i5": m.mod(6, 4)})
        step(0x123009, w={3: ld(a("i3", -0x13))})
        step(0x12300B, w={4: ld(a("i3", -0x11))})
        step(0x12300D, w={0: ld(a("i4"))}, d={"i4": m.mod(4, 1)})
        step(0x12300E, w={8: ld(m.mod(4, 0))})
        step(0x123010, w={0: fmul(r(3), r(0)), 1: ld(a("i5"))}, d={"i5": m.mod(5, 1)})
        step(0x123013, w={8: fmul(r(3), r(8)), 9: ld(m.mod(5, 0))})
        step(0x123016, w={1: fmul(r(4), r(1))})
        step(0x123018, w={9: fmul(r(4), r(9))})
        step(0x12301A, wx={1: m.y[0]}, wy={0: m.x[1]})
        step(0x12301C, wx={9: m.y[8]}, wy={8: m.x[9]})
        step(0x12301E, w={7: fadd(r(0), r(1)), 4: ldb(a("i1", -0x8))})
        step(0x123021, w={15: fadd(r(8), r(9)), 2: ld(a("i3", 0x3))})
        step(0x123024, w={0: fmul(r(7), r(4))})
        step(0x123026, w={1: fmul(r(15), r(4))})
        step(0x123028, w={0: fmul(r(0), r(2)), 7: ldb(a("i1", 0x7))})
        step(0x12302B, w={1: fmul(r(1), r(2)), 15: ldb(a("i1", 0x13))})
        step(0x12302E, w={9: ld(0xC285D)})
        step(0x123031, w={10: ld(0xC285F)})
        step(0x123034, w={4: fmul(r(7), r(0)), 2: ld(a("i3", 0xB))})
        step(0x123037, w={11: fmul(r(9), r(15)), 12: ld(a("i3", 0x15))})
        step(0x12303A, w={0: fadd(r(2), r(4)), 3: ldb(a("i1", 0x9))})
        step(0x12303D, w={6: fadd(r(11), r(12)), 13: ldb(a("i1", 0x14))})
        step(0x123040, w={5: fmul(r(0), r(3))})
        step(0x123042, w={14: fmul(r(6), r(13))})
        step(0x123044, w={2: fsub(r(5), r(4))})
        step(0x123046, w={12: fsub(r(11), r(14))})
        step(0x123048, w={4: fmul(r(7), r(1))})
        step(0x12304A, w={11: fmul(r(10), r(15))})
        step(0x12304C, w={1: fadd(r(2), r(4)), 3: ldb(a("i1", 0x9))})
        step(0x12304F, w={8: fadd(r(11), r(12)), 13: ldb(a("i1", 0x14))})
        step(0x123052, w={5: fmul(r(1), r(3))})
        step(0x123054, w={14: fmul(r(8), r(13))})
        step(0x123056, w={5: fsub(r(5), r(4)), 7: ldb(a("i1", 0x8))})
        step(0x123059, w={12: fsub(r(11), r(14)), 15: ldb(a("i1", 0xF))})
        step(0x12305C, st=[(a("i3", 0xB), r(5))])
        step(0x12305E, w={4: fmul(r(7), r(0))}, st=[(a("i3", 0x15), r(12))])
        step(0x123061, w={11: fmul(r(9), r(15)), 2: ld(a("i3", 0xD))})
        step(0x123064, w={0: fadd(r(2), r(4)), 12: ld(a("i3", 0x11))})
        step(0x123067, w={9: fadd(r(11), r(12)), 3: ldb(a("i1", 0xA))})
        step(0x12306A, w={5: fmul(r(0), r(3)), 13: ldb(a("i1", 0x10))})
        step(0x12306D, w={14: fmul(r(9), r(13))})
        step(0x12306F, w={2: fsub(r(4), r(5))})
        step(0x123071, w={12: fsub(r(14), r(11))})
        step(0x123073, w={4: fmul(r(7), r(1))})
        step(0x123075, w={11: fmul(r(10), r(15))})
        step(0x123077, w={1: fadd(r(2), r(4)), 3: ldb(a("i1", 0xA))})
        step(0x12307A, w={10: fadd(r(11), r(12)), 13: ldb(a("i1", 0x10))})
        step(0x12307D, w={5: fmul(r(1), r(3))})
        step(0x12307F, w={14: fmul(r(10), r(13))})
        step(0x123081, w={5: fsub(r(4), r(5))})
        step(0x123083, w={12: fsub(r(14), r(11))})
        step(0x123085, w={9: fadd(r(9), r(6))}, st=[(a("i3", 0xD), r(5))])
        step(0x123088, w={10: fadd(r(10), r(8))}, st=[(a("i3", 0x11), r(12))])
        step(0x12308B, w={0: fadd(r(0), r(9))})
        step(0x12308E, w={1: fadd(r(1), r(10)), 3: ldb(a("i1", -0x5))})
        step(0x123091, w={8: ld(0xC285D)})
        step(0x123094, w={9: ld(0xC285F)})
        step(0x123097, w={8: fmul(r(8), r(3))})
        step(0x12309A, w={9: fmul(r(9), r(3))})
        step(0x12309D, w={0: fadd(r(0), r(8))})
        step(0x1230A0, w={1: fadd(r(1), r(9))})
        step(0x1230A3, st=[(a("i0"), r(0))], d={"i0": m.mod(0, 2)})
        step(0x1230A6, st=[(a("i0"), r(1))], d={"i0": m.mod(0, 2)})
    step(0x1230A9, d={"l6": 0x0})
    return m


def random_scenario(rng, kind: int = 4) -> dict[int, int]:
    """Context, ring, input frames (1 to 3 loop iterations)."""
    count = rng.randrange(1, 4)
    mem = {a: fb.bits(rng.uniform(-1.0, 1.0)) for a in range(CTX - 0x20, CTX + 0x26)}
    for a in range(IN_ADDR, IN_ADDR + 4 * count):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    for a in range(BUF, BUF + 0x410):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    mem.update({
        CTX - 0x16: CTX - 0x18,
        CTX - 0x18: rng.randrange(1 << 32),
    })  # fmt: skip
    if kind == 4:
        mem.update({
            CTX - 0x20: rng.randrange(1 << 24),
            CTX - 9: BUF,
            CTX - 7: BUF + 2 * rng.randrange(0, 0x100),
            CTX + 9: count,
            CTX + 0x12: rng.randrange(1 << 32),
            CTX + 0x14: fb.bits(rng.choice([0.9908, 0.5, 0.0, 0.99])),
        })  # fmt: skip
    else:
        mem.update({
            CTX - 0x1D: rng.randrange(1 << 24),
            CTX - 0xF: BUF,
            CTX - 0xD: BUF + 2 * rng.randrange(0, 0x100),
            CTX + 2: count,
            CTX + 6: fb.bits(rng.choice([0.9908, 0.5, 0.0, 0.99])),
        })  # fmt: skip
    return mem


def routine(kind: int = 4):
    """The harness description of one of the routines (`tools/fx_oracle.py`)."""
    import fx_oracle as fo

    fn, pc, end = (
        (chor4_0x122e24, CHOR4_PC, CHOR4_END)
        if kind == 4
        else (chor5_0x122f77, CHOR5_PC, CHOR5_END)
    )
    return fo.Routine(
        f"chor{kind}",
        pc,
        end,
        {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1},
        lambda mem, trace=None: fn(mem, IN_ADDR, CTX, trace),
        lambda rng: random_scenario(rng, kind),
    )


class Kind:
    """Registry adapter: `routine()` of one of the two chorus routines (`tools/fx_models.py`)."""

    def __init__(self, kind: int):
        self.kind = kind

    def routine(self):
        return routine(self.kind)
