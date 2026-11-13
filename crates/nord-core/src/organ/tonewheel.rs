//! Tonewheel frequencies, pickup calibration and the 91-wheel generator.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1. `B3` with the opt-in
//! [`ToneWheelSource::Firmware`] reads the PNO Core1 bank arithmetic (`firmware_bank.rs`, algorithm
//! VERIFIED, organ role ASSUMED/disputed); the default, and every other model, keeps the
//! float-phase sine path.

use super::types::{HAMMOND_GEAR_RATIOS, HAMMOND_MOTOR_HZ, NUM_TONEWHEELS};
use super::{OrganLayer, OrganModel, ToneWheelSource};
use std::f32::consts::TAU;

/// Hammond gear frequency of wheel `tw_idx` in `f64`: motor Hz x drive/driven x teeth x step-up.
///
/// Same equation as [`OrganLayer::tonewheel_gear_frequency`] without the `f32` rounding, used to
/// derive the `u32` phase steps. ASSUMED: the firmware pitch ladder is 12-TET, not the Hammond
/// gear ratios (`docs/research/organ-tonewheel-bank.md` section 4); the gear ratios are the
/// existing project choice (TD-04).
pub(super) fn gear_frequency_f64(tw_idx: usize) -> f64 {
    let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
    let (drive, driven, teeth) = OrganLayer::tonewheel_gear_spec(clamped);
    let step_up = if clamped >= 84 { 4.0 / 3.0 } else { 1.0 };
    f64::from(HAMMOND_MOTOR_HZ)
        * (f64::from(drive) / f64::from(driven))
        * f64::from(teeth)
        * step_up
}

impl OrganLayer {
    /// Returns `(drive_teeth, driven_teeth, physical_wheel_teeth)` for tonewheel index `0..91`.
    pub fn tonewheel_gear_spec(tw_idx: usize) -> (u32, u32, u32) {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        let (drive, driven) = HAMMOND_GEAR_RATIOS[clamped % 12];
        let octave = clamped / 12;
        let wheel_teeth = if clamped >= 84 { 192 } else { 2u32 << octave };
        (drive, driven, wheel_teeth)
    }

    /// Computes exact Hammond synchronous-shaft frequency for tonewheel index `0..91`.
    pub fn tonewheel_gear_frequency(tw_idx: usize) -> f32 {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        let (drive, driven, wheel_teeth) = Self::tonewheel_gear_spec(clamped);
        let step_up = if clamped >= 84 { 4.0 / 3.0 } else { 1.0 };
        HAMMOND_MOTOR_HZ * (drive as f32 / driven as f32) * (wheel_teeth as f32) * step_up
    }

    /// Returns the active pickup calibration gain for tonewheel `tw_idx` in `B3` vs `B3 Bass` mode.
    pub fn pickup_calibration(&self, tw_idx: usize) -> f32 {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        if self.b3_bass_mode {
            self.pickup_b3_bass[clamped]
        } else {
            self.pickup_b3[clamped]
        }
    }

    /// Fills the per-wheel pickup tables (ASSUMED shapes, not measured).
    pub(super) fn pickup_tables() -> ([f32; NUM_TONEWHEELS], [f32; NUM_TONEWHEELS]) {
        let mut b3 = [1.0f32; NUM_TONEWHEELS];
        let mut bass = [1.0f32; NUM_TONEWHEELS];
        for (i, (b3_i, bass_i)) in b3.iter_mut().zip(bass.iter_mut()).enumerate() {
            *b3_i = if i < 12 {
                0.74 + 0.22 * (i as f32 / 12.0)
            } else if i < 60 {
                1.0 + 0.02 * ((i as f32) * 1.7).sin()
            } else {
                (1.0 - 0.14 * ((i - 60) as f32 / 30.0)).max(0.84)
            };
            *bass_i = if i < 24 {
                1.32 - 0.28 * (i as f32 / 24.0)
            } else if i < 48 {
                1.04 - 0.08 * ((i - 24) as f32 / 24.0)
            } else {
                (0.95 - 0.20 * ((i - 48) as f32 / 42.0)).max(0.75)
            };
        }
        (b3, bass)
    }

    /// Re-derives the `u32` phase steps if the sample rate or the copy switch changed.
    fn sync_bank(&mut self) {
        if self.tone_bank.sample_rate() != self.sample_rate
            || self.tone_bank.copies() != self.detuned_copies
        {
            self.tone_bank.retune(
                &self.tonewheel_freqs_hz,
                self.sample_rate,
                self.detuned_copies,
            );
        }
    }

    /// Advances all 91 spinning tonewheels and returns calibrated + crosstalk-coupled bus waveforms.
    pub(super) fn render_tonewheels(&mut self, inv_fs: f32) -> [f32; NUM_TONEWHEELS] {
        let mut raw = [0.0f32; NUM_TONEWHEELS];
        if self.model == OrganModel::B3 && self.tonewheel_source == ToneWheelSource::Firmware {
            self.sync_bank();
            self.tone_bank.advance(&mut raw);
        } else {
            for (i, sig) in raw.iter_mut().enumerate() {
                let p = self.tonewheel_phases[i];
                *sig = match self.model {
                    OrganModel::B3 => {
                        // Bottom 12 Hammond complex wheels carry 2nd & 3rd harmonic warmth
                        if i < 12 {
                            (p.sin() + 0.18 * (2.0 * p).sin() + 0.09 * (3.0 * p).sin()) * 0.85
                        } else {
                            p.sin()
                        }
                    }
                    OrganModel::Pipe1 => p.sin(),
                    OrganModel::Vox | OrganModel::Farfisa | OrganModel::Pipe2 => {
                        0.7 * p.sin() + 0.3 * (2.0 * p).sin()
                    }
                };
                let next_p = p + TAU * self.tonewheel_freqs[i] * inv_fs;
                self.tonewheel_phases[i] = if next_p >= TAU { next_p - TAU } else { next_p };
            }
        }

        if self.model != OrganModel::B3 {
            return raw;
        }

        // Apply per-tonewheel pickup calibration and -42 dB generator bin crosstalk matrix.
        let mut calibrated = [0.0f32; NUM_TONEWHEELS];
        let bleed = 0.008 * self.crosstalk_level.clamp(0.0, 2.0);
        for (i, dst) in calibrated.iter_mut().enumerate() {
            let pickup = self.pickup_calibration(i);
            let partner_4oct = (i + 48) % NUM_TONEWHEELS;
            let partner_oct = (i + 12) % NUM_TONEWHEELS;
            *dst = raw[i] * pickup + bleed * (0.75 * raw[partner_4oct] + 0.40 * raw[partner_oct]);
        }
        calibrated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_frequency_and_gear_spec() {
        let organ = OrganLayer::new(48_000.0);
        // MIDI 69 (A4) maps to tonewheel index 69 - 24 = 45 (88/64 * 16 * 20 = 440.0 Hz exact)
        assert!((organ.tonewheel_freqs[45] - 440.0).abs() < 1e-3);
        assert!((gear_frequency_f64(45) - 440.0).abs() < 1e-9);
        // C1 (index 0) uses 85/104 gear ratio -> 32.6923 Hz (distinct from ideal ET 32.7032 Hz)
        assert!((organ.tonewheel_freqs[0] - 32.692307).abs() < 1e-3);
        // Top-octave tonewheels >= 84 use physical 192-tooth wheel
        assert_eq!(OrganLayer::tonewheel_gear_spec(84), (85, 104, 192));
    }

    #[test]
    fn b3_vs_b3_bass_pickup_calibration_and_crosstalk() {
        let mut standard = OrganLayer::new(48_000.0);
        let mut bass = OrganLayer::new(48_000.0);
        bass.b3_bass_mode = true;

        // B3 Bass boosts low tonewheel pickup calibration compared to standard B3
        assert!(bass.pickup_calibration(0) > standard.pickup_calibration(0) * 1.5);
        assert!(standard.pickup_calibration(75) > bass.pickup_calibration(75));

        // Verify crosstalk couples energy into partner tonewheels even on a silent drawbar slot
        standard.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
        standard.percussion_enabled = false;
        standard.rotary_enabled = false;
        standard.crosstalk_level = 1.0;
        let mut clean = OrganLayer::new(48_000.0);
        clean.crosstalk_level = 0.0;

        // Advance phase to avoid a degenerate first sample
        for _ in 0..32 {
            standard.render_tonewheels(1.0 / 48_000.0);
            clean.render_tonewheels(1.0 / 48_000.0);
        }
        let a = standard.render_tonewheels(1.0 / 48_000.0);
        let b = clean.render_tonewheels(1.0 / 48_000.0);
        let max_diff = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_diff > 1e-4,
            "crosstalk must couple wheels (diff={max_diff})"
        );
    }

    /// Frequency of the dominant tone of wheel `w`, by zero-crossing count over one second.
    fn measured_hz(organ: &mut OrganLayer, w: usize, fs: f32) -> f32 {
        organ.crosstalk_level = 0.0;
        let mut prev = organ.render_tonewheels(1.0 / fs)[w];
        let mut crossings = 0;
        for _ in 0..fs as usize {
            let cur = organ.render_tonewheels(1.0 / fs)[w];
            if prev < 0.0 && cur >= 0.0 {
                crossings += 1;
            }
            prev = cur;
        }
        crossings as f32
    }

    #[test]
    fn firmware_and_legacy_sources_agree_on_pitch() {
        let fs = 48_000.0;
        let mut fw = OrganLayer::new(fs);
        fw.tonewheel_source = ToneWheelSource::Firmware;
        let mut legacy = OrganLayer::new(fs);
        assert_eq!(legacy.tonewheel_source, ToneWheelSource::Legacy);
        let f_fw = measured_hz(&mut fw, 45, fs);
        let f_lg = measured_hz(&mut legacy, 45, fs);
        assert!(
            (f_fw - 440.0).abs() <= 1.0 && (f_lg - 440.0).abs() <= 1.0,
            "{f_fw} {f_lg}"
        );
    }

    #[test]
    fn firmware_waveform_differs_from_legacy_and_copies_stay_in_pitch() {
        let fs = 48_000.0;
        let mut fw = OrganLayer::new(fs);
        fw.tonewheel_source = ToneWheelSource::Firmware;
        let mut legacy = OrganLayer::new(fs);
        let mut dual = OrganLayer::new(fs);
        dual.tonewheel_source = ToneWheelSource::Firmware;
        dual.detuned_copies = true;
        let (mut d_legacy, mut d_dual) = (0.0f32, 0.0f32);
        for _ in 0..2000 {
            let a = fw.render_tonewheels(1.0 / fs)[45];
            d_legacy += (a - legacy.render_tonewheels(1.0 / fs)[45]).abs();
            d_dual += (a - dual.render_tonewheels(1.0 / fs)[45]).abs();
        }
        assert!(d_legacy > 1.0, "firmware x wave must differ from the sine");
        assert!(d_dual > 0.0, "detuned copies must change the output");
        assert!(dual.tone_bank.copies());
    }

    #[test]
    fn non_b3_models_keep_the_float_path() {
        let mut o = OrganLayer::new(48_000.0);
        o.model = OrganModel::Pipe1;
        let first = o.render_tonewheels(1.0 / 48_000.0);
        assert_eq!(first[10], 0.0); // sin(0): phase is read before it advances
        assert_eq!(o.tone_bank.sample_rate(), 48_000.0);
    }
}
