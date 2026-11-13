//! Strip word 3 to the filter coefficient `R` (opt-in, not wired into the voice render).
//!
//! The pass control block (`pc 0x125F80..0x125FEE`) clips the PEy-lane word (strip word 3 plus the
//! modulation sum, as the pass smoother left it) to +-0x6A000000, takes its top byte as the signed
//! table index `i` and the low 24 bits as the interpolation fraction into the table at
//! `DM(0xB0BA2 + i)`. The table is a ramp: `R(i) = clamp(0.36083 + 0.0079301 i, 0, 1.0111)`.
//! `R = tan(pi fc / fs)` feeds the filter stages of `fw_filter.rs`.
//!
//! VERIFIED: 19 oracle (word, R) pairs, max error below 5e-6 (test `matches_oracle_pairs`).
//! EMPIRICAL: the closed form of the table (max error 1.0e-6 over the 152 entries from i = -50 to
//! 105). Not covered: the other terms of the sum (`w5`, `w7`, LFO) and the ARM value that becomes
//! the word for a given knob position (open: `docs/research/synth-adr-and-strip-law.md`).

/// Clip of the sum (`DM(0xC1479)`).
pub const FW_CUTOFF_CLIP: i32 = 0x6A00_0000;
const R_BASE: f64 = 0.36083;
const R_STEP: f64 = 0.0079301;
const R_MAX: f64 = 1.0111;

/// `R` at an integer table index.
#[inline]
fn r_table(i: i32) -> f64 {
    (R_BASE + R_STEP * f64::from(i)).clamp(0.0, R_MAX)
}

/// `R` for the Q31 strip word 3 value as the pass sees it.
pub fn fw_strip_cutoff_r(word: i32) -> f32 {
    let w = word.clamp(-FW_CUTOFF_CLIP, FW_CUTOFF_CLIP);
    let i = w >> 24;
    let frac = f64::from(w & 0xFF_FFFF) / f64::from(1u32 << 24);
    let lo = r_table(i);
    (lo + (r_table(i + 1) - lo) * frac) as f32
}

/// Cutoff in cycles per sample of the coefficient `r` (`fc / fs = atan(R) / pi`).
#[inline]
pub fn fw_cutoff_cycles_per_sample(r: f32) -> f32 {
    r.atan() / std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    // (word as the pass sees it, R bits) from the oracle sweep (`tools/tests/test_synth_adr_model.py`).
    const PAIRS: [(u32, u32); 19] = [
        (0x9A7F8000, 0x00000000),
        (0xB0640000, 0x00000000),
        (0xC944C000, 0x00000000),
        (0xD03C0000, 0x00000000),
        (0xE36950C7, 0x3E0955BE),
        (0xF135D950, 0x3E79636E),
        (0xFA078000, 0x3EA08043),
        (0x00000000, 0x3EB8BE4D),
        (0x007F6000, 0x3EBAC378),
        (0x0121D950, 0x3EBD5726),
        (0x02FC4000, 0x3EC4DD52),
        (0x121D950B, 0x3F0125F0),
        (0x2FC40000, 0x3F3D574D),
        (0x3FB00000, 0x3F5DAA05),
        (0x4F9C0000, 0x3F7DFCBD),
        (0x598F8000, 0x3F816BB1),
        (0x65808000, 0x3F816BB1),
        (0x7F5FFFFF, 0x3F816BB1),
        (0x7EC0C7FF, 0x3F816BB1),
    ];

    #[test]
    fn matches_oracle_pairs() {
        for (w, bits) in PAIRS {
            let got = fw_strip_cutoff_r(w as i32);
            let want = f32::from_bits(bits);
            assert!((got - want).abs() < 5e-6, "{w:08x}: {got} vs {want}");
        }
    }

    #[test]
    fn limits_and_warp() {
        assert_eq!(fw_strip_cutoff_r(i32::MIN), 0.0);
        assert!((fw_strip_cutoff_r(0) - 0.36083).abs() < 1e-6);
        assert!((fw_strip_cutoff_r(i32::MAX) - 1.0111).abs() < 1e-6);
        assert!((fw_cutoff_cycles_per_sample(1.0) - 0.25).abs() < 1e-6);
    }
}
