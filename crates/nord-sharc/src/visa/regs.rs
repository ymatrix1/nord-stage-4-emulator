//! Register and condition names of the universal register file (Table 10-2).

pub(super) const COND: [&str; 32] = [
    "EQ",
    "LT",
    "LE",
    "AC",
    "AV",
    "MV",
    "MS",
    "SV",
    "SZ",
    "FLAG0_IN",
    "FLAG1_IN",
    "FLAG2_IN",
    "FLAG3_IN",
    "TF",
    "BM",
    "LCE",
    "NE",
    "GE",
    "GT",
    "NOT_AC",
    "NOT_AV",
    "NOT_MV",
    "NOT_MS",
    "NOT_SV",
    "NOT_SZ",
    "NOT_FLAG0_IN",
    "NOT_FLAG1_IN",
    "NOT_FLAG2_IN",
    "NOT_FLAG3_IN",
    "NOT_TF",
    "NOT_BM",
    "TRUE",
];

pub(super) const SYS6: [&str; 16] = [
    "FADDR", "DADDR", "?", "PC", "PCSTK", "PCSTKP", "LADDR", "CURLCNTR", "LCNTR", "EMUCLK",
    "EMUCLK2", "PX", "PX1", "PX2", "TPERIOD", "TCOUNT",
];
pub(super) const SYS7: [&str; 16] = [
    "USTAT1", "USTAT2", "MODE1", "MMASK", "MODE2", "FLAGS", "ASTATx", "ASTATy", "STKYx", "STKYy",
    "IRPTL", "IMASK", "IMASKP", "LIRPTL", "USTAT3", "USTAT4",
];
pub(super) const UREG_GROUPS: [&str; 6] = ["R", "I", "M", "L", "B", "S"];

/// Name of a 7-bit universal register code (Table 10-2, processing element X).
pub fn ureg_name(code: u8) -> String {
    let group = (code >> 4) & 7;
    let num = (code & 15) as usize;
    match group {
        0..=5 => format!("{}{}", UREG_GROUPS[group as usize], num),
        6 => SYS6[num].to_string(),
        _ => SYS7[num].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names() {
        assert_eq!(ureg_name(0x24), "M4");
        assert_eq!(ureg_name(0x62), "?");
        assert_eq!(ureg_name(0x73), "MMASK");
    }
}
