//! Named fields of an SY1SY2 voice record for the `voice` scenario command.
//!
//! Word offsets are relative to the record base `0x92400 + 0x38 k`. Provenance: the offsets come
//! from the Stage B routine `0x1C10EE` (`I3 = base + 0x26`, accesses `DM(q, I3)`, so word `0x26 + q`)
//! and were confirmed by poking each word of a record and watching the output
//! (`docs/research/oracle-vs-engine.md` section 3). Fields that move values (`pos`, `step`) are raw
//! 32-bit words: the sample position has 25 fraction bits (see the document).
use crate::script::{parse_number, parse_value, REC_BASE, REC_STRIDE};

/// Kernel table base and block size (`docs/research/sy-dsp-map.md` section 2).
pub const KERNEL_BASE: u32 = 0xE3400;
pub const KERNEL_STRIDE: u32 = 0xC00;
/// Stage B resampler entry that Stage A installs into record word `0x15`.
pub const STAGE_B: u32 = 0x1C10EE;

/// (name, word offset in the record).
const FIELDS: [(&str, u32); 8] = [
    ("gain_l", 0x08),
    ("gain_r", 0x09),
    ("stage_b", 0x15),
    ("pos", 0x2B),
    ("step", 0x2D),
    ("buf", 0x31),
    ("m4", 0x33),
    ("kernel_ptr", 0x3B),
];

/// Data pokes `(address, value)` for `voice K name=value...`.
///
/// Besides the table fields: `kernel=N` (index 0..5, sets `kernel_ptr`), `gain=G` (both channels),
/// `stage_b=on` (installs `0x1C10EE`), `ratio=R` (source frames per output frame, sets `step` to
/// `R * 2^24` because one call advances the position by two steps), `pos_frames=P` (position in
/// frames, `P * 2^25`, relative to the centre tap) and `wN=value` for a raw word.
pub fn voice_pokes(k: u32, fields: &[(String, String)]) -> Result<Vec<(u32, u32)>, String> {
    let base = REC_BASE + REC_STRIDE * k;
    let mut out = Vec::new();
    let float = |v: &str| v.parse::<f64>().map_err(|_| format!("bad number {v:?}"));
    for (name, value) in fields {
        let (off, word) = match name.as_str() {
            "kernel" => {
                let n = parse_number(value)
                    .filter(|n| *n < 6)
                    .ok_or("kernel is 0..5")? as u32;
                (0x3B, KERNEL_BASE + KERNEL_STRIDE * n)
            }
            "gain" => {
                let g = (float(value)? as f32).to_bits();
                out.push((base + 0x08, g));
                (0x09, g)
            }
            "stage_b" if value == "on" => (0x15, STAGE_B),
            "ratio" => (0x2D, (float(value)? * f64::from(1u32 << 24)).round() as u32),
            "pos_frames" => (
                0x2B,
                (float(value)? * f64::from(1u32 << 25)).round() as i64 as u32,
            ),
            _ => match FIELDS.iter().find(|f| f.0 == name) {
                Some(f) => (f.1, parse_value(value)?),
                None => (
                    name.strip_prefix('w')
                        .and_then(parse_number)
                        .ok_or_else(|| format!("unknown voice field {name:?}"))?
                        as u32,
                    parse_value(value)?,
                ),
            },
        };
        out.push((base + off, word));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn named_and_raw_fields_map_to_record_words() {
        let p = voice_pokes(1, &f(&[("kernel_ptr", "0xe3400"), ("w5", "f1.0")])).unwrap();
        assert_eq!(p[0], (0x92400 + 0x38 + 0x3B, 0xe3400));
        assert_eq!(p[1], (0x92400 + 0x38 + 5, 0x3f80_0000));
        assert!(voice_pokes(0, &f(&[("bogus", "1")])).is_err());
    }

    #[test]
    fn convenience_fields_expand() {
        let p = voice_pokes(
            0,
            &f(&[
                ("kernel", "2"),
                ("gain", "0.5"),
                ("stage_b", "on"),
                ("ratio", "1.0"),
                ("pos_frames", "0.5"),
            ]),
        )
        .unwrap();
        assert!(p.contains(&(0x92400 + 0x3B, 0xE3400 + 2 * 0xC00)));
        assert!(p.contains(&(0x92408, 0.5f32.to_bits())));
        assert!(p.contains(&(0x92409, 0.5f32.to_bits())));
        assert!(p.contains(&(0x92415, 0x1C10EE)));
        assert!(p.contains(&(0x9242D, 0x0100_0000)));
        assert!(p.contains(&(0x9242B, 0x0100_0000)));
        assert!(voice_pokes(0, &f(&[("kernel", "6")])).is_err());
        assert!(voice_pokes(0, &f(&[("ratio", "x")])).is_err());
    }
}
