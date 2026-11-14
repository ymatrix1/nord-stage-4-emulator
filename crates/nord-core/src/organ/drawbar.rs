//! Drawbar bus mapping: semitone offsets, level law and top/bottom foldback.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.2.

use super::types::NUM_TONEWHEELS;

/// Semitone offsets relative to fundamental (`8'`) for the 9 drawbar buses.
///
/// VERIFIED: the footage layout (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1') equals the
/// wheel the booted ARM key engine picks for each drawbar: `wheel = note - 24 + offset` for all
/// 128 notes x 9 drawbars (1152 of 1152 indices), read from the engine item tables and confirmed
/// by the `wB` messages (`python tools/arm_organ_weights.py keymap|verify`,
/// `docs/research/organ-weights.md` section 3), max err 0.
pub(super) const DRAWBAR_OFFSETS: [i32; 9] = [-12, 7, 0, 12, 19, 24, 28, 31, 36];

/// Drawbar level law selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawbarLaw {
    /// The firmware 9-entry curve `FIRMWARE_DRAWBAR_CURVE / 127` (default).
    #[default]
    Firmware,
    /// The previous law `level / 8`, kept for A/B comparison.
    Linear,
}

/// 7-bit value the ARM record update `0xC00BE348` maps each drawbar edit value `0..=8` to.
///
/// VERIFIED: integer table at `bank0_CODE@0xC02B0EC0` (`0, 3, 6, 13, 22, 39, 62, 87, 127`), read
/// from the image and confirmed by an ARM run (`python tools/arm_bank.py`,
/// `docs/research/arm-organ-messages.md` section 7). That this array is the final gain seen by
/// the DSP (`value / 127`) is ASSUMED: the sender of the array was not found.
pub const FIRMWARE_DRAWBAR_CURVE: [u8; 9] = [0, 3, 6, 13, 22, 39, 62, 87, 127];

/// Gain of a drawbar position `0..=8` (values above 8 clamp to 8) under the default firmware law.
///
/// `FIRMWARE_DRAWBAR_CURVE[level] / 127`: about -3 dB per step at the top, steeper below (TD-10).
pub fn drawbar_gain(level: u8) -> f32 {
    drawbar_gain_law(level, DrawbarLaw::Firmware)
}

/// Gain of a drawbar position `0..=8` under an explicit [`DrawbarLaw`].
///
/// `Linear` is `level / 8`: ASSUMED, superseded as default; it conflicts with the firmware curve.
pub fn drawbar_gain_law(level: u8, law: DrawbarLaw) -> f32 {
    let l = level.min(8);
    match law {
        DrawbarLaw::Firmware => f32::from(FIRMWARE_DRAWBAR_CURVE[usize::from(l)]) / 127.0,
        DrawbarLaw::Linear => f32::from(l) / 8.0,
    }
}

/// Maps a target MIDI pitch with B3 top/bottom octave foldback into tonewheel `0..90`.
///
/// VERIFIED: tonewheel 0 = MIDI 24, wheels below 0 move up and wheels above 90 move down by whole
/// octaves; identical to the firmware key tables for all 128 x 9 entries (`FOLD_TOP = 90` is the
/// only threshold that fits, `docs/research/organ-weights.md` section 3). Wheel 91 is never used.
pub(super) fn foldback_tonewheel(target_midi: i32) -> usize {
    let mut m = target_midi;
    while m < 24 {
        m += 12;
    }
    while m > 24 + (NUM_TONEWHEELS as i32 - 1) {
        m -= 12;
    }
    (m - 24).clamp(0, NUM_TONEWHEELS as i32 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foldback_ranges() {
        assert_eq!(foldback_tonewheel(12), 0);
        assert_eq!(foldback_tonewheel(24), 0);
        assert_eq!(foldback_tonewheel(69), 45);
        assert!(foldback_tonewheel(120) < NUM_TONEWHEELS);
        assert!(foldback_tonewheel(-40) < NUM_TONEWHEELS);
    }

    /// Rows of `docs/research/data/organ-key-wheel-map.csv` (wheel per drawbar of the firmware key
    /// engine) for the lowest, a middle and the highest note.
    #[test]
    fn matches_the_firmware_key_tables() {
        let rows: [(i32, [usize; 9]); 3] = [
            (0, [0, 7, 0, 0, 7, 0, 4, 7, 12]),
            (60, [24, 43, 36, 48, 55, 60, 64, 67, 72]),
            (127, [79, 86, 79, 79, 86, 79, 83, 86, 79]),
        ];
        for (note, wheels) in rows {
            for (j, &want) in wheels.iter().enumerate() {
                assert_eq!(
                    foldback_tonewheel(note + DRAWBAR_OFFSETS[j]),
                    want,
                    "note {note} drawbar {j}"
                );
            }
        }
    }

    #[test]
    fn linear_law_is_linear_and_clamped() {
        assert_eq!(drawbar_gain_law(0, DrawbarLaw::Linear), 0.0);
        assert_eq!(drawbar_gain_law(4, DrawbarLaw::Linear), 0.5);
        assert_eq!(drawbar_gain_law(8, DrawbarLaw::Linear), 1.0);
        assert_eq!(drawbar_gain_law(15, DrawbarLaw::Linear), 1.0);
        assert_eq!(DRAWBAR_OFFSETS[2], 0);
    }

    #[test]
    fn firmware_curve_values_and_clamp() {
        assert_eq!(FIRMWARE_DRAWBAR_CURVE, [0, 3, 6, 13, 22, 39, 62, 87, 127]);
        assert_eq!(drawbar_gain(0), 0.0);
        assert_eq!(drawbar_gain(8), 1.0);
        assert_eq!(drawbar_gain(4), 22.0 / 127.0);
        assert_eq!(drawbar_gain(15), 1.0);
        assert_eq!(DrawbarLaw::default(), DrawbarLaw::Firmware);
        assert!(FIRMWARE_DRAWBAR_CURVE.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn firmware_curve_db_levels() {
        // dB re 127 as listed in docs/research/arm-organ-messages.md section 7
        let want = [-32.5, -26.5, -19.8, -15.2, -10.3, -6.2, -3.3, 0.0];
        for (i, w) in want.iter().enumerate() {
            let db = 20.0 * f64::from(drawbar_gain(i as u8 + 1)).log10();
            assert!((db - w).abs() < 0.06, "level {}: {db} vs {w}", i + 1);
        }
    }
}
