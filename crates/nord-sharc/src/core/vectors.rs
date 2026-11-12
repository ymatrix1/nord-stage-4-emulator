//! Interrupt vector numbers of the SHARC core: names, priorities and where each vector's latch
//! bit lives in IRPTL or LIRPTL (PRM Table B-2, A-12 and A-13).

/// Number of interrupt vectors (priorities 0 = EMUI, highest, to 41 = SFT3I, lowest).
pub const VECTOR_COUNT: usize = 42;

/// Mnemonics of the vectors in priority order (Table B-2); index 7 is reserved.
const NAMES: [&str; VECTOR_COUNT] = [
    "EMUI", "RSTI", "IICDI", "SOVFI", "TMZHI", "SPERRI", "BKPI", "RSV7", "IRQ2I", "IRQ1I", "IRQ0I",
    "P0I", "P1I", "P2I", "P3I", "P4I", "P5I", "P6I", "P7I", "P8I", "P9I", "P10I", "P11I", "P12I",
    "P13I", "P14I", "P15I", "P16I", "P17I", "P18I", "CB7I", "CB15I", "TMZLI", "FIXI", "FLTOI",
    "FLTUI", "FLTII", "EMULI", "SFT0I", "SFT1I", "SFT2I", "SFT3I",
];

/// Vector number of a mnemonic such as `P4I` or of a decimal/hex number below [`VECTOR_COUNT`].
pub fn vector_by_name(name: &str) -> Option<u8> {
    let upper = name.to_ascii_uppercase();
    if let Some(i) = NAMES.iter().position(|n| *n == upper) {
        return Some(i as u8);
    }
    let n = match upper.strip_prefix("0X") {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => upper.parse().ok()?,
    };
    (n < VECTOR_COUNT as u32).then_some(n as u8)
}

/// Where the latch bit of a vector lives: a bit of IRPTL, or a bit pair of LIRPTL (latch bit `j`,
/// mask bit `10 + j`, mask pointer `20 + j`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
    Irptl(u32),
    Lirptl(u32),
}

/// VERIFIED against PRM Table A-12 / A-13: vectors 0-16 are IRPTL bits 0-16, 17-24 are `P6I-P13I`
/// (LIRPTL 0-7), 25-27 are `P14I-P16I` (IRPTL 17-19), 28-29 are `P17I/P18I` (LIRPTL 8-9) and
/// 30-41 are IRPTL bits 20-31.
pub(super) fn slot(v: u8) -> Slot {
    match v {
        0..=16 => Slot::Irptl(v as u32),
        17..=24 => Slot::Lirptl(v as u32 - 17),
        25..=27 => Slot::Irptl(v as u32 - 8),
        28 | 29 => Slot::Lirptl(v as u32 - 20),
        _ => Slot::Irptl(v as u32 - 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_numbers_map_to_latch_bits() {
        assert_eq!(slot(11), Slot::Irptl(11)); // P0I
        assert_eq!(slot(15), Slot::Irptl(15)); // P4I
        assert_eq!(slot(17), Slot::Lirptl(0)); // P6I
        assert_eq!(slot(25), Slot::Irptl(17)); // P14I
        assert_eq!(slot(28), Slot::Lirptl(8)); // P17I
        assert_eq!(slot(30), Slot::Irptl(20)); // CB7I
        assert_eq!(slot(41), Slot::Irptl(31)); // SFT3I
        assert_eq!(vector_by_name("p4i"), Some(15));
        assert_eq!(vector_by_name("0x29"), Some(41));
        assert_eq!(vector_by_name("42"), None);
        assert_eq!(vector_by_name("nope"), None);
    }
}
