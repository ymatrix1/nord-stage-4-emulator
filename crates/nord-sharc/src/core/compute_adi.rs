//! Exhaustive check of the compute decoder and executor against ADI's disassembler.
//!
//! VERIFIED: `docs/research/sharc-type2c/` holds the output of ADI's `elfdump` (CCES 2.12.1) for
//! every 23-bit compute field (bits 22..12 enumerated, `Rn = 3`, `Rx = 1`, `Ry = 2`, wrapped in
//! the 48-bit Type 2a word `0x13E0_0000_0000 | compute`: `adi_compute.tsv`, 2048 rows) and for
//! every 12-bit Type 2c short compute (`0xC000 | OP:Rn:Rx`, `adi_short.tsv`, 4096 rows). The
//! tests require that every compute ADI knows is decoded to the same text and is executed by
//! [`Core::compute`] / [`Core::short_compute`], except the listed registers and cosmetic
//! differences.
use super::Core;
use crate::visa::{compute_text, short_compute_text};

const COMPUTE: &str = include_str!("../../../../docs/research/sharc-type2c/adi_compute.tsv");
const SHORT: &str = include_str!("../../../../docs/research/sharc-type2c/adi_short.tsv");

/// Lower case without blanks, parentheses and semicolons; the parts of a comma list sorted (ADI
/// lists the ALU half of a dual add/subtract first).
fn norm(s: &str) -> String {
    let flat: String = s
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '(' | ')' | ';'))
        .collect();
    let mut parts: Vec<&str> = flat.split(',').collect();
    parts.sort_unstable();
    parts.join(",")
}

fn rows(text: &str) -> impl Iterator<Item = (u64, &str)> {
    text.lines().map(|l| {
        let (h, t) = l.split_once('\t').unwrap_or((l, ""));
        (u64::from_str_radix(h, 16).unwrap(), t)
    })
}

/// ADI prints no instruction (or an operand-less `r3=`) for a reserved code.
fn reserved(adi: &str) -> bool {
    adi.is_empty() || adi.starts_with('=') || adi.ends_with('=')
}

/// Compute fields (bits 22..12) that ADI names but this model does not decode or execute: the
/// transfers `Rn = SHP|SHF|FTA|FTB|FTC|FR` and back (multifunction codes `0x08 0x09 0x0C..0x0F`
/// and `+0x10`). No firmware image uses them (`docs/research/sharc-type2c.md`).
fn unmodelled_register_transfer(hi: u64) -> bool {
    hi & !0x10 >= 0x408 && matches!(hi & 0x0F & !0x10, 0x8 | 0x9 | 0xC..=0xF) && hi >> 5 == 0x20
}

#[test]
fn every_adi_compute_is_decoded_and_executed() {
    let mut named = 0;
    for (c, adi) in rows(COMPUTE) {
        let hi = c >> 12;
        let ours = compute_text(c);
        let mut core = Core::new(&[]);
        let ok = core.compute(c);
        if reserved(adi) {
            assert!(!ok, "{c:06x}: reserved code executed ({adi:?})");
            assert!(
                ours.contains("?op"),
                "{c:06x}: reserved code decoded: {ours}"
            );
            continue;
        }
        // ADI prints a register name that is no multiplier register (SHP, SHF, FTA, FTB, FTC, FR)
        let odd = ["shp", "shf", "fta", "ftb", "ftc"]
            .iter()
            .any(|r| adi.ends_with(&format!("={r}")) || adi.starts_with(&format!("{r}=")))
            || adi.ends_with("=fr")
            || adi.starts_with("fr=");
        if odd {
            assert!(unmodelled_register_transfer(hi), "{c:06x}: {adi}");
            assert!(!ok && ours.contains("?op"), "{c:06x}: {ours}");
            continue;
        }
        named += 1;
        assert!(ok, "{c:06x}: ADI '{adi}' not executed (ours: {ours})");
        // cosmetic: ADI prints the destination of `Fm = Fxm * Fym, Ra = FIX Fxa BY Rya` as an F
        // register and `Fn = FUNPACK Rx` with the operand types swapped (easm21k rejects that text)
        if matches!(hi, 0x5B0..=0x5BF | 0x294) {
            continue;
        }
        assert_eq!(norm(&ours), norm(adi), "{c:06x}");
    }
    assert!(named > 700, "only {named} named compute codes");
}

#[test]
fn every_adi_short_compute_is_decoded_and_executed() {
    let mut count = 0;
    for (s, adi) in rows(SHORT) {
        count += 1;
        let mut core = Core::new(&[]);
        assert!(core.short_compute(s), "{s:03x}: {adi} not executed");
        assert_eq!(norm(&short_compute_text(s)), norm(adi), "{s:03x}");
    }
    assert_eq!(count, 4096);
}

#[test]
fn ssi_short_multiply_is_the_long_multiply() {
    // c7dc (0x12032E in the ORG image) = R13 = R13 * R12 (SSI), c701 (0x124C9A) = R0 = R0 * R1
    for (word, n, x, a, b) in [
        (0x7DCu64, 13, 12, 123_456i32, -789i32),
        (0x701, 0, 1, 40_000, 70_000),
    ] {
        let mut short = Core::new(&[]);
        short.r[n] = a as u32;
        short.r[x] = b as u32;
        let mut long = Core::new(&[]);
        long.r[n] = a as u32;
        long.r[x] = b as u32;
        assert!(short.short_compute(word));
        let (n64, x64) = (n as u64, x as u64);
        assert!(long.compute((1 << 20) | (0x70 << 12) | (n64 << 8) | (n64 << 4) | x64));
        assert_eq!(short.r[n], long.r[n]);
        assert_eq!(short.r[n] as i32, a.wrapping_mul(b));
        assert_eq!(short.sys.astat_extra, long.sys.astat_extra);
    }
}
