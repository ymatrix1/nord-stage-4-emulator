//! Text of the program-flow modifiers and of the stack, `DO UNTIL` and compiler (Type 25)
//! instructions. Every layout is VERIFIED against ADI `elfdump` on instructions assembled with
//! `easm21k -proc ADSP-21489` (probe list in `docs/research/sharc-leftovers.md`).
use super::classify::bits;
use super::regs::COND;

/// Joins the set modifiers as ADI prints them: ` (DB,LA,CI)`, or nothing when none is set.
pub(super) fn modifiers(flags: &[(&str, bool)]) -> String {
    let on: Vec<&str> = flags.iter().filter(|f| f.1).map(|f| f.0).collect();
    if on.is_empty() {
        String::new()
    } else {
        format!(" ({})", on.join(","))
    }
}

/// Modifiers of Types 8a and 9a/9b: `DB` bit 26, `LA` bit 38 (jumps only, bit 39 is the call
/// bit), `CI` bit 24 (`jump (m8,i8)(ci)` is `083e 013f`, `jump (pc,4)(la,ci)` is `077e 0100 0004`).
pub(super) fn branch_modifiers(w: u64) -> String {
    let jump = bits(w, 39, 39) == 0;
    modifiers(&[
        ("DB", bits(w, 26, 26) == 1),
        ("LA", jump && bits(w, 38, 38) == 1),
        ("CI", bits(w, 24, 24) == 1),
    ])
}

/// Modifiers of Types 11a/11c. The 16-bit form (11c) carries `DB` in bit 38 and `LR` in bit 32
/// (`rts (db,lr)` is `0aff`); the 48-bit form (11a) in bits 26 and 24 (`0a3e 0400 0000` is `rts
/// (db)`, `0a3e 0100 0000` is `rts (lr)`; bit 25 is the ELSE flag).
pub(super) fn return_modifiers(w: u64, short: bool) -> String {
    let (db, lr) = if short { (38, 32) } else { (26, 24) };
    modifiers(&[("DB", bits(w, db, db) == 1), ("LR", bits(w, lr, lr) == 1)])
}

/// Termination name of a `DO ... UNTIL` loop: code 31 is `FOREVER` (not `TRUE`).
pub(super) fn term_name(code: u64) -> &'static str {
    if code == 31 {
        "FOREVER"
    } else {
        COND[code as usize]
    }
}

/// Type 13a: `DO (PC, <rel>) UNTIL <term>`; the 23-bit relative end address is signed (bit 23 is
/// not part of it, as in Type 12a).
pub(super) fn do_until_text(w: u64) -> String {
    let shift = 64 - 23;
    let offset = ((bits(w, 22, 0) << shift) as i64) >> shift;
    format!("DO (PC, {offset}) UNTIL {}", term_name(bits(w, 37, 33)))
}

/// Type 20a: the stack operations in the order ADI lists them. A push wins over a pop of the
/// same stack when both bits are set (`17c0` prints `push loop`); no bit set prints nothing.
pub(super) fn stack_text(w: u64) -> String {
    let mut parts = Vec::new();
    for (push, pop, name) in [(39, 38, "LOOP"), (37, 36, "STS"), (35, 34, "PCSTK")] {
        if bits(w, push, push) == 1 {
            parts.push(format!("PUSH {name}"));
        } else if bits(w, pop, pop) == 1 {
            parts.push(format!("POP {name}"));
        }
    }
    if bits(w, 33, 33) == 1 {
        parts.push("FLUSH CACHE".to_string());
    }
    parts.join(", ")
}

/// Type 25: `RFRAME` (16-bit `1901` or 48-bit `1900 0000 0000`) or `CJUMP`. ADI prints no space
/// before the delay modifier: `cjump (pc,0x10)(db)`, `cjump 0x1234(db)`. Bit 38 selects the
/// PC-relative form, bit 34 is the delayed-branch flag (always set in compiler output).
pub(super) fn type25_text(w: u64) -> String {
    if bits(w, 47, 41) == 0x0C && bits(w, 40, 40) == 1 {
        return "RFRAME".to_string();
    }
    let db = if bits(w, 34, 34) == 1 { "(DB)" } else { "" };
    if bits(w, 38, 38) == 1 {
        format!("CJUMP (PC, 0x{:x}){db}", bits(w, 23, 0))
    } else {
        format!("CJUMP 0x{:x}{db}", bits(w, 23, 0))
    }
}

#[cfg(test)]
mod tests {
    use crate::visa::{decode_text, word48, Insn, Kind};

    fn dec(p: &[u16]) -> String {
        let (k, len) = crate::visa::classify(p[0], p.get(1).copied());
        assert_eq!(len, p.len(), "tiling of {p:04x?}");
        let mut parcels = [0u16; 3];
        parcels[..p.len()].copy_from_slice(p);
        decode_text(&Insn {
            kind: k,
            parcels,
            len: p.len(),
        })
    }

    /// ADI elfdump of the probe instructions (assembled with easm21k, read back).
    #[test]
    fn jump_and_call_modifiers_match_adi() {
        assert_eq!(dec(&[0x083E, 0x013F]), "JUMP (M8, I8) (CI)");
        assert_eq!(dec(&[0x083E, 0x053F]), "JUMP (M8, I8) (DB,CI)");
        assert_eq!(dec(&[0x087E, 0x043F]), "JUMP (M8, I8) (DB,LA)");
        assert_eq!(dec(&[0x0800, 0x013F]), "IF EQ JUMP (M8, I8) (CI)");
        assert_eq!(dec(&[0x08BE, 0x043F]), "CALL (M8, I8) (DB)");
        assert_eq!(dec(&[0x073E, 0x0500, 0x0004]), "JUMP (PC, 0x4) (DB,CI)");
        assert_eq!(dec(&[0x077E, 0x0100, 0x0004]), "JUMP (PC, 0x4) (LA,CI)");
        assert_eq!(dec(&[0x077E, 0x0500, 0x0004]), "JUMP (PC, 0x4) (DB,LA,CI)");
        assert_eq!(dec(&[0x0700, 0x0100, 0x0004]), "IF EQ JUMP (PC, 0x4) (CI)");
        assert_eq!(dec(&[0x07BE, 0x0400, 0x0004]), "CALL (PC, 0x4) (DB)");
    }

    #[test]
    fn return_modifiers_match_adi() {
        assert_eq!(dec(&[0x0ABF]), "RTS (LR)");
        assert_eq!(dec(&[0x0AFF]), "RTS (DB,LR)");
        assert_eq!(dec(&[0x0BBF]), "RTI (LR)");
        assert_eq!(dec(&[0x0BFF]), "RTI (DB,LR)");
        assert_eq!(dec(&[0x0A81]), "IF EQ RTS (LR)");
        // 48-bit forms: DB bit 26, LR bit 24; bits 38 and 32 mean nothing there (ADI)
        assert_eq!(dec(&[0x0A3E, 0x0400, 0x0000]), "RTS (DB)");
        assert_eq!(dec(&[0x0A3E, 0x0100, 0x0000]), "RTS (LR)");
        assert_eq!(dec(&[0x0A3E, 0x0500, 0x0000]), "RTS (DB,LR)");
        assert_eq!(dec(&[0x0A7F, 0x0000, 0x0000]), "RTS");
        assert_eq!(dec(&[0x0B3E, 0x0400, 0x0000]), "RTI (DB)");
        assert_eq!(dec(&[0x0A00, 0x0400, 0x0000]), "IF EQ RTS (DB)");
    }

    #[test]
    fn do_until_matches_adi() {
        assert_eq!(dec(&[0x0E00, 0x0000, 0x0007]), "DO (PC, 7) UNTIL EQ");
        assert_eq!(dec(&[0x0E20, 0x0000, 0x0007]), "DO (PC, 7) UNTIL NE");
        assert_eq!(dec(&[0x0E1E, 0x0000, 0x0007]), "DO (PC, 7) UNTIL LCE");
        assert_eq!(dec(&[0x0E3E, 0x0000, 0x0007]), "DO (PC, 7) UNTIL FOREVER");
        assert_eq!(dec(&[0x0E16, 0x0000, 0x0007]), "DO (PC, 7) UNTIL FLAG2_IN");
        assert_eq!(dec(&[0x0E08, 0x00FF, 0xFFF9]), "DO (PC, -7) UNTIL AV");
        // the boot kernel loop: ADI `do (pc,0xffffff81) until eq`
        assert_eq!(dec(&[0x0E00, 0x00FF, 0xFF81]), "DO (PC, -127) UNTIL EQ");
    }

    #[test]
    fn stack_instructions_match_adi() {
        assert_eq!(dec(&[0x1780, 0, 0]), "PUSH LOOP");
        assert_eq!(dec(&[0x1740, 0, 0]), "POP LOOP");
        assert_eq!(dec(&[0x1720, 0, 0]), "PUSH STS");
        assert_eq!(dec(&[0x1710, 0, 0]), "POP STS");
        assert_eq!(dec(&[0x1708, 0, 0]), "PUSH PCSTK");
        assert_eq!(dec(&[0x1704, 0, 0]), "POP PCSTK");
        assert_eq!(dec(&[0x17A8, 0, 0]), "PUSH LOOP, PUSH STS, PUSH PCSTK");
        assert_eq!(dec(&[0x1754, 0, 0]), "POP LOOP, POP STS, POP PCSTK");
        assert_eq!(dec(&[0x1706, 0, 0]), "POP PCSTK, FLUSH CACHE");
        assert_eq!(dec(&[0x1702, 0, 0]), "FLUSH CACHE");
        assert_eq!(dec(&[0x17C0, 0, 0]), "PUSH LOOP", "a push wins over a pop");
        assert_eq!(dec(&[0x1700, 0, 0]), "", "ADI prints an empty line");
    }

    #[test]
    fn compiler_instructions_match_adi() {
        assert_eq!(dec(&[0x1901]), "RFRAME", "16-bit form");
        assert_eq!(dec(&[0x1900, 0x0000, 0x0000]), "RFRAME", "48-bit form");
        assert_eq!(dec(&[0x1804, 0x0000, 0x1234]), "CJUMP 0x1234(DB)");
        assert_eq!(dec(&[0x1844, 0x0000, 0x0010]), "CJUMP (PC, 0x10)(DB)");
        assert_eq!(word48(&[0x1900]), 0x1900_0000_0000);
        assert_eq!(Kind::T25.name(), "25");
    }
}
