#!/usr/bin/env python3
"""Length/type classifier and tiler for ADSP-214xx VISA instructions (best effort, ASSUMED).

Source: SHARC Processor Programming Reference rev 2.4, chapter 10 (`.reference/` only):
VISA instructions are MSB aligned and decoded from bit 47; a trailing `a`/`b`/`c` in the type name
is a 48/32/16-bit instruction. 32-bit forms carry `0111111` in bits 22..16, i.e. the low 7 bits of
their second 16-bit parcel are `0x3F`.

This module only recovers instruction boundaries and type names, not operands, so it can tell
whether a byte stream *tiles* into plausible instructions. Parcels are read little-endian
(firmware layout of `bank0_CODE` 0x440000..0x4FFFFF, see docs/research/dsp-image-hunt.md 5.5).

    python tools/sharc_visa.py target/fw_unpacked/bank0_CODE.bin 0x45d5ec 0x45d630
    python tools/sharc_visa.py target/fw_unpacked/bank0_CODE.bin 0x440000 0x530000 --sweep
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path


def classify(p0: int, p1: int | None) -> tuple[str, int]:
    """Return ``(type name, length in 16-bit parcels)`` for the instruction starting at ``p0``."""
    marker = p1 is not None and (p1 & 0x7F) == 0x3F
    top3, top4, sub = p0 >> 13, p0 >> 12, (p0 >> 2) & 7
    if top4 == 0b1100:
        return "2c", 1
    if top4 == 0b1001:
        if sub & 4:
            return "3c", 1
        return {2: ("15b", 2), 1: ("16b", 2)}.get(sub, ("16a", 3))
    if top3 == 0b001:
        return ("1b", 2) if marker else ("1a", 3)
    if top3 == 0b010:
        return ("3b", 2) if marker else ("3a", 3)
    if top3 == 0b011:
        kind = "5" if (p0 >> 12) & 1 else "4"
        return (kind + "b", 2) if marker else (kind + "a", 3)
    if top3 == 0b100:
        return "6a", 3
    if top3 == 0b101:
        return "15a", 3
    if top3 >= 0b110:
        return "10a", 3
    if p0 >> 8 in (0x18, 0x19):
        # RFRAME has a 16-bit form (`1901`, ADI); the 48-bit form is `1900 0000 0000`
        return "25", 1 if p0 == 0x1901 else 3
    code = (p0 >> 8) & 0x1F
    short = (p0 >> 7) & 1
    table = {
        0x01: ("2b", 2) if short else ("2a", 3),
        0x02: ("6n", 3),
        0x04: ("7b", 2) if marker else ("7a", 3),
        0x06: ("8a", 3),
        0x07: ("8a", 3),
        0x08: ("9b", 2) if marker else ("9a", 3),
        0x09: ("9b", 2) if marker else ("9a", 3),
        0x0A: ("11c", 1) if short else ("11a", 3),
        0x0B: ("11c", 1) if short else ("11a", 3),
        0x0C: ("12a", 3),
        0x0D: ("12a", 3),
        0x0E: ("13a", 3),
        0x0F: ("17b", 2) if short else ("17a", 3),
        0x14: ("18a", 3),
        0x16: ("19a", 3),
        0x17: ("20a", 3),
    }
    if code in table:
        return table[code]
    if 0x10 <= code <= 0x13:
        return "14a", 3
    if code == 0:
        return ("21c", 1) if p0 & 1 else ("21/22", 3)  # 16-bit NOP/IDLE forms end in a 1 bit
    return "??", 3


def tile(parcels: tuple[int, ...] | list[int]) -> tuple[list[tuple[str, tuple[int, ...]]], bool]:
    """Split parcels into instructions; the flag says the last one ends exactly at the end."""
    out: list[tuple[str, tuple[int, ...]]] = []
    i = 0
    while i < len(parcels):
        nxt = parcels[i + 1] if i + 1 < len(parcels) else None
        name, length = classify(parcels[i], nxt)
        out.append((name, tuple(parcels[i : i + length])))
        i += length
    return out, i == len(parcels)


def parcels_le(data: bytes) -> tuple[int, ...]:
    """Little-endian 16-bit parcels of ``data`` (odd trailing byte dropped)."""
    return struct.unpack(f"<{len(data) // 2}H", data[: len(data) & ~1])


def ureg_load(parcels: tuple[int, ...]) -> tuple[int, int] | None:
    """For a Type 17a/17b instruction return ``(ureg code, immediate)``, else None."""
    name, _ = classify(parcels[0], parcels[1] if len(parcels) > 1 else None)
    if name == "17b" and len(parcels) >= 2:
        return parcels[0] & 0x7F, parcels[1]
    if name == "17a" and len(parcels) >= 3:
        return parcels[0] & 0x7F, (parcels[1] << 16) | parcels[2]
    return None


COND = (
    "EQ LT LE AC AV MV MS SV SZ FLAG0_IN FLAG1_IN FLAG2_IN FLAG3_IN TF BM LCE "
    "NE GE GT NOT_AC NOT_AV NOT_MV NOT_MS NOT_SV NOT_SZ NOT_FLAG0_IN NOT_FLAG1_IN "
    "NOT_FLAG2_IN NOT_FLAG3_IN NOT_TF NOT_BM TRUE"
).split()
# Table 10-2 (processing element X), groups 6 and 7 (bits 6..4 of the 7-bit UREG code).
_SYS6 = (
    "FADDR DADDR ? PC PCSTK PCSTKP LADDR CURLCNTR LCNTR EMUCLK EMUCLK2 PX PX1 PX2 TPERIOD TCOUNT"
)
_SYS7 = (
    "USTAT1 USTAT2 MODE1 MMASK MODE2 FLAGS ASTATx ASTATy STKYx STKYy IRPTL IMASK IMASKP LIRPTL "
    "USTAT3 USTAT4"
)
_UREG_GROUPS = ["R", "I", "M", "L", "B", "S"]
BOP = {0: "SET", 1: "CLR", 2: "TGL", 4: "TST", 5: "XOR"}
# Tables 12-3 and 12-4 (single-function ALU); key = 8-bit opcode, CU = 000.
ALU_OPS = {
    0x01: "R{n} = R{x} + R{y}",
    0x02: "R{n} = R{x} - R{y}",
    0x05: "R{n} = R{x} + R{y} + CI",
    0x06: "R{n} = R{x} - R{y} + CI - 1",
    0x09: "R{n} = (R{x} + R{y})/2",
    0x0A: "COMP(R{x}, R{y})",
    0x0B: "COMPU(R{x}, R{y})",
    0x25: "R{n} = R{x} + CI",
    0x26: "R{n} = R{x} + CI - 1",
    0x29: "R{n} = R{x} + 1",
    0x2A: "R{n} = R{x} - 1",
    0x22: "R{n} = -R{x}",
    0x30: "R{n} = ABS R{x}",
    0x21: "R{n} = PASS R{x}",
    0x40: "R{n} = R{x} AND R{y}",
    0x41: "R{n} = R{x} OR R{y}",
    0x42: "R{n} = R{x} XOR R{y}",
    0x43: "R{n} = NOT R{x}",
    0x61: "R{n} = MIN(R{x}, R{y})",
    0x62: "R{n} = MAX(R{x}, R{y})",
    0x63: "R{n} = CLIP R{x} BY R{y}",
    0x81: "F{n} = F{x} + F{y}",
    0x82: "F{n} = F{x} - F{y}",
    0x91: "F{n} = ABS (F{x} + F{y})",
    0x92: "F{n} = ABS (F{x} - F{y})",
    0x89: "F{n} = (F{x} + F{y})/2",
    0x8A: "COMP(F{x}, F{y})",
    0xA2: "F{n} = -F{x}",
    0xB0: "F{n} = ABS F{x}",
    0xA1: "F{n} = PASS F{x}",
    0xA5: "F{n} = RND F{x}",
    0xBD: "F{n} = SCALB F{x} BY R{y}",
    0xAD: "R{n} = MANT F{x}",
    0xC1: "R{n} = LOGB F{x}",
    0xD9: "R{n} = FIX F{x} BY R{y}",
    0xC9: "R{n} = FIX F{x}",
    0xDD: "R{n} = TRUNC F{x} BY R{y}",
    0xCD: "R{n} = TRUNC F{x}",
    0xDA: "F{n} = FLOAT R{x} BY R{y}",
    0xCA: "F{n} = FLOAT R{x}",
    0xC4: "F{n} = RECIPS F{x}",
    0xC5: "F{n} = RSQRTS F{x}",
    0xE0: "F{n} = F{x} COPYSIGN F{y}",
    0xE1: "F{n} = MIN(F{x}, F{y})",
    0xE2: "F{n} = MAX(F{x}, F{y})",
    0xE3: "F{n} = CLIP F{x} BY F{y}",
}


def ureg_name(code: int) -> str:
    """Name of a 7-bit universal register code (Table 10-2, processing element X)."""
    group, num = (code >> 4) & 7, code & 15
    if group < 6:
        return f"{_UREG_GROUPS[group]}{num}"
    return (_SYS6 if group == 6 else _SYS7).split()[num]


def word48(parcels: tuple[int, ...]) -> int:
    """MSB-aligned 48-bit value of an instruction (shorter ones are left-aligned)."""
    w = 0
    for p in parcels:
        w = (w << 16) | p
    return w << (48 - 16 * len(parcels))


def bits(w: int, hi: int, lo: int) -> int:
    """Bits ``hi..lo`` of a 48-bit instruction word."""
    return (w >> lo) & ((1 << (hi - lo + 1)) - 1)


SHORT_OPS = [
    "R{n} = R{n} + R{x}",
    "R{n} = R{n} - R{x}",
    "R{n} = PASS R{x}",
    "COMP(R{n}, R{x})",
    "R{n} = NOT R{x}",
    "R{n} = R{x} + 1",
    "R{n} = R{x} - 1",
    "R{n} = R{n} * R{x} (SSI)",
    "F{n} = F{n} + F{x}",
    "F{n} = F{n} - F{x}",
    "F{n} = FLOAT R{x}",
    "COMP(F{n}, F{x})",
    "R{n} = R{n} AND R{x}",
    "R{n} = R{n} OR R{x}",
    "R{n} = R{n} XOR R{x}",
    "F{n} = F{n} * F{x}",
]


def shift_imm_text(w: int) -> str:
    """Text of a shift-immediate word (opcode 0x02, no data move).

    VERIFIED against ADI's elfdump on 834 instructions of the DSP images (docs 5.34): SHIFTOP bits
    21..16, DATA12 = bits 30..27 (high four) and 15..8 (low eight), Rn 7..4, Rx 3..0;
    bit6 = DATA12[5:0], len6 = DATA12[11:6].
    """
    op = bits(w, 21, 16)
    data = (bits(w, 30, 27) << 8) | bits(w, 15, 8)
    n, x = bits(w, 7, 4), bits(w, 3, 0)
    amount = data - 4096 if data & 0x800 else data
    field = f"{data & 63}:{data >> 6}"
    table = {
        0: f"R{n} = LSHIFT R{x} BY {amount}",
        1: f"R{n} = ASHIFT R{x} BY {amount}",
        2: f"R{n} = ROT R{x} BY {amount}",
        8: f"R{n} = R{n} OR LSHIFT R{x} BY {amount}",
        9: f"R{n} = R{n} OR ASHIFT R{x} BY {amount}",
        16: f"R{n} = FEXT R{x} BY {field}",
        17: f"R{n} = FDEP R{x} BY {field}",
        18: f"R{n} = FEXT R{x} BY {field} (SE)",
        19: f"R{n} = FDEP R{x} BY {field} (SE)",
        25: f"R{n} = R{n} OR FDEP R{x} BY {field}",
        27: f"R{n} = R{n} OR FDEP R{x} BY {field} (SE)",
        # bit FIFO immediates keep DATA12[5:0] (length) or DATA12[6:0] (pointer), as ADI prints them
        20: f"R{n} = BITEXT {data & 63}",
        22: f"R{n} = BITEXT {data & 63} (NU)",
        29: f"BITDEP R{x} BY {data & 63}",
        31: f"BFFWRP = {data & 127}",
        48: f"R{n} = BSET R{x} BY {data}",
        49: f"R{n} = BCLR R{x} BY {data}",
        50: f"R{n} = BTGL R{x} BY {data}",
        51: f"BTST R{x} BY {data}",
    }
    return table.get(op, f"SHIFTIMM?op={op} n={n} x={x} data={data}")


def short_compute_text(c: int) -> str:
    """Type 2c 12-bit short compute: OP in bits 11..8, Rn in 7..4, Rx in 3..0 (Table, ch. 12)."""
    return SHORT_OPS[bits(c, 11, 8)].format(n=bits(c, 7, 4), x=bits(c, 3, 0))


def mult_text(opcode: int, rn: int, rx: int, ry: int) -> str | None:
    """Multiplier opcodes of Tables 12-5 to 12-7 (fixed point) and `Fn = Fx * Fy`."""
    if opcode == 0x30:
        return f"F{rn} = F{rx} * F{ry}"
    cls, y, x = opcode >> 6, (opcode >> 5) & 1, (opcode >> 4) & 1
    f, sel, r = (opcode >> 3) & 1, (opcode >> 1) & 3, opcode & 1
    mod = f"({'S' if x else 'U'}{'S' if y else 'U'}{'F' if f else 'I'}{'R' if r else ''})"
    if cls == 0:
        if opcode >> 4 == 0:  # Rn|MRx = SAT MRx mod2 (0000 f s x)
            sat = {
                0: f"R{rn} = SAT MRF",
                1: f"R{rn} = SAT MRB",
                2: "MRF = SAT MRF",
                3: "MRB = SAT MRB",
            }
            return f"{sat[sel]} ({'U' if opcode & 1 else 'S'}{'F' if f else 'I'})"
        if opcode in (0x14, 0x16):
            return "MRF = 0" if opcode == 0x14 else "MRB = 0"
        if opcode >> 4 == 1 and (opcode >> 3) & 1:
            sel = (opcode >> 1) & 3
            reg = {
                0: f"R{rn} = RND MRF",
                1: f"R{rn} = RND MRB",
                2: "MRF = RND MRF",
                3: "MRB = RND MRB",
            }[sel]
            return f"{reg} ({'U' if opcode & 1 else 'S'}F)"
        return None
    sign = {1: "", 2: "+ ", 3: "- "}[cls]
    reg = {0: "R{n}", 1: "R{n}", 2: "MRF", 3: "MRB"}[sel].format(n=rn)
    acc = ""
    if cls > 1:
        acc = ("MRB " if sel in (1, 3) else "MRF ") + sign.strip() + " "
    return f"{reg} = {acc}R{rx} * R{ry} {mod}"


SHIFT_OPS = {
    0x00: "R{n} = LSHIFT R{x} BY R{y}",
    0x20: "R{n} = R{n} OR LSHIFT R{x} BY R{y}",
    0x04: "R{n} = ASHIFT R{x} BY R{y}",
    0x24: "R{n} = R{n} OR ASHIFT R{x} BY R{y}",
    0x08: "R{n} = ROT R{x} BY R{y}",
    0xC4: "R{n} = BCLR R{x} BY R{y}",
    0xC0: "R{n} = BSET R{x} BY R{y}",
    0xC8: "R{n} = BTGL R{x} BY R{y}",
    0xCC: "BTST R{x} BY R{y}",
    0x44: "R{n} = FDEP R{x} BY R{y}",
    0x4C: "R{n} = FDEP R{x} BY R{y} (SE)",
    0x64: "R{n} = R{n} OR FDEP R{x} BY R{y}",
    0x6C: "R{n} = R{n} OR FDEP R{x} BY R{y} (SE)",
    0x40: "R{n} = FEXT R{x} BY R{y}",
    0x48: "R{n} = FEXT R{x} BY R{y} (SE)",
    0x80: "R{n} = EXP R{x}",
    0x84: "R{n} = EXP R{x} (EX)",
    0x88: "R{n} = LEFTZ R{x}",
    0x8C: "R{n} = LEFTO R{x}",
    0x90: "R{n} = FPACK F{x}",
    0x94: "F{n} = FUNPACK R{x}",
    0x74: "BITDEP R{x} BY R{y}",
    # bit FIFO, VERIFIED against ADI elfdump (docs/research/sharc-bitfifo.md): operand is Ry
    0x50: "R{n} = BITEXT R{y}",
    0x58: "R{n} = BITEXT R{y} (NU)",
    0x70: "R{n} = BFFWRP",
    0x7C: "BFFWRP = R{y}",
}
# Table 12-12, multifunction multiplier + ALU (opcode = compute bits 21..16).
MULALU_OPS = {
    0b000100: "Rm = Rxm * Rym (SSFR), Ra = Rxa + Rya",
    0b000101: "Rm = Rxm * Rym (SSFR), Ra = Rxa - Rya",
    0b000110: "Rm = Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
    0b001000: "MRF = MRF + Rxm * Rym (SSF), Ra = Rxa + Rya",
    0b001001: "MRF = MRF + Rxm * Rym (SSF), Ra = Rxa - Rya",
    0b001010: "MRF = MRF + Rxm * Rym (SSF), Ra = (Rxa + Rya)/2",
    0b001100: "Rm = MRF + Rxm * Rym (SSFR), Ra = Rxa + Rya",
    0b001101: "Rm = MRF + Rxm * Rym (SSFR), Ra = Rxa - Rya",
    0b001110: "Rm = MRF + Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
    0b010000: "MRF = MRF - Rxm * Rym (SSF), Ra = Rxa + Rya",
    0b010001: "MRF = MRF - Rxm * Rym (SSF), Ra = Rxa - Rya",
    0b010010: "MRF = MRF - Rxm * Rym (SSF), Ra = (Rxa + Rya)/2",
    0b010100: "Rm = MRF - Rxm * Rym (SSFR), Ra = Rxa + Rya",
    0b010101: "Rm = MRF - Rxm * Rym (SSFR), Ra = Rxa - Rya",
    0b010110: "Rm = MRF - Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
    0b011000: "Fm = Fxm * Fym, Fa = Fxa + Fya",
    0b011001: "Fm = Fxm * Fym, Fa = Fxa - Fya",
    0b011010: "Fm = Fxm * Fym, Fa = FLOAT Rxa BY Rya",
    0b011011: "Fm = Fxm * Fym, Ra = FIX Fxa BY Rya",
    0b011100: "Fm = Fxm * Fym, Fa = (Fxa + Fya)/2",
    0b011101: "Fm = Fxm * Fym, Fa = ABS Fxa",
    0b011110: "Fm = Fxm * Fym, Fa = MAX (Fxa, Fya)",
    0b011111: "Fm = Fxm * Fym, Fa = MIN (Fxa, Fya)",
}


def mr_transfer_text(op: int, n: int) -> str | None:
    """`Rn = MR1F` / `MR1F = Rn` (operation code bits 19..12, register bits 11..8), docs 5.36."""
    if op & 3 == 3 or op & ~0x17:
        return None
    mr = f"MR{op & 3}{'B' if op & 4 else 'F'}"
    return f"{mr} = R{n}" if op & 16 else f"R{n} = {mr}"


def multi_text(c: int) -> str | None:
    """Multifunction compute fields (bit 22 set): multiplier + ALU, multiplier + dual ALU."""
    if not bits(c, 22, 22):
        return None
    if bits(c, 21, 17) == 0:
        return mr_transfer_text(bits(c, 19, 12), bits(c, 11, 8))
    xm, ym = bits(c, 7, 6), 4 + bits(c, 5, 4)
    xa, ya = 8 + bits(c, 3, 2), 12 + bits(c, 1, 0)
    rm, ra = bits(c, 15, 12), bits(c, 11, 8)
    if bits(c, 21, 21):  # 1 1x: multiplier and dual ALU
        k = "F" if bits(c, 20, 20) else "R"
        rs = bits(c, 19, 16)
        return (
            f"{k}{rm} = {k}{xm} * {k}{ym}, {k}{ra} = {k}{xa} + {k}{ya}, {k}{rs} = {k}{xa} - {k}{ya}"
        )
    text = MULALU_OPS.get(bits(c, 21, 16))
    if text is None:
        return None
    fl = text.startswith("Fm")
    k = "F" if fl else "R"
    out = text.replace("Rxm", f"{k}{xm}").replace("Fxm", f"F{xm}")
    out = out.replace("Rym", f"{k}{ym}").replace("Fym", f"F{ym}")
    out = out.replace("Rxa", f"{k}{xa}").replace("Fxa", f"F{xa}")
    out = out.replace("Rya", f"{k}{ya}").replace("Fya", f"F{ya}")
    out = out.replace("Rm", f"R{rm}").replace("Fm", f"F{rm}")
    return out.replace("Ra =", f"R{ra} =").replace("Fa =", f"F{ra} =")


def compute_text(c: int) -> str:
    """Text for a 23-bit compute field (ALU, multiplier and dual ALU decoded; else raw)."""
    if c == 0:
        return "NOP"
    cu, opcode = bits(c, 22, 20), bits(c, 19, 12)
    rn, rx, ry = bits(c, 11, 8), bits(c, 7, 4), bits(c, 3, 0)
    if cu == 0 and opcode in ALU_OPS:
        return ALU_OPS[opcode].format(n=rn, x=rx, y=ry)
    if cu == 0 and bits(c, 19, 16) in (0x7, 0xF):
        k = "F" if bits(c, 19, 16) == 0xF else "R"
        return f"{k}{bits(c, 15, 12)} = {k}{rx} - {k}{ry}, {k}{rn} = {k}{rx} + {k}{ry}"
    if cu == 1:
        text = mult_text(opcode, rn, rx, ry)
        if text:
            return text
    if cu == 2 and opcode in SHIFT_OPS:
        return SHIFT_OPS[opcode].format(n=rn, x=rx, y=ry)
    if bits(c, 22, 22):
        text = multi_text(c)
        if text:
            return text
    unit = {0: "ALU", 1: "MUL", 2: "SHIFT"}.get(cu, "MULTI")
    return f"{unit}?op={opcode:02x} n={rn} x={rx} y={ry}"


def _mem_move(w: int, name: str, iff: str) -> str:
    """Types 3a/3b (Ureg <-> DM/PM(Im, Mn)) and 4a/4b (Dreg <-> DM/PM(data6, Ii))."""
    tail = f", {compute_text(bits(w, 22, 0))}" if name.endswith("a") and bits(w, 22, 0) else ""
    if name.startswith("3"):
        pm = bits(w, 32, 32)
        base = 8 if pm else 0
        i, m = bits(w, 43, 41) + base, bits(w, 40, 38) + base
        ea = f"{'PM' if pm else 'DM'}(I{i}, M{m}){'' if bits(w, 44, 44) else ' [pre-mod]'}"
        reg = ureg_name(bits(w, 29, 23))
        write = bits(w, 31, 31)
    else:
        pm = bits(w, 40, 40)
        i = bits(w, 43, 41) + (8 if pm else 0)
        data = bits(w, 32, 27)
        data -= 64 if data & 32 else 0  # DATA6 is signed (VERIFIED against ADI elfdump, docs 5.34)
        # bit 38 = U: DM(I6, 2) post-modifies I6 by 2, DM(2, I6) is a pre-modify offset
        operands = f"I{i}, {data}" if bits(w, 38, 38) else f"{data}, I{i}"
        ea = f"{'PM' if pm else 'DM'}({operands})"
        reg = f"R{bits(w, 26, 23)}"
        write = bits(w, 39, 39)
    return f"{iff}{ea} = {reg}{tail}" if write else f"{iff}{reg} = {ea}{tail}"


# Instruction forms whose type is known from the diagrams but whose operand fields are not decoded.
TYPE_ONLY = {
    "10a": "jump + dual move + compute",
}


def _dual_half(space: str, write: int, i: int, m: int, reg: int) -> str:
    if write:
        return f"{space}(I{i}, M{m}) = R{reg}"
    return f"R{reg} = {space}(I{i}, M{m})"


def _modifiers(flags: list[tuple[str, int]]) -> str:
    """` (DB,LA,CI)` as ADI prints the set modifiers, or nothing."""
    on = [name for name, bit in flags if bit]
    return f" ({','.join(on)})" if on else ""


def _branch_modifiers(w: int) -> str:
    """Types 8a/9a/9b: DB bit 26, LA bit 38 (jumps only), CI bit 24 (ADI `(la,ci)`)."""
    jump = not bits(w, 39, 39)
    return _modifiers(
        [("DB", bits(w, 26, 26)), ("LA", jump and bits(w, 38, 38)), ("CI", bits(w, 24, 24))]
    )


def _return_modifiers(w: int, short: bool) -> str:
    """Types 11a/11c: 16-bit DB bit 38, LR bit 32 (`rts (db,lr)` is `0aff`); 48-bit DB bit 26,
    LR bit 24 (`0a3e 0400 0000` is `rts (db)`)."""
    db, lr = (38, 32) if short else (26, 24)
    return _modifiers([("DB", bits(w, db, db)), ("LR", bits(w, lr, lr))])


def _stack_text(w: int) -> str:
    """Type 20a in ADI order; a push wins over a pop of the same stack."""
    parts = []
    for push, pop, name in ((39, 38, "LOOP"), (37, 36, "STS"), (35, 34, "PCSTK")):
        if bits(w, push, push):
            parts.append(f"PUSH {name}")
        elif bits(w, pop, pop):
            parts.append(f"POP {name}")
    if bits(w, 33, 33):
        parts.append("FLUSH CACHE")
    return ", ".join(parts)


def _type25_text(w: int) -> str:
    """RFRAME (`1901` / `1900 0000 0000`) or CJUMP; ADI prints no space before `(db)`."""
    if bits(w, 47, 41) == 0x0C and bits(w, 40, 40):
        return "RFRAME"
    db = "(DB)" if bits(w, 34, 34) else ""
    if bits(w, 38, 38):
        return f"CJUMP (PC, 0x{bits(w, 23, 0):x}){db}"
    return f"CJUMP 0x{bits(w, 23, 0):x}{db}"


def decode(parcels: tuple[int, ...]) -> str:
    """Disassembly text for one instruction (best effort; unknown forms stay raw)."""
    name, _ = classify(parcels[0], parcels[1] if len(parcels) > 1 else None)
    w = word48(parcels)
    cond = COND[bits(w, 37, 33)]
    iff = "" if cond == "TRUE" else f"IF {cond} "
    if name in ("3a", "3b", "4a", "4b"):
        return _mem_move(w, name, iff)
    if name in ("5a", "5b"):
        # VERIFIED against ADI elfdump (docs 5.34): source bits 42..38 and 32..31, destination
        # bits 29..23; swap "Rx <-> Sy" with Rx bits 26..23 and Sy bits 41..38.
        tail = f", {compute_text(bits(w, 22, 0))}" if name == "5a" and bits(w, 22, 0) else ""
        if bits(w, 43, 43):
            return f"{iff}R{bits(w, 26, 23)} <-> S{bits(w, 41, 38)}{tail}"
        src = (bits(w, 42, 38) << 2) | bits(w, 32, 31)
        return f"{iff}{ureg_name(bits(w, 29, 23))} = {ureg_name(src)}{tail}"
    if name in ("14a", "15a"):
        pm = bits(w, 41, 41) if name == "14a" else bits(w, 44, 44)
        mem = "PM" if pm else "DM"
        write = bits(w, 40, 40)
        u = ureg_name(bits(w, 38, 32))
        if name == "14a":
            ea = f"{mem}(0x{bits(w, 31, 0):x})"
        else:
            ea = f"{mem}(0x{bits(w, 31, 0):x}, I{bits(w, 43, 41) + (8 if pm else 0)})"
        return f"{ea} = {u}" if write else f"{u} = {ea}"
    if name == "15b":
        # Fields: I 43..41, D 40, UREG 29..23, signed DATA7 22..16. VERIFIED on the ORG image
        # (docs/research/dsp-image-hunt.md 5.28): 99% of UREG codes are R or I registers and the
        # register is used again within six instructions in 88% of the loads; the earlier
        # swapped split put 79% of the transfers into system registers.
        data = bits(w, 22, 16)
        data -= 128 if data & 0x40 else 0
        # G bit 37: program memory with the DAG2 index (ADI `i5=pm(0x8,i9)`), 1,400 samples
        space, bank = ("PM", 8) if bits(w, 37, 37) else ("DM", 0)
        ea = f"{space}({data}, I{bits(w, 43, 41) + bank})"
        u = ureg_name(bits(w, 29, 23))
        return f"{ea} = {u}" if bits(w, 40, 40) else f"{u} = {ea}"
    if name == "3c":
        ea = f"DM(I{bits(w, 43, 41)}, M{bits(w, 40, 38)})"
        reg = f"R{bits(w, 35, 32)}"
        return f"{ea} = {reg}" if bits(w, 37, 37) else f"{reg} = {ea}"
    if name == "2b":
        return compute_text(bits(w, 38, 16))
    if name == "2c":
        return short_compute_text(bits(w, 43, 32))
    if name in ("1a", "1b"):
        # layout VERIFIED against ADI elfdump on 587 instructions (docs 5.34)
        dm = _dual_half("DM", bits(w, 44, 44), bits(w, 43, 41), bits(w, 40, 38), bits(w, 36, 33))
        pm = _dual_half(
            "PM", bits(w, 37, 37), bits(w, 32, 30) + 8, bits(w, 29, 27) + 8, bits(w, 26, 23)
        )
        if name == "1a" and bits(w, 22, 0):
            return f"{dm}, {pm}, {compute_text(bits(w, 22, 0))}"
        return f"{dm}, {pm}"
    if name in ("16a", "16b"):
        bank = 8 if bits(w, 37, 37) else 0
        space = "PM" if bank else "DM"
        if name == "16a":
            value = bits(w, 31, 0)
        else:
            value = bits(w, 31, 16)
            value = (value - 0x10000 if value & 0x8000 else value) & 0xFFFFFFFF
        # no condition field in this type
        return f"{space}(I{bits(w, 43, 41) + bank}, M{bits(w, 40, 38) + bank}) = 0x{value:x}"
    if name in ("7a", "7b"):
        bank = 8 if bits(w, 38, 38) else 0
        src = bits(w, 32, 30) + bank
        dst = src ^ bits(w, 26, 24)
        text = f"{iff}I{dst} = MODIFY(I{src}, M{bits(w, 29, 27) + bank})"
        if name == "7a" and bits(w, 22, 0):
            text += f", {compute_text(bits(w, 22, 0))}"
        return text
    if name == "6a":
        # shift fields of the 0x02 form plus a post-modify data move (docs 5.36)
        bank = 8 if bits(w, 32, 32) else 0
        move = _dual_half(
            "PM" if bank else "DM",
            bits(w, 31, 31),
            bits(w, 43, 41) + bank,
            bits(w, 40, 38) + bank,
            bits(w, 26, 23),
        )
        return f"{iff}{shift_imm_text(w)}, {move}"
    if name == "6n":
        return f"{iff}{shift_imm_text(w)}"
    if name == "6a":
        # shift immediate + DM/PM move (VERIFIED on 10 ADI samples of the SMP/SY1SY2 images):
        # post-modify only, PM bit 32, write bit 31, data register bits 26..23
        bank = 8 if bits(w, 32, 32) else 0
        ea = f"{'PM' if bank else 'DM'}(I{bits(w, 43, 41) + bank}, M{bits(w, 40, 38) + bank})"
        reg = f"R{bits(w, 26, 23)}"
        move = f"{ea} = {reg}" if bits(w, 31, 31) else f"{reg} = {ea}"
        return f"{iff}{shift_imm_text(w)}, {move}"
    if name == "19a":
        g = 8 if bits(w, 38, 38) else 0
        src = bits(w, 34, 32) + g
        dst = (bits(w, 37, 35) ^ bits(w, 34, 32)) + g
        op = "BITREV" if bits(w, 39, 39) else "MODIFY"
        return f"I{dst} = {op}(I{src}, 0x{bits(w, 31, 0):x})"
    if name in TYPE_ONLY:
        return f"{TYPE_ONLY[name]} (operands raw) " + " ".join(f"{x:04x}" for x in parcels)
    if (name == "21c" and parcels[0] == 0x00FF) or (name == "21/22" and w == 0x0080_0000_0000):
        return "IDLE"
    if name == "20a":
        return _stack_text(w)
    if name == "25":
        return _type25_text(w)
    if name == "21c" or (name == "21/22" and w == 0):
        return "NOP"
    if name == "17a":
        return f"{ureg_name(bits(w, 38, 32))} = 0x{bits(w, 31, 0):x}"
    if name == "17b":
        value = bits(w, 31, 16)
        value = (value - 0x10000 if value & 0x8000 else value) & 0xFFFFFFFF  # sign extended
        return f"{ureg_name(bits(w, 38, 32))} = 0x{value:x}"
    if name == "18a":
        sreg = ureg_name(0x70 | bits(w, 35, 32))
        return f"BIT {BOP.get(bits(w, 39, 37), '?')} {sreg} 0x{bits(w, 31, 0):x}"
    if name == "8a":
        kind = "CALL" if bits(w, 39, 39) else "JUMP"
        if bits(w, 40, 40):  # ADI prints `(pc, 0)` or a 32-bit hex offset
            off = (bits(w, 23, 0) ^ 0x800000) - 0x800000 & 0xFFFFFFFF
            target = "(PC, 0)" if off == 0 else f"(PC, 0x{off:x})"
        else:
            target = f"0x{bits(w, 23, 0):x}"
        tag = _branch_modifiers(w)
        return f"{iff}{kind} {target}{tag}"
    if name == "2a":
        return f"{iff}{compute_text(bits(w, 22, 0))}"
    if name in ("11a", "11c"):
        delayed = _return_modifiers(w, name == "11c")
        tail = f", {compute_text(bits(w, 22, 0))}" if name == "11a" and bits(w, 22, 0) else ""
        return f"{iff}{'RTI' if bits(w, 40, 40) else 'RTS'}{delayed}{tail}"
    if name in ("9a", "9b"):
        # delayed flag bit 26, call bit 39, indirect target Ic + Md on DAG2 (docs 5.34)
        op = "CALL" if bits(w, 39, 39) else "JUMP"
        delayed = _branch_modifiers(w)
        if bits(w, 40, 40):
            rel = bits(w, 32, 27)
            target = f"(PC, {rel - 64 if rel & 32 else rel})"
        else:
            target = f"(M{bits(w, 29, 27) + 8}, I{bits(w, 32, 30) + 8})"
        tail = f", {compute_text(bits(w, 22, 0))}" if name == "9a" and bits(w, 22, 0) else ""
        return f"{iff}{op} {target}{delayed}{tail}"
    if name == "12a":
        # opcode 0x0D (bit 40) counts with a universal register, 0x0C with a 16-bit immediate
        count = ureg_name(bits(w, 38, 32)) if bits(w, 40, 40) else bits(w, 39, 24)
        offset = bits(w, 22, 0)
        offset -= 1 << 23 if offset & (1 << 22) else 0
        return f"DO (PC, {offset}) UNTIL LCE, LCNTR = {count}"
    if name == "13a":
        offset = bits(w, 22, 0)
        offset -= 1 << 23 if offset & (1 << 22) else 0
        term = "FOREVER" if bits(w, 37, 33) == 31 else COND[bits(w, 37, 33)]
        return f"DO (PC, {offset}) UNTIL {term}"
    return f"{name}? raw " + " ".join(f"{x:04x}" for x in parcels)


def sweep(
    data: bytes, start: int, end: int, window: int = 0x400, min_hits: int = 1
) -> list[tuple[int, int, int]]:
    """Find code-like spans as ``(start, end, hits)`` runs.

    A hit is a Type 17a load that puts a value in SHARC internal RAM (`0x80000..0xFFFFF`) into an
    index register `I0..I15`. ARM code and PCM-like data give 0 hits per window (checked on
    `bank0_CODE` of v1.64). Windows within `0x2000` of each other merge into one run.
    """
    runs: list[list[int]] = []
    for off in range(start, end, window):
        best = 0
        for phase in (0, 2):
            insns, _ = tile(parcels_le(data[off + phase : off + window]))
            n = 0
            for nm, ps in insns:
                r = ureg_load(ps)
                if nm == "17a" and r and (r[0] >> 4) == 1 and 0x80000 <= r[1] < 0x100000:
                    n += 1
            best = max(best, n)
        if best >= min_hits:
            if runs and off - runs[-1][1] <= 0x2000:
                runs[-1][1] = off + window
                runs[-1][2] += best
            else:
                runs.append([off, off + window, best])
    return [(a, b, c) for a, b, c in runs]


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("start", type=lambda s: int(s, 0))
    p.add_argument("end", type=lambda s: int(s, 0))
    p.add_argument("--sweep", action="store_true", help="list code-like spans in start..end")
    args = p.parse_args(argv)
    if args.sweep:
        full = args.image.read_bytes()
        for a, b, hits in sweep(full, args.start, min(args.end, len(full))):
            print(
                f"0x{a:06x}..0x{b:06x} size 0x{b - a:x} index-register loads in SHARC RAM: {hits}"
            )
        return 0
    data = args.image.read_bytes()[args.start : args.end]
    insns, exact = tile(parcels_le(data))
    off = args.start
    for name, ps in insns:
        raw = " ".join(f"{x:04x}" for x in ps)
        print(f"0x{off:06x} {name:5s} {raw:15s} {decode(ps)}")
        off += 2 * len(ps)
    print("tiles exactly" if exact else "does NOT tile exactly")
    return 0


if __name__ == "__main__":
    sys.exit(main())
