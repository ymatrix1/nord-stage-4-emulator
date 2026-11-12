//! Instruction text: the operand decoding of every type and the listing line format.
use super::classify::{bits, word48};
use super::compute::{compute_text, short_compute_text};
use super::regs::{ureg_name, COND};
use super::types::{Insn, Kind};
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
    let has_compute = matches!(kind, Kind::T3a | Kind::T4a);
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
        (
            format!(
                "{}({}, I{i})",
                if pm { "PM" } else { "DM" },
                bits(w, 32, 27)
            ),
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

pub(super) fn type_only(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::T1a => "dual DM/PM move + compute",
        Kind::T1b => "dual DM/PM move",
        Kind::T6a => "shift immediate",
        Kind::T7a => "index modify + compute",
        Kind::T7b => "index modify",
        Kind::T9a => "indirect/rel branch + compute",
        Kind::T9b => "indirect/rel branch",
        Kind::T10a => "jump + dual move + compute",
        Kind::T16a => "immediate store",
        Kind::T16b => "immediate store (16-bit)",
        _ => return None,
    })
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
            if bits(w, 43, 43) == 1 {
                let raw = w >> (48 - 16 * parcels.len());
                return format!("{iff}Dreg <-> CDreg swap (raw {raw:x})");
            }
            let src = (bits(w, 42, 38) << 2) | bits(w, 32, 31);
            let tail = if kind == Kind::T5a {
                format!(", {}", compute_text(bits(w, 22, 0)))
            } else {
                String::new()
            };
            format!(
                "{iff}{} = {}{tail}",
                ureg_name(bits(w, 30, 24) as u8),
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
            let ea = format!("DM({}, I{})", bits(w, 29, 23), bits(w, 43, 41));
            let u = ureg_name(bits(w, 22, 16) as u8);
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
        Kind::T2x => {
            let extra = bits(w, 31, 23);
            let tail = if extra != 0 {
                format!("  [extra bits 31..23 = 0x{extra:x}]")
            } else {
                String::new()
            };
            format!("{iff}{}{tail}", compute_text(bits(w, 22, 0)))
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
            bits(w, 31, 16)
        ),
        Kind::T18a => {
            let sreg = ureg_name((0x70 | bits(w, 35, 32)) as u8);
            format!("BIT {} {sreg} 0x{:x}", bop(bits(w, 39, 37)), bits(w, 31, 0))
        }
        Kind::T8a => {
            let kind_s = if bits(w, 39, 39) == 1 { "CALL" } else { "JUMP" };
            let rel = if bits(w, 40, 40) == 1 {
                " (PC,rel)"
            } else {
                ""
            };
            format!("{iff}{kind_s}{rel} 0x{:x}", bits(w, 23, 0))
        }
        Kind::T2a => format!("{iff}{}", compute_text(bits(w, 22, 0))),
        Kind::T21c => {
            if parcels[0] == 0x0001 {
                "NOP".to_string()
            } else {
                format!("21c? raw {}", raw_hex(parcels))
            }
        }
        Kind::T11a | Kind::T11c => {
            format!("{iff}{}", if bits(w, 40, 40) == 1 { "RTI" } else { "RTS" })
        }
        Kind::T12a => format!(
            "DO 0x{:x} UNTIL LCE, LCNTR = {}",
            bits(w, 23, 0),
            bits(w, 39, 24)
        ),
        Kind::T13a => format!(
            "DO 0x{:x} UNTIL {}",
            bits(w, 23, 0),
            COND[bits(w, 37, 33) as usize]
        ),
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
        assert_eq!(dec(&[0x687E, 0x143F]), "R8 = DM(2, I4)");
        assert_eq!(dec(&[0x1000, 0x0001, 0x06BE]), "R0 = DM(0x106be)");
        assert_eq!(dec(&[0x1103, 0x000C, 0x02C0]), "DM(0xc02c0) = R3");
        assert_eq!(dec(&[0xC000 | 0x0F34]), "F3 = F3 * F4");
        assert_eq!(dec(&[0x1629, 0x0000, 0x0012]), "I4 = MODIFY(I1, 0x12)");
        assert_eq!(
            dec(&[0x023E, 0x2019, 0x1002]),
            "R0 = MRF + R0 * R2 (SUIR)  [extra bits 31..23 = 0x40]"
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
