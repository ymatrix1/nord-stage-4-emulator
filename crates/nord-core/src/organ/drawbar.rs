//! Drawbar bus mapping: semitone offsets, level law and top/bottom foldback.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.2.

use super::types::NUM_TONEWHEELS;

/// Semitone offsets relative to fundamental (`8'`) for the 9 drawbar buses.
///
/// ASSUMED: textbook Hammond footage layout (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1').
/// The firmware weight writer (`0x9566E..0x95949`) is not decoded
/// (`docs/research/organ-tonewheel-bank.md` section 6).
pub(super) const DRAWBAR_OFFSETS: [i32; 9] = [-12, 7, 0, 12, 19, 24, 28, 31, 36];

/// Linear gain of a drawbar position `0..=8` (values above 8 clamp to 8).
///
/// ASSUMED: `level / 8`. Unverified against the firmware and in conflict with the -3 dB per notch
/// rule of `docs/specs/organ-engine.md` 3.2; the preset field is 4 bits `0..=8`
/// (`docs/research/preset-dsp-map.md`). Tracked as TD-10 in `docs/TECH_DEBT.md`. This is the
/// single place to change when the ARM-side drawbar-to-weight mapping is decoded.
pub fn drawbar_gain(level: u8) -> f32 {
    f32::from(level.min(8)) / 8.0
}

/// Maps a target MIDI pitch with B3 top/bottom octave foldback into tonewheel `0..90`.
///
/// ASSUMED: tonewheel 0 = MIDI 24 and octave foldback at both ends (Hammond practice).
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

    #[test]
    fn drawbar_law_is_linear_and_clamped() {
        assert_eq!(drawbar_gain(0), 0.0);
        assert_eq!(drawbar_gain(4), 0.5);
        assert_eq!(drawbar_gain(8), 1.0);
        assert_eq!(drawbar_gain(15), 1.0);
        assert_eq!(DRAWBAR_OFFSETS[2], 0);
    }
}
