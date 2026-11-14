//! Mid band words of the firmware Amp EQ (`kAmpMid`, `kAmpMidFreq`), piano unit of the settled
//! firmware, and the interpolation over the mid frequency index.
//!
//! VERIFIED words: every table entry is the float32 word the ARM sent to the tone stack context
//! `0xC04438D8#2` (FX core 2, routine `0x1243F4`) for the listed setting, from the series of
//! `docs/research/data/fx-param-fits.csv` (`tools/fx_clients.py --slots`, ids `Edt_Effects_kAmpMid`
//! and `Edt_Effects_kAmpMidFreq`, Amp type 0). The organ unit's words (`0xC02BB2C4#1`) are the
//! same 13 + 11 values (compared in `docs/research/fx-eq-words.md` s2). The synth unit differs (its
//! words are about half, `B` 0.0706 against 0.1378 at the default): not used here. Roles (`B`
//! step, `C` damping, `D` band mix) are EMPIRICAL (word position in the routine's equation, s5.3
//! of `docs/research/fx-contexts.md`).
//!
//! ASSUMED: the series change one id at a time from the default state, so the joint setting
//! (gain and frequency both away from default) is the product form `B = Bf(f) Bg(g) / Bg(0)`,
//! `C = Cf(f) Cg(g) / Cg(0)`, `D = Dg(g)`; and the frequency words between the 11 captured
//! indices are interpolated linearly in `ln` (`B`) and linearly (`C`). `tools/fx_eq_capture.py`
//! captures every index and a joint grid to replace both.

use super::fw_map::tone_table;

/// Mid frequency indices of the captured series (`kAmpMidFreq`, 0..=127, default 64).
pub(super) const FREQ_INDICES: [u8; 11] = [0, 1, 2, 15, 31, 47, 63, 79, 95, 111, 127];

/// `B` (state step, `c[-0x1F]`) against the frequency index.
const MID_B_FREQ: [f32; 11] = [
    f32::from_bits(0x3CEAC900), // 0
    f32::from_bits(0x3CF0B3C0), // 1
    f32::from_bits(0x3CF6C440), // 2
    f32::from_bits(0x3D2A6520), // 15
    f32::from_bits(0x3D7CF860), // 31
    f32::from_bits(0x3DBB22F0), // 47
    f32::from_bits(0x3E09BFC0), // 63
    f32::from_bits(0x3E60ABF0), // 79
    f32::from_bits(0x3EB53D5C), // 95
    f32::from_bits(0x3F0E9356), // 111
    f32::from_bits(0x3F5A4662), // 127
];
/// `B` against the gain counts (13 points of `TONE_COUNTS`, flat above 0).
const MID_B_GAIN: [f32; 13] = [
    f32::from_bits(0x3DEC2360), // -60
    f32::from_bits(0x3DED5E90), // -59
    f32::from_bits(0x3DEE9590), // -58
    f32::from_bits(0x3DFCD940), // -45
    f32::from_bits(0x3E04E7E8), // -30
    f32::from_bits(0x3E09B0D0), // -15
    f32::from_bits(0x3E0CE610), // -1
    f32::from_bits(0x3E0D16E8), // 0
    f32::from_bits(0x3E0D16E8), // 1
    f32::from_bits(0x3E0D16E8), // 15
    f32::from_bits(0x3E0D16E8), // 30
    f32::from_bits(0x3E0D16E8), // 45
    f32::from_bits(0x3E0D16E8), // 60
];
/// `C` (damping, `c[-0x17]`) against the frequency index.
const MID_C_FREQ: [f32; 11] = [
    f32::from_bits(0x3F332C90), // 0
    f32::from_bits(0x3F33209A), // 1
    f32::from_bits(0x3F331458), // 2
    f32::from_bits(0x3F3255EC), // 15
    f32::from_bits(0x3F310596), // 31
    f32::from_bits(0x3F2F1340), // 47
    f32::from_bits(0x3F2C341E), // 63
    f32::from_bits(0x3F266E30), // 79
    f32::from_bits(0x3F1CF096), // 95
    f32::from_bits(0x3F0DD494), // 111
    f32::from_bits(0x3EEC0ED8), // 127
];
/// `C` against the gain counts (flat above 0).
const MID_C_GAIN: [f32; 13] = [
    f32::from_bits(0x404A5596), // -60
    f32::from_bits(0x40459DD3), // -59
    f32::from_bits(0x4040FD8B), // -58
    f32::from_bits(0x400CB0BB), // -45
    f32::from_bits(0x3FC0178D), // -30
    f32::from_bits(0x3F813B5F), // -15
    f32::from_bits(0x3F30C7D4), // -1
    f32::from_bits(0x3F2BFC18), // 0
    f32::from_bits(0x3F2BFC18), // 1
    f32::from_bits(0x3F2BFC18), // 15
    f32::from_bits(0x3F2BFC18), // 30
    f32::from_bits(0x3F2BFC18), // 45
    f32::from_bits(0x3F2BFC18), // 60
];
/// `D` (band mix, `c[-0x18]`) against the gain counts.
const MID_D_GAIN: [f32; 13] = [
    f32::from_bits(0xC0265A7C), // -60
    f32::from_bits(0xC02172B3), // -59
    f32::from_bits(0xC01CA308), // -58
    f32::from_bits(0xBFCC539E), // -45
    f32::from_bits(0xBF5E2CC4), // -30
    f32::from_bits(0xBEB53E8C), // -15
    f32::from_bits(0xBCA0E880), // -1
    f32::from_bits(0x00000000), // 0
    f32::from_bits(0x3C9DC580), // 1
    f32::from_bits(0x3EB63148), // 15
    f32::from_bits(0x3F666D22), // 30
    f32::from_bits(0x3FDDC266), // 45
    f32::from_bits(0x40405A26), // 60
];

/// Mid band words `(B, C, D)` for a gain in counts at the default frequency (piano unit).
pub(super) fn gain_words(counts: f32) -> (f32, f32, f32) {
    (
        tone_table(&MID_B_GAIN, counts),
        tone_table(&MID_C_GAIN, counts),
        tone_table(&MID_D_GAIN, counts),
    )
}

/// Mid band words `(B, C)` for a frequency index at gain 0: `B` interpolated in `ln`, `C` linear.
pub(super) fn freq_words(index: f32) -> (f32, f32) {
    let x = index.clamp(0.0, 127.0);
    let hi = FREQ_INDICES
        .iter()
        .position(|&i| f32::from(i) >= x)
        .unwrap_or(10)
        .max(1);
    let (x0, x1) = (f32::from(FREQ_INDICES[hi - 1]), f32::from(FREQ_INDICES[hi]));
    let t = (x - x0) / (x1 - x0);
    let b = (MID_B_FREQ[hi - 1].ln() * (1.0 - t) + MID_B_FREQ[hi].ln() * t).exp();
    (
        b,
        MID_C_FREQ[hi - 1] + t * (MID_C_FREQ[hi] - MID_C_FREQ[hi - 1]),
    )
}

/// Joint words `(B, C, D)` of gain `counts` at frequency index `index` (product form, ASSUMED).
pub(super) fn words(counts: f32, index: f32) -> (f32, f32, f32) {
    let (bf, cf) = freq_words(index);
    let (bg, cg, dg) = gain_words(counts);
    let (b0, c0, _) = gain_words(0.0);
    (bf * bg / b0, cf * cg / c0, dg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_points_are_returned_exactly() {
        let (b, c, d) = gain_words(0.0);
        assert_eq!((b, c, d), (MID_B_GAIN[7], MID_C_GAIN[7], 0.0));
        assert!((b - 0.137_78).abs() < 1e-5, "default B");
        assert_eq!(freq_words(127.0), (MID_B_FREQ[10], MID_C_FREQ[10]));
        assert!(
            (freq_words(0.0).0 - 0.028_660_3).abs() < 1e-6,
            "B at index 0"
        );
        assert!((gain_words(60.0).2 - 3.0055).abs() < 1e-3, "D at +60");
        assert!((gain_words(-60.0).2 + 2.5993).abs() < 1e-3, "D at -60");
    }

    #[test]
    fn frequency_interpolation_is_monotone_and_hits_the_default() {
        let mut last = 0.0;
        for i in 0..=127 {
            let (b, _) = freq_words(i as f32);
            assert!(b > last, "B rises with the index at {i}");
            last = b;
        }
        // the gain series was captured at index 64: its B word is within 1 % of the interpolation
        let (b64, _) = freq_words(64.0);
        assert!((b64 / MID_B_GAIN[7] - 1.0).abs() < 0.01, "{b64}");
    }

    #[test]
    fn joint_words_reduce_to_the_single_series() {
        let (b, c, d) = words(-30.0, 63.0);
        let (bg, cg, dg) = gain_words(-30.0);
        let (b0, c0, _) = gain_words(0.0);
        let (bf, cf) = freq_words(63.0);
        assert!((b - bf * bg / b0).abs() < 1e-7 && (c - cf * cg / c0).abs() < 1e-7);
        assert_eq!(d, dg);
        assert_eq!(words(0.0, 63.0).2, 0.0, "gain 0 mixes nothing");
        let (bf, _) = freq_words(79.0);
        assert!(
            (words(0.0, 79.0).0 - bf).abs() < 1e-7,
            "gain 0 keeps the frequency word"
        );
    }
}
