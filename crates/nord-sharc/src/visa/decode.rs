//! Instruction text: the operand decoding of every type and the listing line format.
use super::classify::{bits, word48};
use super::compute::{compute_text, shift_imm_text, short_compute_text};
use super::flow_text::{
    branch_modifiers, do_until_text, return_modifiers, stack_text, type25_text,
};
use super::regs::{ureg_name, COND};
use super::types::{Insn, Kind};
/// A `width`-bit two's-complement field as a signed number.
fn signed_field(v: u64, width: u32) -> i64 {
    let shift = 64 - width;
    ((v << shift) as i64) >> shift
}

fn bop(code: u64) -> &'static str {
    match code {
        0 => "SET",
        1 => "CLR",
        2 => "TGL",
        4 => "TST",
        5 => "XOR",
        _ => "?",
    }
}

pub(super) fn mem_move(w: u64, kind: Kind, iff: &str) -> String {
    let has_compute = matches!(kind, Kind::T3a | Kind::T4a) && bits(w, 22, 0) != 0;
    let tail = if has_compute {
        format!(", {}", compute_text(bits(w, 22, 0)))
    } else {
        String::new()
    };
    let (ea, reg, write) = if matches!(kind, Kind::T3a | Kind::T3b) {
        let pm = bits(w, 32, 32) == 1;
        let base = if pm { 8 } else { 0 };
        let i = bits(w, 43, 41) + base;
        let m = bits(w, 40, 38) + base;
        let pre = if bits(w, 44, 44) == 1 {
            ""
        } else {
            " [pre-mod]"
        };
        (
            format!("{}(I{i}, M{m}){pre}", if pm { "PM" } else { "DM" }),
            ureg_name(bits(w, 29, 23) as u8),
            bits(w, 31, 31) == 1,
        )
    } else {
        let pm = bits(w, 40, 40) == 1;
        let i = bits(w, 43, 41) + if pm { 8 } else { 0 };
        let data = signed_field(bits(w, 32, 27), 6);
        // bit 38 = U: `DM(I6, 2)` post-modifies I6 by 2, `DM(2, I6)` is a pre-modify offset
        let operands = if bits(w, 38, 38) == 1 {
            format!("I{i}, {data}")
        } else {
            format!("{data}, I{i}")
        };
        (
            format!("{}({operands})", if pm { "PM" } else { "DM" }),
            format!("R{}", bits(w, 26, 23)),
            bits(w, 39, 39) == 1,
        )
    };
    if write {
        format!("{iff}{ea} = {reg}{tail}")
    } else {
        format!("{iff}{reg} = {ea}{tail}")
    }
}

/// `DM(Ii, Mm) = Rn` or `Rn = DM(Ii, Mm)` for a dual-move half.
fn dual_half(space: &str, write: bool, i: u64, m: u64, reg: u64) -> String {
    if write {
        format!("{space}(I{i}, M{m}) = R{reg}")
    } else {
        format!("R{reg} = {space}(I{i}, M{m})")
    }
}

pub(super) fn type_only(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::T9a => "indirect/rel branch + compute",
        Kind::T9b => "indirect/rel branch",
        Kind::T10a => "jump + dual move + compute",
        _ => return None,
    })
}

/// A 24-bit two's complement field as a 32-bit value (how ADI prints relative offsets).
fn sign_extend_24(v: u64) -> u32 {
    (((v as u32) << 8) as i32 >> 8) as u32
}

pub(super) fn raw_hex(parcels: &[u16]) -> String {
    parcels
        .iter()
        .map(|p| format!("{p:04x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Disassembly text of one instruction (best effort; unknown forms stay raw).
pub fn decode_text(insn: &Insn) -> String {
    let parcels = insn.words();
    let kind = insn.kind;
    let w = word48(parcels);
    let cond = COND[bits(w, 37, 33) as usize];
    let iff = if cond == "TRUE" {
        String::new()
    } else {
        format!("IF {cond} ")
    };
    match kind {
        Kind::T3a | Kind::T3b | Kind::T4a | Kind::T4b => mem_move(w, kind, &iff),
        Kind::T5a | Kind::T5b => {
            // Fields VERIFIED against ADI elfdump on 1,242 (5b) and 1,748 (5a) register moves and
            // 1,087 swaps (docs 5.34): source (bits 42..38, 32..31), destination bits 29..23;
            // swap `Rx <-> Sy` with Rx bits 26..23 and Sy bits 41..38.
            let swap = bits(w, 43, 43) == 1;
            let tail = if kind == Kind::T5a && bits(w, 22, 0) != 0 {
                format!(", {}", compute_text(bits(w, 22, 0)))
            } else {
                String::new()
            };
            if swap {
                return format!("{iff}R{} <-> S{}{tail}", bits(w, 26, 23), bits(w, 41, 38));
            }
            let src = (bits(w, 42, 38) << 2) | bits(w, 32, 31);
            format!(
                "{iff}{} = {}{tail}",
                ureg_name(bits(w, 29, 23) as u8),
                ureg_name(src as u8)
            )
        }
        Kind::T14a | Kind::T15a => {
            let pm = if kind == Kind::T14a {
                bits(w, 41, 41)
            } else {
                bits(w, 44, 44)
            } == 1;
            let mem = if pm { "PM" } else { "DM" };
            let u = ureg_name(bits(w, 38, 32) as u8);
            let ea = if kind == Kind::T14a {
                format!("{mem}(0x{:x})", bits(w, 31, 0))
            } else {
                let i = bits(w, 43, 41) + if pm { 8 } else { 0 };
                format!("{mem}(0x{:x}, I{i})", bits(w, 31, 0))
            };
            if bits(w, 40, 40) == 1 {
                format!("{ea} = {u}")
            } else {
                format!("{u} = {ea}")
            }
        }
        Kind::T15b => {
            // I 43..41, D 40, UREG 29..23, signed DATA7 22..16 (VERIFIED, docs 5.28)
            let data = signed_field(bits(w, 22, 16), 7);
            // G bit 37: program memory with the DAG2 index (`pm(0x8,i9)`), VERIFIED on 1,400 samples
            let (space, bank) = if bits(w, 37, 37) == 1 {
                ("PM", 8)
            } else {
                ("DM", 0)
            };
            let ea = format!("{space}({data}, I{})", bits(w, 43, 41) + bank);
            let u = ureg_name(bits(w, 29, 23) as u8);
            if bits(w, 40, 40) == 1 {
                format!("{ea} = {u}")
            } else {
                format!("{u} = {ea}")
            }
        }
        Kind::T3c => {
            let ea = format!("DM(I{}, M{})", bits(w, 43, 41), bits(w, 40, 38));
            let reg = format!("R{}", bits(w, 35, 32));
            if bits(w, 37, 37) == 1 {
                format!("{ea} = {reg}")
            } else {
                format!("{reg} = {ea}")
            }
        }
        Kind::T2b => compute_text(bits(w, 38, 16)),
        Kind::T2c => short_compute_text(bits(w, 43, 32)),
        Kind::T1a | Kind::T1b => {
            // layout VERIFIED against ADI elfdump on 587 instructions (docs 5.34)
            let dm = dual_half(
                "DM",
                bits(w, 44, 44) == 1,
                bits(w, 43, 41),
                bits(w, 40, 38),
                bits(w, 36, 33),
            );
            let pm = dual_half(
                "PM",
                bits(w, 37, 37) == 1,
                bits(w, 32, 30) + 8,
                bits(w, 29, 27) + 8,
                bits(w, 26, 23),
            );
            if kind == Kind::T1a && bits(w, 22, 0) != 0 {
                format!("{dm}, {pm}, {}", compute_text(bits(w, 22, 0)))
            } else {
                format!("{dm}, {pm}")
            }
        }
        Kind::T16a | Kind::T16b => {
            let bank = if bits(w, 37, 37) == 1 { 8 } else { 0 };
            let space = if bank == 8 { "PM" } else { "DM" };
            let value = if kind == Kind::T16a {
                bits(w, 31, 0)
            } else {
                (bits(w, 31, 16) as u16 as i16 as i32 as u32) as u64
            };
            format!(
                "{space}(I{}, M{}) = 0x{value:x}",
                bits(w, 43, 41) + bank,
                bits(w, 40, 38) + bank
            )
        }
        Kind::T7a | Kind::T7b => {
            let bank = if bits(w, 38, 38) == 1 { 8 } else { 0 };
            let src = bits(w, 32, 30) + bank;
            let dst = src ^ bits(w, 26, 24);
            let modify = format!("I{dst} = MODIFY(I{src}, M{})", bits(w, 29, 27) + bank);
            if kind == Kind::T7a && bits(w, 22, 0) != 0 {
                format!("{iff}{modify}, {}", compute_text(bits(w, 22, 0)))
            } else {
                format!("{iff}{modify}")
            }
        }
        Kind::T6n => format!("{iff}{}", shift_imm_text(w)),
        Kind::T6a => {
            // shift fields of the 0x02 form plus a post-modify data move (docs 5.36)
            let bank = if bits(w, 32, 32) == 1 { 8 } else { 0 };
            let mv = dual_half(
                if bank == 8 { "PM" } else { "DM" },
                bits(w, 31, 31) == 1,
                bits(w, 43, 41) + bank,
                bits(w, 40, 38) + bank,
                bits(w, 26, 23),
            );
            format!("{iff}{}, {mv}", shift_imm_text(w))
        }
        Kind::T19a => {
            let g = if bits(w, 38, 38) == 1 { 8 } else { 0 };
            let src = bits(w, 34, 32) + g;
            let dst = (bits(w, 37, 35) ^ bits(w, 34, 32)) + g;
            let op = if bits(w, 39, 39) == 1 {
                "BITREV"
            } else {
                "MODIFY"
            };
            format!("I{dst} = {op}(I{src}, 0x{:x})", bits(w, 31, 0))
        }
        Kind::T17a => format!(
            "{} = 0x{:x}",
            ureg_name(bits(w, 38, 32) as u8),
            bits(w, 31, 0)
        ),
        Kind::T17b => format!(
            "{} = 0x{:x}",
            ureg_name(bits(w, 38, 32) as u8),
            bits(w, 31, 16) as u16 as i16 as i32 as u32 // sign extended, as ADI prints it
        ),
        Kind::T18a => {
            let sreg = ureg_name((0x70 | bits(w, 35, 32)) as u8);
            format!("BIT {} {sreg} 0x{:x}", bop(bits(w, 39, 37)), bits(w, 31, 0))
        }
        Kind::T8a => {
            let kind_s = if bits(w, 39, 39) == 1 { "CALL" } else { "JUMP" };
            // a PC-relative jump prints its offset as ADI does: `(PC, 0)`, else a 32-bit hex value
            let target = if bits(w, 40, 40) == 1 {
                let off = sign_extend_24(bits(w, 23, 0));
                if off == 0 {
                    "(PC, 0)".to_string()
                } else {
                    format!("(PC, 0x{off:x})")
                }
            } else {
                format!("0x{:x}", bits(w, 23, 0))
            };
            format!("{iff}{kind_s} {target}{}", branch_modifiers(w))
        }
        Kind::T2a => format!("{iff}{}", compute_text(bits(w, 22, 0))),
        Kind::T21c if parcels[0] == 0x00FF => "IDLE".to_string(),
        Kind::T21c => "NOP".to_string(),
        Kind::T21 if w == 0 => "NOP".to_string(),
        Kind::T21 if w == 0x0080_0000_0000 => "IDLE".to_string(),
        Kind::T20a => stack_text(w),
        Kind::T25 => type25_text(w),
        Kind::T11a | Kind::T11c => {
            let delayed = return_modifiers(w, kind == Kind::T11c);
            // Type 11a may carry a compute in parallel (`IF GT RTS, R2 = MRF - R3 * R14 (UUI)`)
            let tail = if kind == Kind::T11a && bits(w, 22, 0) != 0 {
                format!(", {}", compute_text(bits(w, 22, 0)))
            } else {
                String::new()
            };
            format!(
                "{iff}{}{delayed}{tail}",
                if bits(w, 40, 40) == 1 { "RTI" } else { "RTS" }
            )
        }
        Kind::T9a | Kind::T9b => {
            // delayed flag bit 26, call bit 39, indirect target Ic + Md on DAG2 (docs 5.34)
            let op = if bits(w, 39, 39) == 1 { "CALL" } else { "JUMP" };
            let delayed = branch_modifiers(w);
            let target = if bits(w, 40, 40) == 1 {
                format!("(PC, {})", signed_field(bits(w, 32, 27), 6))
            } else {
                format!("(M{}, I{})", bits(w, 29, 27) + 8, bits(w, 32, 30) + 8)
            };
            if kind == Kind::T9a && bits(w, 22, 0) != 0 {
                format!(
                    "{iff}{op} {target}{delayed}, {}",
                    compute_text(bits(w, 22, 0))
                )
            } else {
                format!("{iff}{op} {target}{delayed}")
            }
        }
        Kind::T12a => {
            // opcode 0x0D (bit 40) counts with a universal register, 0x0C with a 16-bit immediate
            let count = if bits(w, 40, 40) == 1 {
                ureg_name(bits(w, 38, 32) as u8)
            } else {
                bits(w, 39, 24).to_string()
            };
            let offset = signed_field(bits(w, 22, 0), 23);
            format!("DO (PC, {offset}) UNTIL LCE, LCNTR = {count}")
        }
        Kind::T13a => do_until_text(w),
        _ => {
            if let Some(t) = type_only(kind) {
                format!("{t} (operands raw) {}", raw_hex(parcels))
            } else {
                format!("{}? raw {}", kind.name(), raw_hex(parcels))
            }
        }
    }
}

/// One disassembly line in the format of `tools/sharc_visa.py`.
pub fn listing_line(offset: usize, insn: &Insn) -> String {
    format!(
        "0x{offset:06x} {:<5} {:<15} {}",
        insn.kind.name(),
        raw_hex(insn.words()),
        decode_text(insn)
    )
}

#[cfg(test)]
mod tests {
    use super::super::classify::{classify, parcels_le, tile};
    use super::*;

    fn dec(p: &[u16]) -> String {
        let (k, _) = classify(p[0], p.get(1).copied());
        let mut parcels = [0u16; 3];
        parcels[..p.len()].copy_from_slice(p);
        decode_text(&Insn {
            kind: k,
            parcels,
            len: p.len(),
        })
    }

    #[test]
    fn decodes_instructions_seen_in_the_firmware() {
        assert_eq!(dec(&[0x0FA4, 0x0070]), "M4 = 0x70");
        assert_eq!(dec(&[0x0F15, 0x000C, 0x0440]), "I5 = 0xc0440");
        assert_eq!(dec(&[0x1486, 0x0000, 0x0020]), "BIT TST ASTATx 0x20");
        assert_eq!(
            dec(&[0x6A3E, 0x060A, 0x1880]),
            "R12 = DM(0, I5), F8 = PASS F8"
        );
        assert_eq!(dec(&[0x687E, 0x143F]), "R8 = DM(I4, 2)");
        assert_eq!(dec(&[0x9228, 0x0A88]), "I5 = PM(8, I9)");
        // Type 4: DATA6 is a signed offset; a zero compute field prints no NOP (ADI ground truth)
        assert_eq!(
            dec(&[0x6A3F, 0x218D, 0xA00F]),
            "R3 = DM(-28, I5), F0 = FLOAT R0 BY R15"
        );
        assert_eq!(dec(&[0x507E, 0x8000, 0x0000]), "DM(I0, M1) = R0");
        // shift immediate (opcode 0x02), ADI elfdump ground truth
        assert_eq!(dec(&[0x023E, 0x1010, 0x1810]), "R1 = FEXT R0 BY 24:8");
        assert_eq!(dec(&[0x023E, 0x3011, 0x0700]), "R0 = FDEP R0 BY 7:24");
        assert_eq!(dec(&[0x023E, 0x1012, 0x9614]), "R1 = FEXT R4 BY 22:10 (SE)");
        assert_eq!(dec(&[0x023E, 0x7801, 0xFF11]), "R1 = ASHIFT R1 BY -1");
        assert_eq!(dec(&[0x023E, 0x0032, 0x0044]), "R4 = BTGL R4 BY 0");
        // shift immediate with a data move (Type 6a), ADI elfdump ground truth (SMP image)
        assert_eq!(
            dec(&[0x827E, 0x0280, 0x0188]),
            "R8 = LSHIFT R8 BY 1, R5 = DM(I1, M1)"
        );
        assert_eq!(
            dec(&[0x8AFE, 0xF900, 0xE811]),
            "R1 = LSHIFT R1 BY -24, DM(I5, M3) = R2"
        );
        assert_eq!(
            dec(&[0x8ABE, 0x0008, 0x1823]),
            "R2 = R2 OR LSHIFT R3 BY 24, R0 = DM(I5, M2)"
        );
        assert_eq!(
            dec(&[0x893B, 0x3E9B, 0x893B]),
            "IF NOT_TF R3 = R3 OR FDEP R11 BY 9:30 (SE), R13 = PM(I12, M12)"
        );
        // dual moves, immediate stores and index modify, ADI elfdump ground truth (docs 5.34)
        assert_eq!(
            dec(&[0x2C40, 0x9211, 0x4000]),
            "R0 = DM(I6, M1), R4 = PM(I10, M10), MRF = 0"
        );
        assert_eq!(
            dec(&[0x2CC0, 0x921B, 0xC004]),
            "R0 = DM(I6, M3), R4 = PM(I10, M10), MRF = MRF + R0 * R4 (SSF)"
        );
        assert_eq!(dec(&[0x3D89, 0xC83F]), "DM(I6, M6) = R4, R0 = PM(I15, M9)");
        assert_eq!(dec(&[0x9040, 0x0000, 0x0000]), "DM(I0, M1) = 0x0");
        assert_eq!(dec(&[0x92C3, 0x3D0B, 0x22A7]), "DM(I1, M3) = 0x3d0b22a7");
        assert_eq!(dec(&[0x9BC4, 0x3E9D]), "DM(I5, M7) = 0x3e9d");
        assert_eq!(dec(&[0x043E, 0x583F]), "I1 = MODIFY(I1, M3)");
        assert_eq!(
            dec(&[0x043F, 0xD808, 0x1BAE]),
            "I7 = MODIFY(I7, M3), F11 = F10 + F14"
        );
        // shift immediate with a data move (ADI elfdump ground truth, docs 5.36)
        assert_eq!(
            dec(&[0x8B7E, 0x3011, 0x4650]),
            "R5 = FDEP R0 BY 6:25, R0 = DM(I5, M5)"
        );
        // multiplier result register transfers (ADI elfdump ground truth, docs 5.36)
        assert_eq!(dec(&[0x5EBE, 0x8440, 0x1100]), "DM(I7, M2) = R8, R1 = MR1F");
        assert_eq!(dec(&[0x5CBE, 0x0440, 0x0000]), "R8 = DM(I6, M2), R0 = MR0F");
        // Type 4 post-modify, MODIFY bank/destination, loop counts (ADI elfdump ground truth)
        assert_eq!(dec(&[0x6C7E, 0x10BF]), "R1 = DM(I6, 2)");
        assert_eq!(
            dec(&[0x6CFE, 0x1008, 0x122E]),
            "DM(I6, 2) = R0, F2 = F2 + F14"
        );
        assert_eq!(dec(&[0x043E, 0xE73F]), "I4 = MODIFY(I3, M4)");
        assert_eq!(
            dec(&[0x0D05, 0x0080, 0x0006]),
            "DO (PC, 6) UNTIL LCE, LCNTR = R5"
        );
        assert_eq!(
            dec(&[0x0D00, 0x0000, 0x0003]),
            "DO (PC, 3) UNTIL LCE, LCNTR = R0"
        );
        assert_eq!(
            dec(&[0x0C00, 0x6480, 0x0003]),
            "DO (PC, 3) UNTIL LCE, LCNTR = 100"
        );
        assert_eq!(dec(&[0x783E, 0x0080, 0x0000]), "R1 <-> S0");
        // NOP forms, indirect calls, delayed returns, sign-extended 16-bit immediates (ADI)
        assert_eq!(dec(&[0x0000, 0x0000, 0x0000]), "NOP");
        assert_eq!(dec(&[0x0003]), "NOP");
        assert_eq!(dec(&[0x08BE, 0x003F]), "CALL (M8, I8)");
        assert_eq!(dec(&[0x083E, 0x043F]), "JUMP (M8, I8) (DB)");
        assert_eq!(dec(&[0x0AFE]), "RTS (DB)");
        assert_eq!(dec(&[0x0ABE]), "RTS");
        assert_eq!(dec(&[0x0F8F, 0xFFE1]), "R15 = 0xffffffe1");
        // register moves and swaps, ADI elfdump ground truth (docs 5.34)
        assert_eq!(dec(&[0x703E, 0x933F]), "M6 = R1");
        assert_eq!(dec(&[0x753E, 0x93BF]), "M7 = S1");
        assert_eq!(dec(&[0x7A3E, 0x04BF]), "R9 <-> S8");
        assert_eq!(dec(&[0x72BE, 0x0188, 0x1C00]), "R3 = M8, F12 = F0 + F0");
        // Type 15b: UREG bits 29..23, signed DATA7 bits 22..16
        assert_eq!(dec(&[0x9E08, 0x0C00]), "I8 = DM(0, I7)");
        assert_eq!(dec(&[0x9208, 0x007A]), "R0 = DM(-6, I1)");
        assert_eq!(dec(&[0x9908, 0x0014]), "DM(20, I4) = R0");
        assert_eq!(dec(&[0x1000, 0x0001, 0x06BE]), "R0 = DM(0x106be)");
        assert_eq!(dec(&[0x1103, 0x000C, 0x02C0]), "DM(0xc02c0) = R3");
        assert_eq!(dec(&[0xC000 | 0x0F34]), "F3 = F3 * F4");
        assert_eq!(dec(&[0x1629, 0x0000, 0x0012]), "I4 = MODIFY(I1, 0x12)");
        assert_eq!(
            dec(&[0x023E, 0x2019, 0x1002]),
            "R0 = R0 OR FDEP R2 BY 16:16"
        );
    }

    #[test]
    fn listing_line_matches_the_python_format() {
        let (insns, _) = tile(&parcels_le(&[0x15, 0x0F, 0x0C, 0x00, 0x40, 0x04]));
        assert_eq!(
            listing_line(0x2423DE, &insns[0]),
            "0x2423de 17a   0f15 000c 0440  I5 = 0xc0440"
        );
    }

    #[test]
    fn entry_code_of_the_fx_core1_section_reads_back() {
        // entry code of the FX Core1 section: BIT CLR; NOP; CALL; CALL; R0 = 0; I0 = 0xc0430
        let stream = [
            0x1422, 0x0000, 0x1000, 0x0001, 0x06BE, 0x0012, 0x0528, 0x06BE, 0x0012, 0x05A0, 0x0F80,
            0x0000, 0x0F10, 0x000C, 0x0430,
        ];
        let (insns, exact) = tile(&stream);
        assert!(exact);
        let text: Vec<String> = insns.iter().map(decode_text).collect();
        assert_eq!(
            text,
            [
                "BIT CLR MODE1 0x1000",
                "NOP",
                "CALL 0x120528",
                "CALL 0x1205a0",
                "R0 = 0x0",
                "I0 = 0xc0430"
            ]
        );
    }

    #[test]
    fn mixed_forms_decode_without_panicking() {
        // every possible first parcel must classify and decode safely
        for p0 in (0u32..=0xFFFF).step_by(7) {
            let p0 = p0 as u16;
            let (kind, len) = classify(p0, Some(0x003F));
            let mut parcels = [0u16; 3];
            parcels[0] = p0;
            parcels[1] = 0x003F;
            let insn = Insn {
                kind,
                parcels,
                len: len.min(3),
            };
            assert!(!decode_text(&insn).is_empty());
        }
    }
}
