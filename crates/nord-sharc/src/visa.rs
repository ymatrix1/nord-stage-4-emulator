//! ADSP-214xx VISA instruction classifier and text decoder.
//!
//! Port of `tools/sharc_visa.py` (same output, so both can be diffed on the real DSP images).
//! Layouts come from the SHARC Processor Programming Reference rev 2.4, chapters 9 to 12; the
//! parcel order (16-bit little-endian parcels, most significant parcel first) and the type
//! lengths are VERIFIED on the DSP images (`docs/research/dsp-image-hunt.md` 5.5 and 5.6). Field
//! positions not confirmed by the data are tagged ASSUMED in the Python tool and here.

/// Instruction type of the Programming Reference (letters: a = 48, b = 32, c = 16 bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    T1a,
    T1b,
    T2a,
    T2b,
    T2c,
    /// First byte `0x02`: undocumented 48-bit conditional compute (ASSUMED), see docs 5.9.
    T2x,
    T3a,
    T3b,
    T3c,
    T4a,
    T4b,
    T5a,
    T5b,
    T6a,
    T7a,
    T7b,
    T8a,
    T9a,
    T9b,
    T10a,
    T11a,
    T11c,
    T12a,
    T13a,
    T14a,
    T15a,
    T15b,
    T16a,
    T16b,
    T17a,
    T17b,
    T18a,
    T19a,
    T20a,
    T25,
    /// `000 00000` first byte: NOP, idle or data.
    T21,
    Unknown,
}

impl Kind {
    /// Name used by the Python tool (`"17a"`, `"21/22"`, `"??"`, ...).
    pub fn name(self) -> &'static str {
        use Kind::*;
        match self {
            T1a => "1a",
            T1b => "1b",
            T2a => "2a",
            T2b => "2b",
            T2c => "2c",
            T2x => "2x",
            T3a => "3a",
            T3b => "3b",
            T3c => "3c",
            T4a => "4a",
            T4b => "4b",
            T5a => "5a",
            T5b => "5b",
            T6a => "6a",
            T7a => "7a",
            T7b => "7b",
            T8a => "8a",
            T9a => "9a",
            T9b => "9b",
            T10a => "10a",
            T11a => "11a",
            T11c => "11c",
            T12a => "12a",
            T13a => "13a",
            T14a => "14a",
            T15a => "15a",
            T15b => "15b",
            T16a => "16a",
            T16b => "16b",
            T17a => "17a",
            T17b => "17b",
            T18a => "18a",
            T19a => "19a",
            T20a => "20a",
            T25 => "25",
            T21 => "21/22",
            Unknown => "??",
        }
    }
}

/// One instruction: its type and the 1 to 3 parcels it spans (shorter at the end of a stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insn {
    pub kind: Kind,
    pub parcels: [u16; 3],
    pub len: usize,
}

impl Insn {
    /// The parcels of this instruction.
    pub fn words(&self) -> &[u16] {
        &self.parcels[..self.len]
    }
}

/// Type and length (in 16-bit parcels) of the instruction starting at `p0`.
///
/// `p1` is the following parcel, needed to recognise 32-bit forms (low 7 bits `0x3F`).
pub fn classify(p0: u16, p1: Option<u16>) -> (Kind, usize) {
    use Kind::*;
    let marker = p1.is_some_and(|p| p & 0x7F == 0x3F);
    let top3 = p0 >> 13;
    let top4 = p0 >> 12;
    let sub = (p0 >> 2) & 7;
    if top4 == 0b1100 {
        return (T2c, 1);
    }
    if top4 == 0b1001 {
        if sub & 4 != 0 {
            return (T3c, 1);
        }
        return match sub {
            2 => (T15b, 2),
            1 => (T16b, 2),
            _ => (T16a, 3),
        };
    }
    match top3 {
        0b001 => return if marker { (T1b, 2) } else { (T1a, 3) },
        0b010 => return if marker { (T3b, 2) } else { (T3a, 3) },
        0b011 => {
            let five = (p0 >> 12) & 1 == 1;
            return match (five, marker) {
                (false, true) => (T4b, 2),
                (false, false) => (T4a, 3),
                (true, true) => (T5b, 2),
                (true, false) => (T5a, 3),
            };
        }
        0b100 => return (T6a, 3),
        0b101 => return (T15a, 3),
        0b110 | 0b111 => return (T10a, 3),
        _ => {}
    }
    if matches!(p0 >> 8, 0x18 | 0x19) {
        return (T25, 3);
    }
    let code = (p0 >> 8) & 0x1F;
    let short = (p0 >> 7) & 1 == 1;
    match code {
        0x01 => {
            if short {
                (T2b, 2)
            } else {
                (T2a, 3)
            }
        }
        0x02 => (T2x, 3),
        0x04 => {
            if marker {
                (T7b, 2)
            } else {
                (T7a, 3)
            }
        }
        0x06 | 0x07 => (T8a, 3),
        0x08 | 0x09 => {
            if marker {
                (T9b, 2)
            } else {
                (T9a, 3)
            }
        }
        0x0A | 0x0B => {
            if short {
                (T11c, 1)
            } else {
                (T11a, 3)
            }
        }
        0x0C | 0x0D => (T12a, 3),
        0x0E => (T13a, 3),
        0x0F => {
            if short {
                (T17b, 2)
            } else {
                (T17a, 3)
            }
        }
        0x14 => (T18a, 3),
        0x16 => (T19a, 3),
        0x17 => (T20a, 3),
        0x10..=0x13 => (T14a, 3),
        0x00 => (T21, 3),
        _ => (Unknown, 3),
    }
}

/// Splits parcels into instructions. The flag says the last instruction ends exactly at the end.
pub fn tile(parcels: &[u16]) -> (Vec<Insn>, bool) {
    let mut out = Vec::new();
    let mut i = 0;
    while i < parcels.len() {
        let (kind, len) = classify(parcels[i], parcels.get(i + 1).copied());
        let end = (i + len).min(parcels.len());
        let mut p = [0u16; 3];
        p[..end - i].copy_from_slice(&parcels[i..end]);
        out.push(Insn {
            kind,
            parcels: p,
            len: end - i,
        });
        i += len;
    }
    (out, i == parcels.len())
}

/// Little-endian 16-bit parcels of `data` (an odd trailing byte is dropped).
pub fn parcels_le(data: &[u8]) -> Vec<u16> {
    data.chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// MSB-aligned 48-bit value of an instruction (shorter ones are left-aligned).
pub fn word48(parcels: &[u16]) -> u64 {
    let mut w: u64 = 0;
    for p in parcels {
        w = (w << 16) | u64::from(*p);
    }
    w << (48 - 16 * parcels.len())
}

/// Bits `hi..=lo` of a 48-bit instruction word.
pub fn bits(w: u64, hi: u32, lo: u32) -> u64 {
    (w >> lo) & ((1u64 << (hi - lo + 1)) - 1)
}

/// `(ureg code, immediate)` of a Type 17a/17b instruction.
pub fn ureg_load(insn: &Insn) -> Option<(u8, u32)> {
    let p = insn.words();
    match insn.kind {
        Kind::T17b if p.len() >= 2 => Some(((p[0] & 0x7F) as u8, u32::from(p[1]))),
        Kind::T17a if p.len() >= 3 => Some((
            (p[0] & 0x7F) as u8,
            (u32::from(p[1]) << 16) | u32::from(p[2]),
        )),
        _ => None,
    }
}

const COND: [&str; 32] = [
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

const SYS6: [&str; 16] = [
    "FADDR", "DADDR", "?", "PC", "PCSTK", "PCSTKP", "LADDR", "CURLCNTR", "LCNTR", "EMUCLK",
    "EMUCLK2", "PX", "PX1", "PX2", "TPERIOD", "TCOUNT",
];
const SYS7: [&str; 16] = [
    "USTAT1", "USTAT2", "MODE1", "MMASK", "MODE2", "FLAGS", "ASTATx", "ASTATy", "STKYx", "STKYy",
    "IRPTL", "IMASK", "IMASKP", "LRPTL", "USTAT3", "USTAT4",
];
const UREG_GROUPS: [&str; 6] = ["R", "I", "M", "L", "B", "S"];

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

/// Tables 12-3 and 12-4 (single-function ALU): `opcode -> template` with `{n}`, `{x}`, `{y}`.
fn alu_template(opcode: u64) -> Option<&'static str> {
    Some(match opcode {
        0x01 => "R{n} = R{x} + R{y}",
        0x02 => "R{n} = R{x} - R{y}",
        0x05 => "R{n} = R{x} + R{y} + CI",
        0x06 => "R{n} = R{x} - R{y} + CI - 1",
        0x09 => "R{n} = (R{x} + R{y})/2",
        0x0A => "COMP(R{x}, R{y})",
        0x0B => "COMPU(R{x}, R{y})",
        0x25 => "R{n} = R{x} + CI",
        0x26 => "R{n} = R{x} + CI - 1",
        0x29 => "R{n} = R{x} + 1",
        0x2A => "R{n} = R{x} - 1",
        0x22 => "R{n} = -R{x}",
        0x30 => "R{n} = ABS R{x}",
        0x21 => "R{n} = PASS R{x}",
        0x40 => "R{n} = R{x} AND R{y}",
        0x41 => "R{n} = R{x} OR R{y}",
        0x42 => "R{n} = R{x} XOR R{y}",
        0x43 => "R{n} = NOT R{x}",
        0x61 => "R{n} = MIN(R{x}, R{y})",
        0x62 => "R{n} = MAX(R{x}, R{y})",
        0x63 => "R{n} = CLIP R{x} BY R{y}",
        0x81 => "F{n} = F{x} + F{y}",
        0x82 => "F{n} = F{x} - F{y}",
        0x91 => "F{n} = ABS (F{x} + F{y})",
        0x92 => "F{n} = ABS (F{x} - F{y})",
        0x89 => "F{n} = (F{x} + F{y})/2",
        0x8A => "COMP(F{x}, F{y})",
        0xA2 => "F{n} = -F{x}",
        0xB0 => "F{n} = ABS F{x}",
        0xA1 => "F{n} = PASS F{x}",
        0xA5 => "F{n} = RND F{x}",
        0xBD => "F{n} = SCALB F{x} BY R{y}",
        0xAD => "R{n} = MANT F{x}",
        0xC1 => "R{n} = LOGB F{x}",
        0xD9 => "R{n} = FIX F{x} BY R{y}",
        0xC9 => "R{n} = FIX F{x}",
        0xDD => "R{n} = TRUNC F{x} BY R{y}",
        0xCD => "R{n} = TRUNC F{x}",
        0xDA => "F{n} = FLOAT R{x} BY R{y}",
        0xCA => "F{n} = FLOAT R{x}",
        0xC4 => "F{n} = RECIPS F{x}",
        0xC5 => "F{n} = RSQRTS F{x}",
        0xE0 => "F{n} = F{x} COPYSIGN F{y}",
        0xE1 => "F{n} = MIN(F{x}, F{y})",
        0xE2 => "F{n} = MAX(F{x}, F{y})",
        0xE3 => "F{n} = CLIP F{x} BY F{y}",
        _ => return None,
    })
}

/// Table 12-11 (shifter): `opcode -> template`.
fn shift_template(opcode: u64) -> Option<&'static str> {
    Some(match opcode {
        0x00 => "R{n} = LSHIFT R{x} BY R{y}",
        0x20 => "R{n} = R{n} OR LSHIFT R{x} BY R{y}",
        0x04 => "R{n} = ASHIFT R{x} BY R{y}",
        0x24 => "R{n} = R{n} OR ASHIFT R{x} BY R{y}",
        0x08 => "R{n} = ROT R{x} BY R{y}",
        0xC4 => "R{n} = BCLR R{x} BY R{y}",
        0xC0 => "R{n} = BSET R{x} BY R{y}",
        0xC8 => "R{n} = BTGL R{x} BY R{y}",
        0xCC => "BTST R{x} BY R{y}",
        0x44 => "R{n} = FDEP R{x} BY R{y}",
        0x4C => "R{n} = FDEP R{x} BY R{y} (SE)",
        0x64 => "R{n} = R{n} OR FDEP R{x} BY R{y}",
        0x6C => "R{n} = R{n} OR FDEP R{x} BY R{y} (SE)",
        0x40 => "R{n} = FEXT R{x} BY R{y}",
        0x48 => "R{n} = FEXT R{x} BY R{y} (SE)",
        0x80 => "R{n} = EXP R{x}",
        0x84 => "R{n} = EXP R{x} (EX)",
        0x88 => "R{n} = LEFTZ R{x}",
        0x8C => "R{n} = LEFTO R{x}",
        0x90 => "R{n} = FPACK F{x}",
        0x94 => "F{n} = FUNPACK R{x}",
        0x74 => "BITDEP R{x} BY R{y}",
        0x50 => "R{n} = BITEXT R{x}",
        0x58 => "R{n} = BITEXT R{x} (NU)",
        0x70 => "R{n} = BFFWRP1",
        _ => return None,
    })
}

/// Table 12-12 (multiplier + ALU), opcode = compute bits 21..16.
fn mulalu_template(opcode: u64) -> Option<&'static str> {
    Some(match opcode {
        0b000100 => "Rm = Rxm * Rym (SSFR), Ra = Rxa + Rya",
        0b000101 => "Rm = Rxm * Rym (SSFR), Ra = Rxa - Rya",
        0b000110 => "Rm = Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
        0b001000 => "MRF = MRF + Rxm * Rym (SSF), Ra = Rxa + Rya",
        0b001001 => "MRF = MRF + Rxm * Rym (SSF), Ra = Rxa - Rya",
        0b001010 => "MRF = MRF + Rxm * Rym (SSF), Ra = (Rxa + Rya)/2",
        0b001100 => "Rm = MRF + Rxm * Rym (SSFR), Ra = Rxa + Rya",
        0b001101 => "Rm = MRF + Rxm * Rym (SSFR), Ra = Rxa - Rya",
        0b001110 => "Rm = MRF + Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
        0b010000 => "MRF = MRF - Rxm * Rym (SSF), Ra = Rxa + Rya",
        0b010001 => "MRF = MRF - Rxm * Rym (SSF), Ra = Rxa - Rya",
        0b010010 => "MRF = MRF - Rxm * Rym (SSF), Ra = (Rxa + Rya)/2",
        0b010100 => "Rm = MRF - Rxm * Rym (SSFR), Ra = Rxa + Rya",
        0b010101 => "Rm = MRF - Rxm * Rym (SSFR), Ra = Rxa - Rya",
        0b010110 => "Rm = MRF - Rxm * Rym (SSFR), Ra = (Rxa + Rya)/2",
        0b011000 => "Fm = Fxm * Fym, Fa = Fxa + Fya",
        0b011001 => "Fm = Fxm * Fym, Fa = Fxa - Fya",
        0b011010 => "Fm = Fxm * Fym, Fa = FLOAT Rxa BY Rya",
        0b011011 => "Fm = Fxm * Fym, Ra = FIX Fxa BY Rya",
        0b011100 => "Fm = Fxm * Fym, Fa = (Fxa + Fya)/2",
        0b011101 => "Fm = Fxm * Fym, Fa = ABS Fxa",
        0b011110 => "Fm = Fxm * Fym, Fa = MAX (Fxa, Fya)",
        0b011111 => "Fm = Fxm * Fym, Fa = MIN (Fxa, Fya)",
        _ => return None,
    })
}

const SHORT_OPS: [&str; 16] = [
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
];

fn fill(template: &str, n: u64, x: u64, y: u64) -> String {
    template
        .replace("{n}", &n.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
}

/// Type 2c 12-bit short compute: OP in bits 11..8, Rn in 7..4, Rx in 3..0.
pub fn short_compute_text(c: u64) -> String {
    fill(
        SHORT_OPS[bits(c, 11, 8) as usize],
        bits(c, 7, 4),
        bits(c, 3, 0),
        0,
    )
}

/// Multiplier opcodes of Tables 12-5 to 12-7 (fixed point) and `Fn = Fx * Fy`.
pub fn mult_text(opcode: u64, rn: u64, rx: u64, ry: u64) -> Option<String> {
    if opcode == 0x30 {
        return Some(format!("F{rn} = F{rx} * F{ry}"));
    }
    let cls = opcode >> 6;
    let y = (opcode >> 5) & 1;
    let x = (opcode >> 4) & 1;
    let f = (opcode >> 3) & 1;
    let sel = (opcode >> 1) & 3;
    let r = opcode & 1;
    let fi = if f == 1 { 'F' } else { 'I' };
    let m = format!(
        "({}{}{}{})",
        if x == 1 { 'S' } else { 'U' },
        if y == 1 { 'S' } else { 'U' },
        fi,
        if r == 1 { "R" } else { "" }
    );
    if cls == 0 {
        if opcode >> 4 == 0 {
            let sat = match sel {
                0 => format!("R{rn} = SAT MRF"),
                1 => format!("R{rn} = SAT MRB"),
                2 => "MRF = SAT MRF".to_string(),
                _ => "MRB = SAT MRB".to_string(),
            };
            return Some(format!(
                "{sat} ({}{})",
                if opcode & 1 == 1 { 'U' } else { 'S' },
                fi
            ));
        }
        if opcode == 0x14 {
            return Some("MRF = 0".to_string());
        }
        if opcode == 0x16 {
            return Some("MRB = 0".to_string());
        }
        if opcode >> 4 == 1 && (opcode >> 3) & 1 == 1 {
            let reg = match sel {
                0 => format!("R{rn} = RND MRF"),
                1 => format!("R{rn} = RND MRB"),
                2 => "MRF = RND MRF".to_string(),
                _ => "MRB = RND MRB".to_string(),
            };
            return Some(format!(
                "{reg} ({}F)",
                if opcode & 1 == 1 { 'U' } else { 'S' }
            ));
        }
        return None;
    }
    let reg = match sel {
        0 | 1 => format!("R{rn}"),
        2 => "MRF".to_string(),
        _ => "MRB".to_string(),
    };
    let acc = if cls > 1 {
        let mr = if sel == 1 || sel == 3 { "MRB" } else { "MRF" };
        let sign = if cls == 2 { "+" } else { "-" };
        format!("{mr} {sign} ")
    } else {
        String::new()
    };
    Some(format!("{reg} = {acc}R{rx} * R{ry} {m}"))
}

/// Multifunction compute fields (bit 22 set): multiplier + ALU, multiplier + dual ALU.
pub fn multi_text(c: u64) -> Option<String> {
    if bits(c, 22, 22) == 0 {
        return None;
    }
    let xm = bits(c, 7, 6);
    let ym = 4 + bits(c, 5, 4);
    let xa = 8 + bits(c, 3, 2);
    let ya = 12 + bits(c, 1, 0);
    let rm = bits(c, 15, 12);
    let ra = bits(c, 11, 8);
    if bits(c, 21, 21) == 1 {
        let k = if bits(c, 20, 20) == 1 { 'F' } else { 'R' };
        let rs = bits(c, 19, 16);
        return Some(format!(
            "{k}{rm} = {k}{xm} * {k}{ym}, {k}{ra} = {k}{xa} + {k}{ya}, {k}{rs} = {k}{xa} - {k}{ya}"
        ));
    }
    let text = mulalu_template(bits(c, 21, 16))?;
    let k = if text.starts_with("Fm") { "F" } else { "R" };
    let out = text
        .replace("Rxm", &format!("{k}{xm}"))
        .replace("Fxm", &format!("F{xm}"))
        .replace("Rym", &format!("{k}{ym}"))
        .replace("Fym", &format!("F{ym}"))
        .replace("Rxa", &format!("{k}{xa}"))
        .replace("Fxa", &format!("F{xa}"))
        .replace("Rya", &format!("{k}{ya}"))
        .replace("Fya", &format!("F{ya}"))
        .replace("Rm", &format!("R{rm}"))
        .replace("Fm", &format!("F{rm}"));
    Some(
        out.replace("Ra =", &format!("R{ra} ="))
            .replace("Fa =", &format!("F{ra} =")),
    )
}

/// Text for a 23-bit compute field (ALU, multiplier, shifter, multifunction; else raw).
pub fn compute_text(c: u64) -> String {
    if c == 0 {
        return "NOP".to_string();
    }
    let cu = bits(c, 22, 20);
    let opcode = bits(c, 19, 12);
    let rn = bits(c, 11, 8);
    let rx = bits(c, 7, 4);
    let ry = bits(c, 3, 0);
    if cu == 0 {
        if let Some(t) = alu_template(opcode) {
            return fill(t, rn, rx, ry);
        }
        let top = bits(c, 19, 16);
        if top == 0x7 || top == 0xF {
            let k = if top == 0xF { 'F' } else { 'R' };
            return format!(
                "{k}{} = {k}{rx} - {k}{ry}, {k}{rn} = {k}{rx} + {k}{ry}",
                bits(c, 15, 12)
            );
        }
    }
    if cu == 1 {
        if let Some(t) = mult_text(opcode, rn, rx, ry) {
            return t;
        }
    }
    if cu == 2 {
        if let Some(t) = shift_template(opcode) {
            return fill(t, rn, rx, ry);
        }
    }
    if bits(c, 22, 22) == 1 {
        if let Some(t) = multi_text(c) {
            return t;
        }
    }
    let unit = match cu {
        0 => "ALU",
        1 => "MUL",
        2 => "SHIFT",
        _ => "MULTI",
    };
    format!("{unit}?op={opcode:02x} n={rn} x={rx} y={ry}")
}

fn mem_move(w: u64, kind: Kind, iff: &str) -> String {
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

fn type_only(kind: Kind) -> Option<&'static str> {
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

fn raw_hex(parcels: &[u16]) -> String {
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
    fn classify_known_forms() {
        assert_eq!(classify(0x721A, Some(0x003F)), (Kind::T5b, 2));
        assert_eq!(classify(0x1901, None), (Kind::T25, 3));
        assert_eq!(classify(0xC123, None), (Kind::T2c, 1));
        assert_eq!(classify(0x0FA4, Some(0x006C)), (Kind::T17b, 2));
        assert_eq!(classify(0x0F15, Some(0x000C)), (Kind::T17a, 3));
        assert_eq!(classify(0x1486, Some(0x0000)), (Kind::T18a, 3));
        assert_eq!(classify(0x023E, Some(0x2019)), (Kind::T2x, 3));
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
    fn compute_fields() {
        assert_eq!(compute_text(0), "NOP");
        assert_eq!(compute_text(0x0A1880), "F8 = PASS F8");
        assert_eq!(compute_text(0x200312), "R3 = LSHIFT R1 BY R2");
        assert_eq!(compute_text(0x445666), "R5 = R1 * R6 (SSFR), R6 = R9 + R14");
        assert_eq!(
            compute_text(0x635666),
            "R5 = R1 * R6, R6 = R9 + R14, R3 = R9 - R14"
        );
        assert_eq!(compute_text(0x585666), "F5 = F1 * F6, F6 = F9 + F14");
        assert_eq!(
            compute_text((0b0000111 << 16) | 0x4321),
            "R4 = R2 - R1, R3 = R2 + R1"
        );
        assert_eq!(compute_text(0x4F0000), "MULTI?op=f0 n=0 x=0 y=0");
        assert_eq!(mult_text(0x30, 1, 2, 3).as_deref(), Some("F1 = F2 * F3"));
        assert_eq!(
            mult_text(0x78, 0, 1, 2).as_deref(),
            Some("R0 = R1 * R2 (SSF)")
        );
        assert_eq!(mult_text(0x10, 0, 1, 2), None);
        assert_eq!(short_compute_text(0x0804), "F0 = F0 + F4");
    }

    #[test]
    fn register_names_and_bits() {
        assert_eq!(ureg_name(0x24), "M4");
        assert_eq!(ureg_name(0x62), "?");
        assert_eq!(ureg_name(0x73), "MMASK");
        assert_eq!(bits(0xABCD_0000_0000, 47, 40), 0xAB);
        assert_eq!(word48(&[0x1486, 0, 0x20]), 0x1486_0000_0020);
        assert_eq!(word48(&[0xFA4, 0x70]), 0x0FA4_0070_0000);
    }

    #[test]
    fn tile_splits_and_reports_exactness() {
        // 17a (3 parcels) + 17b (2 parcels)
        let p = [0x0F15, 0x000C, 0x0440, 0x0FA4, 0x0070];
        let (insns, exact) = tile(&p);
        assert!(exact && insns.len() == 2);
        assert_eq!(insns[0].kind, Kind::T17a);
        assert_eq!(insns[1].words(), &[0x0FA4, 0x0070]);
        let (_, exact) = tile(&p[..4]);
        assert!(!exact);
        assert_eq!(ureg_load(&insns[0]), Some((0x15, 0x000C_0440)));
        assert_eq!(ureg_load(&insns[1]), Some((0x24, 0x70)));
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
