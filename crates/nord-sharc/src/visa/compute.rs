//! Text of compute fields: ALU, shifter, multiplier, multifunction and short compute.
use super::classify::bits;

/// Tables 12-3 and 12-4 (single-function ALU): `opcode -> template` with `{n}`, `{x}`, `{y}`.
pub(super) fn alu_template(opcode: u64) -> Option<&'static str> {
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
pub(super) fn shift_template(opcode: u64) -> Option<&'static str> {
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
pub(super) fn mulalu_template(opcode: u64) -> Option<&'static str> {
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

pub(super) const SHORT_OPS: [&str; 16] = [
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

pub(super) fn fill(template: &str, n: u64, x: u64, y: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
