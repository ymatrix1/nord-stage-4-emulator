//! Synth oscillator selection: `0x2F` type/category/wave to engine oscillator.
//!
//! Spec: `docs/specs/cbin-and-presets.md` (Synth Block). Name lists come from the
//! third-party ns4decode map (MIT, `.reference/ns4decode_pkg/ns4names.py`).

use crate::synth::{SynthOscMode, VaWaveform};

/// Sizes of the WAVE-type categories (`BELLS/TINES`, `ACOUSTIC`, `DIGITAL`, `ORGAN`, `KEYS`).
/// EMPIRICAL: name lists from ns4decode `ns4names.SynthWave`; category index is the 3b field at
/// `0x2F +15`; checked by `test_synth_wave_flat_index_roundtrip`.
pub(super) const WAVE_CATEGORY_SIZES: [u8; 5] = [6, 6, 14, 10, 10];

/// Flat index `0..46` over all WAVE categories. ASSUMED: this order maps 1:1 to our 64-table
/// wavetable bank (the bank contents are synthetic, not the hardware waves).
pub(super) fn wave_flat_index(category: u8, wave: u8) -> u8 {
    let cat = usize::from(category).min(WAVE_CATEGORY_SIZES.len() - 1);
    let base: u8 = WAVE_CATEGORY_SIZES[..cat].iter().sum();
    base + wave.min(WAVE_CATEGORY_SIZES[cat] - 1)
}

pub(super) fn wave_from_flat_index(flat: u8) -> (u8, u8) {
    let mut left = flat;
    for (cat, &size) in WAVE_CATEGORY_SIZES.iter().enumerate() {
        if left < size {
            return (cat as u8, left);
        }
        left -= size;
    }
    let last = WAVE_CATEGORY_SIZES.len() - 1;
    (last as u8, WAVE_CATEGORY_SIZES[last] - 1)
}

/// Maps `0x2F` type/category/wave to the engine oscillator. The bit fields are EMPIRICAL (third-party decoder, not firmware);
/// ANALOG categories without a dedicated engine voice (`SUB OSC`, `SYNC`, `SHAPE*`, `MISC`,
/// noise) are ASSUMED approximations onto the existing VA waveforms.
pub(super) fn map_osc_mode_and_wave(
    is_osc_mode: bool,
    osc_type: u8,
    osc_category: u8,
    osc_wave: u8,
) -> (SynthOscMode, VaWaveform, u8) {
    if !is_osc_mode {
        return (SynthOscMode::Sample, VaWaveform::Saw, osc_wave);
    }
    match osc_type {
        1 | 2 => (SynthOscMode::Fm, VaWaveform::Saw, osc_wave),
        3 => (
            SynthOscMode::Wavetable,
            VaWaveform::Saw,
            wave_flat_index(osc_category, osc_wave),
        ),
        _ => {
            let waveform = match (osc_category, osc_wave) {
                (0, 0) | (4, _) => VaWaveform::Sine,
                (0, 1) | (3, 0) => VaWaveform::Triangle,
                (0, 3..=5) | (3, 2) => VaWaveform::Pulse,
                (5, _) | (6, _) => VaWaveform::SuperSaw,
                _ => VaWaveform::Saw,
            };
            (SynthOscMode::Analog, waveform, osc_wave)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synth_wave_flat_index_roundtrip() {
        // 6 + 6 + 14 + 10 + 10 = 46 WAVE-type waves.
        assert_eq!(
            WAVE_CATEGORY_SIZES
                .iter()
                .map(|&n| u32::from(n))
                .sum::<u32>(),
            46
        );
        assert_eq!(wave_flat_index(0, 0), 0);
        assert_eq!(wave_flat_index(2, 0), 12);
        assert_eq!(wave_flat_index(4, 9), 45);
        // out-of-range wave clamps inside its category
        assert_eq!(wave_flat_index(0, 63), 5);
        for flat in 0..46u8 {
            let (cat, wave) = wave_from_flat_index(flat);
            assert_eq!(wave_flat_index(cat, wave), flat);
        }
        assert_eq!(wave_from_flat_index(200), (4, 9));
    }

    #[test]
    fn test_map_osc_selects_engine_oscillator_by_type() {
        assert_eq!(
            map_osc_mode_and_wave(false, 0, 0, 5),
            (SynthOscMode::Sample, VaWaveform::Saw, 5)
        );
        assert_eq!(map_osc_mode_and_wave(true, 1, 3, 7).0, SynthOscMode::Fm);
        assert_eq!(map_osc_mode_and_wave(true, 2, 0, 12).0, SynthOscMode::Fm);
        assert_eq!(
            map_osc_mode_and_wave(true, 3, 2, 3),
            (SynthOscMode::Wavetable, VaWaveform::Saw, 15)
        );
    }

    #[test]
    fn test_map_osc_analog_categories_to_va_waveforms() {
        let wf = |cat, wave| map_osc_mode_and_wave(true, 0, cat, wave).1;
        assert_eq!(wf(0, 0), VaWaveform::Sine);
        assert_eq!(wf(0, 1), VaWaveform::Triangle);
        assert_eq!(wf(0, 2), VaWaveform::Saw);
        assert_eq!(wf(0, 3), VaWaveform::Pulse);
        assert_eq!(wf(0, 4), VaWaveform::Pulse);
        assert_eq!(wf(0, 5), VaWaveform::Pulse);
        assert_eq!(wf(3, 0), VaWaveform::Triangle);
        assert_eq!(wf(3, 1), VaWaveform::Saw);
        assert_eq!(wf(3, 2), VaWaveform::Pulse);
        assert_eq!(wf(4, 6), VaWaveform::Sine);
        assert_eq!(wf(5, 0), VaWaveform::SuperSaw);
        assert_eq!(wf(6, 4), VaWaveform::SuperSaw);
        assert_eq!(wf(1, 0), VaWaveform::Saw);
        assert_eq!(wf(7, 2), VaWaveform::Saw);
    }
}
