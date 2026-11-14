#!/usr/bin/env python3
"""Reference model of the FX delay routine `0x1206B0` (module list `0xC0279098`).

Transliteration of the routine, one `Machine.step` per instruction (see `tools/fx_pe.py`). It
processes 2 stereo frames per call: a time-ratio smoother, 8 one-pole parameter smoothers with the
`0.9908` coefficient, a peak meter, a fractional tap read from two packed 16-bit-float delay
memories with an interpolation table, a feedback mix and the write of the new packed samples.
`docs/research/fx-routine-models.md` s2 has the equations and the field table.
"""

from __future__ import annotations

import math

import fx_biquad_check as fb
from fx_pe import (
    M32,
    Machine,
    comp,
    cond_ge,
    fabs,
    fadd,
    favg,
    fclip,
    fdep_or,
    fdep_r,
    fext_p,
    ffix,
    ffloat,
    fmax,
    fmin,
    fmul,
    fpack,
    fpack_p,
    frecips,
    fsub,
    funpack_p,
    iadd,
    imm,
    ishl,
    swap,
    wd,
)

PC, END = 0x1206B0, 0x1208AE  # entry, last pc before `rts`
#: Direct (absolute) data addresses: pass pointers of the two delay memories and scratch pairs.
PTR_A, PTR_B, SCRATCH_1, SCRATCH_2 = 0xC03D4, 0xC03D5, 0xC02C8, 0xC02CA


def delay_0x1206b0(mem: dict[int, int], i0: int, i1: int, trace=None) -> Machine:
    """Run the routine on `mem` (updated in place). `M0 = 0`, `M2 = 2` as set by the pass."""
    m = Machine(mem, {"i0": i0, "i1": i1, "m0": 0, "m2": 2}, trace)
    r, ld, ldb, a = m.r, m.ld, m.ldb, m.a
    step = m.step
    one, two = 0x3F800000, 0x40000000
    # --- time ratio: r0 = clamp(ratio (1 - k) + state k) ------------------------------------
    step(0x1206B0, d={"i3": m.ir["i1"]})
    step(0x1206B2, w={0: ld(a("i3", 7))})
    step(0x1206B4, w={2: swap(r(0))})
    step(0x1206B6, w={0: fmax(r(0), r(2)), 4: ld(a("i3", 9))})
    step(0x1206B9, w={2: swap(r(4))})
    step(0x1206BB, w={4: fmax(r(4), r(2))})
    step(0x1206BD, w={2: imm(one)})
    step(0x1206C0, w={0: fmin(r(0), r(2))})
    step(0x1206C2, w={5: imm(0x3C23D70A)})
    step(0x1206C5, w={0: fmax(r(0), r(5)), 2: ld(a("i3", 0xC))})
    step(0x1206C8, w={4: fmax(r(4), r(5)), 3: ldb(a("i1", 0xE))})
    step(0x1206CB, w={11: imm(two)})
    step(0x1206CE, w={0: frecips(r(4)), 6: r(0)})
    step(0x1206D1, w={14: fmul(r(0), r(4))})
    step(0x1206D3, w={6: fmul(r(0), r(6)), 0: fsub(r(11), r(14))})
    step(0x1206D5, w={14: fmul(r(0), r(14))})
    step(0x1206D7, w={6: fmul(r(0), r(6)), 0: fsub(r(11), r(14))})
    step(0x1206D9, w={0: fmul(r(0), r(6))})
    step(0x1206DA, w={2: fmul(r(2), r(3))})
    step(0x1206DB, w={3: fmul(r(0), r(3))})
    step(0x1206DD, w={0: fsub(r(0), r(3))})
    step(0x1206DE, w={0: fadd(r(0), r(2))})
    step(0x1206DF, w={2: ldb(a("i1", 0x24))})
    step(0x1206E1, w={0: fmin(r(0), r(2))})
    step(0x1206E3, w={2: ldb(a("i1", 0x25))})
    step(0x1206E5, w={0: fmax(r(0), r(2))})
    step(0x1206E7, st=[(a("i3", 0xC), r(0))])
    # --- eight one-pole smoothers x' = t + k (x - t), k = 0.9908 -----------------------------
    step(0x1206E9, w={8: imm(0x3F7DA3C2)})
    step(0x1206EC, w={1: ldb(a("i1", -0x1B))})
    step(0x1206EE, w={12: ld(a("i3", -0x14))})
    step(0x1206F0, w={12: fsub(r(12), r(1)), 2: ld(a("i3", -0x16))})
    step(0x1206F3, w={12: fmul(r(8), r(12)), 13: ld(a("i3", -0xE))})
    step(0x1206F6, w={1: fadd(r(1), r(12)), 3: ld(a("i3", 4))})
    step(0x1206F9, w={13: fsub(r(13), r(2))}, st=[(a("i3", -0x14), r(1))])
    step(0x1206FC, w={13: fmul(r(8), r(13)), 1: ld(a("i3", -0x18))})
    step(0x1206FF, w={2: fadd(r(2), r(13)), 14: ld(a("i3", -0x20))})
    step(0x120702, w={14: fsub(r(14), r(3))}, st=[(a("i3", -0xE), r(2))])
    step(0x120705, w={14: fmul(r(8), r(14)), 12: ld(a("i3", -0x10))})
    step(0x120708, w={3: fadd(r(3), r(14)), 4: ld(a("i3", -0x1A))})
    step(0x12070B, w={12: fsub(r(12), r(1)), 5: ld(a("i3", -0x12))})
    step(0x12070E, w={5: fsub(r(5), r(4))}, st=[(a("i3", -0x20), r(3))])
    step(0x120711, d={"m4": mem[a("i1", 0x10)]})
    step(0x120713, w={5: fmul(r(8), r(5)), 0: ld(a("i3", -7))})
    step(0x120716, w={12: fmul(r(8), r(12))})
    step(0x120718, w={4: fadd(r(4), r(5))})
    step(0x120719, d={"m5": mem[a("i1", 0x11)]})
    step(0x12071B, w={1: fadd(r(1), r(12))}, st=[(a("i3", -0x12), r(4))])
    step(0x12071E, w={0: fmul(r(0), r(8)), 3: ld(a("i3", -0xB))})
    step(0x120721, w={2: fmul(r(3), r(8))}, st=[(a("i3", -0x10), r(1))])
    step(0x120724, w={0: fadd(r(0), r(3))})
    step(0x120725, d={"i4": mem[a("i1", 0xF)]})
    step(0x120727, w={0: fsub(r(0), r(2))})
    step(0x120728, w={1: imm(one)})
    step(0x12072B, w={1: favg(r(1), r(0)), 8: ldb(a("i1", 0x12))})
    step(0x12072E, st=[(a("i3", -7), r(0))])
    step(0x120730, st=[(a("i3", -9), r(1))])
    # --- read positions in the two packed delay memories -----------------------------------
    step(0x120732, w={2: ld(PTR_B)})
    step(0x120735, w={2: iadd(r(2), r(8)), 9: ldb(a("i1", 0x13))})
    step(0x120738, d={"i2": wd(m.x[2])})
    step(0x12073A, d={"i6": mem[PTR_A]})
    step(0x12073D, d={"i7": mem[PTR_B]})
    step(0x120740, d={"i6": m.ir["i6"] + m.ir["m4"]})
    step(0x120742, d={"i7": m.ir["i7"] + m.ir["m5"]})
    step(0x120744, w={3: ld(a("i2"))})
    step(0x120746, d={"i2": m.ir["i2"] + 0x28})
    step(0x120749, w={4: ld(a("i2"))})
    step(0x12074B, w={3: ishl(r(3), r(9))})
    step(0x12074D, w={4: ishl(r(4), r(9))})
    step(0x12074F, d={"i2": m.ir["i3"]})
    step(0x120751, w={3: fext_p(r(3), [0, 0], [16, 16], [False, False])})
    step(0x120754, w={4: fext_p(r(4), [0, 0], [16, 16], [False, False])})
    step(0x120757, w={2: imm(0xFFFFFFF1)})
    step(0x120759, w={3: ffloat(r(3), r(2))})
    step(0x12075B, w={4: ffloat(r(4), r(2))})
    step(0x12075D, w={8: imm(one)})
    step(0x120760, w={1: fsub(r(8), r(3))})
    step(0x120762, w={2: fsub(r(8), r(4))})
    step(0x120764, wx={1: m.y[3]}, wy={3: m.x[1]})
    step(0x120766, wx={2: m.y[4]}, wy={4: m.x[2]})
    step(0x120768, st=[(SCRATCH_1, r(3))])
    step(0x12076B, st=[(SCRATCH_2, r(4))])
    step(0x12076E, d={"i2": m.ir["i2"] + 0x1D})
    # --- the two frames, wet / dry ---------------------------------------------------------
    step(0x120771, w={0: ld(a("i0", 0))})
    step(0x120773, w={8: ld(a("i0", 2))})
    step(0x120775, w={1: ldb(a("i1", -0x14))})
    step(0x120777, w={6: ld(a("i3", -0x12))})
    step(0x120779, w={5: fmul(r(0), r(1)), 4: ld(a("i3", -0x10))})
    step(0x12077C, w={13: fmul(r(8), r(1))})
    step(0x12077E, w={3: swap(r(0))})
    step(0x120780, w={11: swap(r(8))})
    step(0x120782, w={3: fadd(r(3), r(0))})
    step(0x120783, w={11: fadd(r(11), r(8))})
    step(0x120784, w={3: fmul(r(3), r(4)), 2: ld(a("i2", 0))})
    step(0x120787, w={11: fmul(r(11), r(4)), 10: ld(a("i2", 2))})
    step(0x12078A, w={5: fadd(r(5), r(3)), 4: ld(a("i3", -0xE))})
    step(0x12078D, w={13: fadd(r(13), r(11))})
    step(0x12078E, w={3: swap(r(2))})
    step(0x120790, w={11: swap(r(10))})
    step(0x120792, w={3: fmul(r(3), r(4))})
    step(0x120793, w={11: fmul(r(11), r(4))})
    step(0x120794, w={2: fmul(r(2), r(6))})
    step(0x120795, w={10: fmul(r(10), r(6))})
    step(0x120796, w={2: fadd(r(2), r(3))})
    step(0x120797, w={10: fadd(r(10), r(11))})
    step(0x120798, w={5: fadd(r(5), r(2))})
    step(0x120799, w={13: fadd(r(13), r(10))})
    # --- output peak meter (decaying maximum) ------------------------------------------------
    step(0x12079A, w={2: ld(a("i3", 7))})
    step(0x12079C, w={3: ldb(a("i1", 0xB))})
    step(0x12079E, w={4: fmul(r(2), r(3))})
    step(0x1207A0, w={1: fabs(r(5))})
    step(0x1207A2, w={2: fabs(r(13))})
    flags = comp(r(1), r(2))
    step(0x1207A4)
    step(0x1207A5, w={2: cond_ge(flags, r(1), r(2))})
    flags = comp(r(4), r(2))
    step(0x1207A7)
    step(0x1207A8, w={2: cond_ge(flags, r(4), r(2))})
    step(0x1207AA, st=[(a("i3", 7), r(2))])
    # --- write the wet frames to the output buffer ------------------------------------------
    step(0x1207AC, w={3: ld(a("i4", 0))})
    step(0x1207AE, w={15: ld(a("i4", 2))})
    step(0x1207B0, st=[(a("i4"), r(5))], d={"i4": m.ir["i4"] + 2})
    step(0x1207B1, st=[(a("i4"), r(13))], d={"i4": m.ir["i4"] + 2})
    # --- feedback mix and the packing coefficients ----------------------------------------
    step(0x1207B2, w={1: ldb(a("i1", -9))})
    step(0x1207B4, w={12: fmul(r(1), r(3)), 2: ld(a("i3", -5))})
    step(0x1207B7, w={5: fadd(r(2), r(12)), 3: ldb(a("i1", -7))})
    step(0x1207BA, w={11: fmul(r(5), r(3)), 1: ldb(a("i1", -8))})
    step(0x1207BD, w={2: fmul(r(1), r(5)), 11: fsub(r(11), r(12)), 3: ld(a("i3", -3))})
    step(0x1207C0, w={5: fadd(r(2), r(3)), 1: ldb(a("i1", -6))})
    step(0x1207C3, w={6: fmul(r(5), r(1))})
    step(0x1207C5, w={6: fsub(r(2), r(6)), 1: ldb(a("i1", -9))})
    step(0x1207C8, w={12: fmul(r(1), r(15))})
    step(0x1207CA, w={4: fadd(r(11), r(12)), 3: ldb(a("i1", -7))})
    step(0x1207CD, w={9: fmul(r(4), r(3)), 1: ldb(a("i1", -8))})
    step(0x1207D0, w={10: fmul(r(1), r(4)), 9: fsub(r(9), r(12))})
    step(0x1207D2, w={13: fadd(r(10), r(6)), 1: ldb(a("i1", -6))})
    step(0x1207D5, w={6: fmul(r(13), r(1))}, st=[(a("i3", -5), r(9))])
    step(0x1207D8, w={6: fsub(r(10), r(6))})
    step(0x1207DA, st=[(a("i3", -3), r(6))])
    # --- second peak meter ---------------------------------------------------------------------
    step(0x1207DC, w={2: ld(a("i3", 9))})
    step(0x1207DE, w={3: ldb(a("i1", 0xB))})
    step(0x1207E0, w={4: fmul(r(2), r(3))})
    step(0x1207E2, w={1: fabs(r(5))})
    step(0x1207E4, w={2: fabs(r(13))})
    flags = comp(r(1), r(2))
    step(0x1207E6)
    step(0x1207E7, w={2: cond_ge(flags, r(1), r(2))})
    flags = comp(r(4), r(2))
    step(0x1207E9)
    step(0x1207EA, w={2: cond_ge(flags, r(4), r(2))})
    step(0x1207EC, st=[(a("i3", 9), r(2))])
    # --- table index, delay-memory update ------------------------------------------------------
    step(0x1207EE, w={3: ld(a("i3", 0xC))})
    step(0x1207F0, w={5: fmul(r(5), r(3)), 6: ldb(a("i1", -0xC))})
    step(0x1207F3, w={13: fmul(r(13), r(3))})
    step(0x1207F4, w={1: imm(0x3E99999A)})
    step(0x1207F7, w={4: fmul(r(5), r(1)), 7: ldb(a("i1", -0x1D))})
    step(0x1207FA, w={12: fmul(r(13), r(1))})
    step(0x1207FC, w={4: fclip(r(4), r(6)), 3: ldb(a("i1", -0x1E))})
    step(0x1207FF, w={12: fclip(r(12), r(6))})
    step(0x120801, d={"i10": mem[a("i3", 6)]})
    step(0x120803, w={4: ffix(r(4), r(3)), 2: ldb(a("i1", -0x1C))})
    step(0x120806, w={12: ffix(r(12), r(3))})
    step(0x120808, w={1: fext_p(r(4), [22, 22], [10, 10], [True, True])})
    step(0x12080B, w={9: fext_p(r(12), [22, 22], [10, 10], [True, True])})
    step(0x12080E, w={3: fdep_r(r(4), r(2))}, d={"m11": wd(m.x[1])})
    step(0x120811, w={11: fdep_r(r(12), r(2))}, d={"m13": wd(m.x[9])})
    step(0x120814, w={3: ffloat(r(3), r(7))}, d={"m12": wd(m.y[1])})
    step(0x120817, w={11: ffloat(r(11), r(7))}, d={"m14": wd(m.y[9])})
    step(0x12081A, w={4: ld((a("i10") + m.ir["m11"]) & M32)})
    step(0x12081C, w={12: ld((a("i10") + m.ir["m13"]) & M32)})
    step(0x12081E, w={1: ld((a("i10") + m.ir["m12"]) & M32)})
    step(0x120820, w={9: ld((a("i10") + m.ir["m14"]) & M32)})
    step(0x120822, wx={1: m.y[4]}, wy={4: m.x[1]})
    step(0x120824, wx={9: m.y[12]}, wy={12: m.x[9]})
    step(0x120826, w={2: fsub(r(1), r(4))})
    step(0x120828, w={10: fsub(r(9), r(12))})
    step(0x12082A, w={1: fmul(r(2), r(3)), 6: ldb(a("i1", -0x1F))})
    step(0x12082D, w={9: fmul(r(10), r(11))})
    step(0x12082F, w={2: fadd(r(4), r(1)), 3: ldb(a("i1", -0x20))})
    step(0x120832, w={10: fadd(r(12), r(9))})
    step(0x120834, w={1: imm(0x406429FB)})
    step(0x120837, w={2: fmul(r(2), r(1))})
    step(0x120838, w={10: fmul(r(10), r(1))})
    step(0x120839, w={2: fmul(r(2), r(6))})
    step(0x12083A, w={10: fmul(r(10), r(6))})
    step(0x12083B, w={1: fmul(r(5), r(6))})
    step(0x12083D, w={9: fmul(r(13), r(6))})
    step(0x12083F, w={5: fsub(r(5), r(1))})
    step(0x120840, w={13: fsub(r(13), r(9))})
    step(0x120841, w={5: fadd(r(5), r(2))})
    step(0x120842, w={13: fadd(r(13), r(10))})
    step(0x120843, w={0: fmul(r(0), r(3))})
    step(0x120844, w={8: fmul(r(8), r(3))})
    step(0x120845, wx={13: m.y[5]}, wy={5: m.x[13]})
    step(0x120847, w={1: fpack_p(r(5))})
    step(0x120849, w={2: fpack_p(r(13))})
    step(0x12084B, w={1: fdep_or(r(1), r(2), [16, 16], [16, 16])})
    step(0x12084E, w={3: ld(a("i6"))})
    step(0x120850, w={2: swap(r(1))})
    step(0x120852, wx={3: m.y[2]}, wy={2: m.x[3]})
    step(0x120854, st=[(a("i6"), r(3))])
    step(0x120856, d={"i6": m.ir["i6"] + 0x14})
    step(0x120859, w={3: ld(a("i6"))})
    step(0x12085B, wx={3: m.y[1]}, wy={1: m.x[3]})
    step(0x12085D, st=[(a("i6"), r(3))])
    step(0x12085F, w={7: ld(a("i7"))})
    step(0x120861, w={2: fext_p(r(7), [16, 16], [16, 16], [False, False])})
    step(0x120864, w={1: funpack_p(r(7))})
    step(0x120866, w={2: funpack_p(r(2))})
    step(0x120868, d={"i7": m.ir["i7"] + 0x28})
    step(0x12086B, w={7: ld(a("i7"))})
    step(0x12086D, w={4: fext_p(r(7), [16, 16], [16, 16], [False, False])})
    step(0x120870, w={3: funpack_p(r(7))})
    step(0x120872, w={4: funpack_p(r(4))})
    step(0x120874, w={5: ld(SCRATCH_1)})
    step(0x120877, w={6: ld(SCRATCH_2)})
    step(0x12087A, w={7: fmul(r(5), r(1))})
    step(0x12087C, w={9: fmul(r(5), r(2))})
    step(0x12087E, wx={9: m.y[7]}, wy={7: m.x[9]})
    step(0x120880, w={7: fadd(r(7), r(9))})
    step(0x120881, w={1: fmul(r(6), r(3))})
    step(0x120883, w={2: fmul(r(6), r(4))})
    step(0x120885, wx={2: m.y[1]}, wy={1: m.x[2]})
    step(0x120887, w={9: fadd(r(1), r(2))})
    step(0x120889, st=[(a("i2", 0), r(7))])
    step(0x12088B, st=[(a("i2", 2), r(9))])
    step(0x12088D, w={1: ldb(a("i1", -1))})
    step(0x12088F, w={2: fmul(r(7), r(1)), 4: ldb(a("i1", 0))})
    step(0x120892, w={3: fmul(r(7), r(4)), 6: ld(a("i3", 2))})
    step(0x120895, w={5: fadd(r(2), r(6)), 1: ldb(a("i1", 1))})
    step(0x120898, w={4: fmul(r(5), r(1))})
    step(0x12089A, w={3: fadd(r(3), r(4)), 1: ldb(a("i1", -1))})
    step(0x12089D, w={0: fadd(r(0), r(5))})
    step(0x12089E, w={10: fmul(r(9), r(1)), 4: ldb(a("i1", 0))})
    step(0x1208A1, w={11: fmul(r(9), r(4)), 1: ldb(a("i1", 1))})
    step(0x1208A4, w={13: fadd(r(10), r(3))})
    step(0x1208A6, w={12: fmul(r(13), r(1))})
    step(0x1208A8, w={11: fadd(r(11), r(12))})
    step(0x1208A9, w={8: fadd(r(8), r(13))}, st=[(a("i3", 2), r(11))])
    step(0x1208AC, st=[(a("i0"), r(0))], d={"i0": m.ir["i0"] + 2})
    step(0x1208AD, st=[(a("i0"), r(8))], d={"i0": m.ir["i0"] + 2})
    return m


IN_ADDR, CTX, OUT_ADDR, BUF_A, BUF_B, TABLE = 0xB1000, 0xB2040, 0xB3000, 0xB4000, 0xB5000, 0xB6200
#: DAG registers the pass leaves for the routine: `M0 = 0`, `M2 = 2` (one frame step).
SETS = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}


def _packed(rng) -> int:
    """A word of two packed 16-bit floats (`FPACK` of random values)."""

    def one() -> int:
        v = math.exp(rng.uniform(math.log(1e-3), math.log(2.0))) * rng.choice([-1, 1])
        return fpack(fb.bits(v))

    return one() | (one() << 16)


def random_scenario(rng) -> dict[int, int]:
    """Context, frame, output buffer and the two packed delay memories of one call."""

    def fl_(lo: float, hi: float) -> int:
        return fb.bits(math.exp(rng.uniform(math.log(lo), math.log(hi))))

    mem = {a: fl_(0.05, 1.0) for a in range(CTX - 0x20, CTX + 0x26)}
    for a in range(IN_ADDR, IN_ADDR + 4):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    for a in range(OUT_ADDR, OUT_ADDR + 4):
        mem[a] = fb.bits(rng.uniform(-1, 1))
    m4, m5 = rng.randrange(0, 6), rng.randrange(0, 6)
    off, shift = rng.randrange(0, 16), rng.choice([0, 16, 0, 1])
    scale = rng.choice([22, 22, 22, 20, 24])
    mem.update({
        CTX + 6: TABLE, CTX + 0xF: OUT_ADDR, CTX + 0x10: m4, CTX + 0x11: m5, CTX + 0x12: off,
        CTX + 0x13: shift,
        CTX - 0x1E: scale,
        CTX - 0x1D: (-scale) & M32,
        CTX - 0x1C: (scale << 6) | rng.choice([0, 0, 1]),
        CTX - 0xC: fb.bits(rng.choice([300.0, 400.0, 100.0, 5.0])),
        CTX + 0xE: fb.bits(rng.choice([0.9908, 0.9908, 0.5, 0.0])),
        CTX + 0x24: fb.bits(rng.choice([1.0, 0.8])),
        CTX + 0x25: fb.bits(rng.choice([0.01, 0.1])),
        0xC03D4: BUF_A, 0xC03D5: BUF_B, 0xC03D6: rng.randrange(0, 1 << 30),
    })  # fmt: skip
    for a in range(BUF_A, BUF_A + 0x20):
        mem[a] = _packed(rng)
    for a in range(BUF_B, BUF_B + 0x60):
        mem[a] = _packed(rng) if rng.random() < 0.7 else rng.randrange(0, 1 << 32)
    return mem


def table(rng) -> dict[int, int]:
    """Interpolation table read through `I10` (offsets `-512 .. 511`, pairs)."""
    return {TABLE - 512 + k: fb.bits(rng.uniform(-1.0, 1.0)) for k in range(1026)}


def routine():
    """The harness description of this routine (`tools/fx_oracle.py`)."""
    import random

    import fx_oracle as fo

    return fo.Routine(
        "delay",
        PC,
        END,
        SETS,
        lambda mem, trace=None: delay_0x1206b0(mem, IN_ADDR, CTX, trace),
        random_scenario,
        table(random.Random(9)),
    )
