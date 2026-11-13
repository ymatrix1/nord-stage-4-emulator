//! Biquad coefficient sets read from the Nord Stage 4 firmware: the ARM Amp type switch and the
//! FX DSP master IIR. Derived interoperability data only (float32 words, 5 per section).
//!
//! Sections are transposed direct form II, `[b0, b1, b2, a1, a2]` with `a0 = 1`
//! (`y = b0 x + s1; s1 = b1 x + s2 - a1 y; s2 = b2 x - a2 y`), the form of DSP routine `0x120FDB`.

use super::types::AmpCabinet;

/// Number of cascaded sections the ARM sends per Amp case.
pub(super) const AMP_FW_SECTIONS: usize = 2;
/// Number of cases of the ARM Amp type switch (`Edt_Effects_kAmpTypeNew` 0..9).
pub(super) const AMP_FW_CASES: usize = 10;

/// Identity section (case 4 of the switch).
const ID: [u32; 5] = [0x3F80_0000, 0, 0, 0, 0];

/// Two cascaded coefficient sets per Amp case `0..9` as float32 words.
///
/// VERIFIED: the words the ARM Amp type switch `0xC021D064 (this, case)` of OS v1.66 sends to DM
/// `0xB060B..0xB060F` (set 1) and `0xB0614..0xB0618` (set 2) after the layer constructor
/// `0xC0220E50`, read in the unicorn harness with
/// `uv run --with unicorn --with capstone python tools/fx_chain_install.py amp
/// target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin`
/// (`docs/research/fx-chain-install.md` s5); every word identical to the command output (checked
/// when the table was generated; `amp_table_words_decode_to_the_documented_values` pins samples).
/// Cases 6..9 send no coefficient sets: they are held as identity sections and excluded by
/// [`amp_fw_has_sets`].
pub(super) const AMP_FW_WORDS: [[[u32; 5]; AMP_FW_SECTIONS]; AMP_FW_CASES] = [
    [
        [0x4033_D9B3, 0xC098_543D, 0x4004_2D9E, 0xBFBB_A49A, 0x3F14_C491],
        [0x4030_D78F, 0xC0AA_F8EE, 0x4027_68A7, 0xBFAA_F8EE, 0x3EB0_806C],
    ],
    [
        [0x3F80_0000, 0xBFFC_11CA, 0x3F78_4202, 0xBFFC_11CA, 0x3F78_4202],
        [0x4040_31C8, 0xC0BC_A334, 0x403A_B4A5, 0xBFD7_95F2, 0x3F31_0758],
    ],
    [
        [0x4132_E6C1, 0xC19D_7A98, 0x410B_3F4D, 0xBF90_0CE5, 0x3EB1_A779],
        [0x3F34_9E72, 0xBFB3_1C19, 0x3F31_9CA9, 0xBFFF_DF00, 0x3F7F_C228],
    ],
    [
        [0x3F7E_4439, 0xBFFC_7195, 0x3F7A_C870, 0xBFFC_7195, 0x3F79_0CA9],
        [0x4051_7486, 0xC0CD_C4E0, 0x404B_29D1, 0xBFEB_2A24, 0x3F57_9063],
    ],
    [ID, ID],
    [
        [0x3FD0_C965, 0xC050_C965, 0x3FD0_C965, 0xBFF5_6CE6, 0x3F6B_ACC3],
        [0x3D70_0B71, 0x3DF0_0B71, 0x3D70_0B71, 0xBF9A_0CBA, 0x3EE0_38A1],
    ],
    [ID, ID],
    [ID, ID],
    [ID, ID],
    [ID, ID],
];

/// Case of the ARM Amp switch that sends coefficient sets (`0..=5`).
pub(super) fn amp_fw_has_sets(case: usize) -> bool {
    case <= 5
}

/// Float coefficient sets of firmware Amp case `case` (`None` for cases without sets).
pub(super) fn amp_fw_sets(case: usize) -> Option<[[f32; 5]; AMP_FW_SECTIONS]> {
    amp_fw_has_sets(case).then(|| AMP_FW_WORDS[case].map(|s| s.map(f32::from_bits)))
}

/// Firmware Amp case that voices a Rust cabinet mode, where one is known.
///
/// ASSUMED: only case 5 (high pass followed by low pass, a band-limited speaker) is matched, by
/// shape, to `Small Speaker`; the order of the other cases against the UI names is not decoded
/// (`docs/research/fx-chain-install.md` s5), so every other mode keeps the RBJ voicing.
pub(super) fn amp_fw_case_for(cabinet: AmpCabinet, variation: bool) -> Option<usize> {
    match (cabinet, variation) {
        (AmpCabinet::Small, false) => Some(5),
        _ => None,
    }
}

/// Master IIR: four cascaded sections on the summed bus, float32 words `[b0, b1, b2, a1, a2]`.
///
/// VERIFIED: PM `0x945F2..0x94605` of the FX DSP image (identical in Core1 and Core2), read from
/// the flat SRAM image (`python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --sram dir`, word at byte
/// `4 * address`; 20 words dumped and compared with this table, 0 differing). The numerators are
/// palindromic (`b0 == b2`). The fs/4 Butterworth pair of `0x9437A` is a different table
/// (`FW_BUTTERWORTH_FS4_WORDS`). Section order is the memory order (ASSUMED to be the signal order).
pub(super) const MASTER_IIR_WORDS: [[u32; 5]; 4] = [
    [0x3F05_70C0, 0x3E86_59B2, 0x3F05_70C0, 0xBE3F_F6D3, 0x3F41_4CF5],
    [0x3F27_B1AA, 0x3F05_CCF3, 0x3F27_B1AA, 0xBD44_728B, 0x3ECC_3FA1],
    [0x3EF6_EF68, 0x3F27_7272, 0x3EF6_EF68, 0x3DD7_E720, 0x3E22_3B71],
    [0x3E5F_F7D4, 0x3ED5_9E20, 0x3E5F_F7D4, 0x3E54_046B, 0x3CEE_34FC],
];

/// The master IIR sets as floats.
pub(super) fn master_iir_sets() -> [[f32; 5]; 4] {
    MASTER_IIR_WORDS.map(|s| s.map(f32::from_bits))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Magnitude of a section at `f` Hz for sample rate `fs` (f64 evaluation of `H(z)`).
    pub(in crate::fx) fn mag(c: &[f32; 5], f: f64, fs: f64) -> f64 {
        let [b0, b1, b2, a1, a2] = c.map(f64::from);
        let w = std::f64::consts::TAU * f / fs;
        let (z1r, z1i) = (w.cos(), -w.sin());
        let (z2r, z2i) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let (nr, ni) = (b0 + b1 * z1r + b2 * z2r, b1 * z1i + b2 * z2i);
        let (dr, di) = (1.0 + a1 * z1r + a2 * z2r, a1 * z1i + a2 * z2i);
        nr.hypot(ni) / dr.hypot(di)
    }

    /// Stability triangle: both poles strictly inside the unit circle.
    fn stable(c: &[f32; 5]) -> bool {
        let (a1, a2) = (f64::from(c[3]), f64::from(c[4]));
        a2.abs() < 1.0 && a1.abs() < 1.0 + a2
    }

    #[test]
    fn amp_table_words_decode_to_the_documented_values() {
        // values printed by `fx_chain_install.py amp` (6 significant digits)
        let s0 = amp_fw_sets(0).unwrap();
        assert!((s0[0][0] - 2.81016).abs() < 1e-5 && (s0[1][4] - 0.34473).abs() < 1e-5);
        let s5 = amp_fw_sets(5).unwrap();
        assert!((s5[0][0] - 1.6311).abs() < 1e-4 && (s5[1][0] - 0.0586).abs() < 1e-4);
        assert!((s5[0][3] + 1.9174).abs() < 1e-4 && (s5[1][4] - 0.4379).abs() < 1e-4);
    }

    #[test]
    fn amp_cases_6_to_9_have_no_sets_and_case_4_is_identity() {
        assert!((6..AMP_FW_CASES).all(|c| amp_fw_sets(c).is_none()));
        assert_eq!(amp_fw_sets(4).unwrap(), [[1.0, 0.0, 0.0, 0.0, 0.0]; 2]);
    }

    #[test]
    fn every_amp_and_master_section_is_stable() {
        for case in 0..=5 {
            for s in amp_fw_sets(case).unwrap() {
                assert!(stable(&s), "amp case {case}");
            }
        }
        assert!(master_iir_sets().iter().all(stable));
    }

    #[test]
    fn case_5_is_a_band_limited_high_pass_then_low_pass() {
        let [hp, lp] = amp_fw_sets(5).unwrap();
        // set 1 numerator 1 -2 1 shape (zero at DC), set 2 numerator 1 2 1 shape (zero at Nyquist)
        assert!((hp[0] + hp[1] + hp[2]).abs() < 1e-5);
        assert!((lp[0] - lp[2]).abs() < 1e-9 && (lp[1] - 2.0 * lp[0]).abs() < 1e-5);
        let db = |f: f64| 20.0 * (mag(&hp, f, 48_000.0) * mag(&lp, f, 48_000.0)).log10();
        assert!(db(50.0) < -25.0 && db(100.0) < db(500.0) - 15.0, "low cut");
        assert!(db(20_000.0) < db(1000.0) - 30.0, "high cut");
        assert!(db(1000.0) > 0.0 && db(1000.0) < 8.0, "mid presence");
    }

    #[test]
    fn master_table_is_palindromic_and_starts_with_the_documented_words() {
        assert_eq!(MASTER_IIR_WORDS[0][..3], [0x3F05_70C0, 0x3E86_59B2, 0x3F05_70C0]);
        assert!(MASTER_IIR_WORDS.iter().all(|s| s[0] == s[2]));
    }

    #[test]
    fn master_iir_is_flat_in_band_and_cuts_the_top_octave() {
        let sets = master_iir_sets();
        let g = |f: f64| sets.iter().map(|s| mag(s, f, 48_000.0)).product::<f64>();
        assert!((g(100.0) - 1.0).abs() < 1e-3 && (g(5000.0) - 1.0).abs() < 1e-3);
        assert!(g(20_000.0) < 0.01, "{}", g(20_000.0));
    }

    #[test]
    fn mapping_assumes_only_small_speaker() {
        assert_eq!(amp_fw_case_for(AmpCabinet::Small, false), Some(5));
        assert_eq!(amp_fw_case_for(AmpCabinet::Small, true), None);
        assert_eq!(amp_fw_case_for(AmpCabinet::Twin, false), None);
    }
}
