//! `OrganLayer` glue of the Vox tone source (`ToneWheelSource::Vox`, model `Vox`).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. The held keys select the oscillator weights,
//! the vibrato / chorus setting selects the vibrato depth of the bank's pitch LFO (the bank has no
//! delay-line scanner), and the key click and the rotary speaker stay as in the other paths.
//! The drawbars and the percussion do not act on this source (the Vox footage to weight map is
//! not decoded, `docs/research/pno-full-pass.md` section 6).

use super::types::ScannerMode;
use super::vibrato_bank::clamp_fine_tune;
use super::vox_bank::NOTES;
use super::{OrganLayer, ToneWheelSource};

/// VERIFIED: the three vibrato depth words the ARM writes to `DM(0xE20E3)` for the Vox vibrato
/// types, `0xE464 << 8`, `0x1782D << 8`, `0x240B7 << 8` as fractions of `2^31`: 0.70 %, 1.15 %,
/// 1.76 % peak pitch deviation (`docs/research/organ-post-stage.md` section 3.1).
pub(super) const VOX_DEPTHS: [f32; 3] = [
    (0xE464u32 << 8) as f32 / 2_147_483_648.0,
    (0x1782Du32 << 8) as f32 / 2_147_483_648.0,
    (0x240B7u32 << 8) as f32 / 2_147_483_648.0,
];

/// Vibrato depth (pitch fraction) for the layer's vibrato / chorus setting. ASSUMED: V1..V3 map
/// to the three Vox depths in order, and C1..C3 to the same three (the bank has one LFO and no
/// chorus path).
pub(super) fn vox_depth(mode: ScannerMode) -> f32 {
    match mode {
        ScannerMode::Off => 0.0,
        ScannerMode::V1 | ScannerMode::C1 => VOX_DEPTHS[0],
        ScannerMode::V2 | ScannerMode::C2 => VOX_DEPTHS[1],
        ScannerMode::V3 | ScannerMode::C3 => VOX_DEPTHS[2],
    }
}

impl OrganLayer {
    /// `true` when the Vox bank is the tone source in force.
    pub(super) fn vox_active(&self) -> bool {
        self.effective_tone_source() == ToneWheelSource::Vox
    }

    /// One stereo frame of the Vox path: bank and output stage, key click, rotary speaker.
    pub(super) fn render_vox_frame(&mut self, inv_fs: f32) -> [f32; 2] {
        if self.vox.sample_rate() != self.sample_rate {
            self.vox.set_sample_rate(self.sample_rate);
        }
        self.vox
            .set_fine_cents(clamp_fine_tune(self.fine_tune_cents));
        // oscillator k plays MIDI note k + 24 (ASSUMED map, docs/research/pno-full-pass.md s4)
        let shift = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        let mut targets = [0.0f32; NOTES];
        for (k, t) in targets.iter_mut().enumerate() {
            let note = k as i32 + 24 - shift;
            if (0..128).contains(&note) && self.held_notes[note as usize] {
                *t = self.note_gains[note as usize] * self.weight_level;
            }
        }
        let dry = self.vox.render(&targets, vox_depth(self.scanner_mode));
        if self.active_key_count == 0
            && self.vox.is_idle()
            && self.click_env < 1e-6
            && self.click_bp_state[0].abs() < 1e-6
        {
            return [0.0, 0.0];
        }
        let dry = dry + self.process_key_click(inv_fs);
        if self.rotary_enabled {
            self.process_rotary(dry, inv_fs)
        } else {
            [dry, dry]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organ::OrganModel;

    fn vox_layer() -> OrganLayer {
        let mut o = OrganLayer::new(48_000.0);
        o.model = OrganModel::Vox;
        o.tonewheel_source = ToneWheelSource::Vox;
        o.rotary_enabled = false;
        o.key_click_enabled = false;
        o
    }

    #[test]
    fn depths_are_the_verified_peak_pitch_fractions() {
        assert!((VOX_DEPTHS[0] - 0.00697).abs() < 1e-5);
        assert!((VOX_DEPTHS[1] - 0.01148).abs() < 1e-5);
        assert!((VOX_DEPTHS[2] - 0.01760).abs() < 1e-5);
        assert_eq!(vox_depth(ScannerMode::Off), 0.0);
        assert_eq!(vox_depth(ScannerMode::C2), VOX_DEPTHS[1]);
        assert_eq!(vox_depth(ScannerMode::V3), VOX_DEPTHS[2]);
    }

    #[test]
    fn explicit_source_only_applies_to_the_vox_model() {
        let mut o = vox_layer();
        assert!(o.vox_active());
        o.model = OrganModel::B3;
        assert!(!o.vox_active());
        assert_eq!(OrganLayer::new(48_000.0).effective_tone_source(), {
            // the default model keeps its legacy source
            ToneWheelSource::Legacy
        });
    }

    #[test]
    fn silent_without_keys_and_audible_with_a_key() {
        let mut o = vox_layer();
        for _ in 0..2000 {
            assert_eq!(o.render_frame(), [0.0, 0.0]);
        }
        o.note_on(60);
        let mut peak = 0.0f32;
        for _ in 0..24_000 {
            let [l, r] = o.render_frame();
            assert!(l.is_finite() && l == r);
            peak = peak.max(l.abs());
        }
        assert!(peak > 0.02 && peak < 0.2, "peak {peak}");
        o.note_off(60);
        let mut last = [1.0, 1.0];
        for _ in 0..300_000 {
            last = o.render_frame();
        }
        assert_eq!(last, [0.0, 0.0]);
    }

    #[test]
    fn octave_shift_moves_the_oscillator_and_vibrato_follows_the_scanner_setting() {
        let render = |shift: i8, mode: ScannerMode| {
            let mut o = vox_layer();
            o.octave_shift = shift;
            o.scanner_mode = mode;
            o.note_on(60);
            (0..9600).map(|_| o.render_frame()[0]).collect::<Vec<_>>()
        };
        let plain = render(0, ScannerMode::Off);
        let shifted = render(1, ScannerMode::Off);
        let vib = render(0, ScannerMode::V3);
        let diff = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>();
        assert!(
            diff(&plain, &shifted) > 10.0,
            "octave shift changes the note"
        );
        assert!(diff(&plain, &vib) > 5.0, "vibrato changes the waveform");
        assert_eq!(plain, render(0, ScannerMode::Off), "deterministic");
    }

    #[test]
    fn keys_outside_the_oscillator_range_stay_silent() {
        let mut o = vox_layer();
        o.note_on(10);
        o.note_on(120);
        for _ in 0..4000 {
            assert_eq!(o.render_frame(), [0.0, 0.0]);
        }
    }

    #[test]
    fn rotary_and_key_click_stay_finite() {
        let mut o = vox_layer();
        o.rotary_enabled = true;
        o.key_click_enabled = true;
        o.note_on(48);
        o.note_on(67);
        for _ in 0..24_000 {
            let [l, r] = o.render_frame();
            assert!(l.is_finite() && r.is_finite() && l.abs() < 2.0);
        }
    }
}
