//! Tonewheel frequencies, pickup calibration and the 91-wheel generator.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1. The Pipe models (Pipe1 / Pipe2) read the PNO
//! Core1 bank arithmetic (`firmware_bank.rs`, algorithm VERIFIED, role = Pipe organ bank per
//! `docs/research/organ-model-paths.md`); `B3` can opt in with [`ToneWheelSource::Firmware`] as an
//! experiment (attribution disproved). Every other case keeps the float-phase sine path.

use super::types::{HAMMOND_GEAR_RATIOS, HAMMOND_MOTOR_HZ, NUM_TONEWHEELS};
use super::vibrato_bank::{clamp_fine_tune, wheel_hz};
use super::{
    level_for_scanner, OrganLayer, OrganModel, ToneWheelSource, VibratoLevel, VibratoLink,
    WeightMode,
};
use std::f32::consts::TAU;

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

    /// Re-derives the `u32` phase steps if the sample rate, the vibrato table or Fine Tune changed.
    fn sync_bank(&mut self, copies: bool) {
        let fine = clamp_fine_tune(self.fine_tune_cents);
        if self.tone_bank.sample_rate() != self.sample_rate
            || self.tone_bank.copies() != copies
            || self.synced_fine_cents != fine
        {
            self.lane_mixer.set_rate(self.sample_rate);
            for (w, f) in self.tonewheel_freqs_hz.iter_mut().enumerate() {
                *f = wheel_hz(w, fine);
            }
            self.synced_fine_cents = fine;
            self.tone_bank
                .retune(&self.tonewheel_freqs_hz, self.sample_rate, copies);
        }
    }

    /// Vibrato-enable level in force: `detuned_copies` forces level 1, otherwise (Firmware link)
    /// the level follows `scanner_mode`; the Manual link never leaves level 0 by itself.
    fn vibrato_level(&self) -> VibratoLevel {
        if self.detuned_copies {
            VibratoLevel::DETUNED
        } else if self.vibrato_link == VibratoLink::Firmware {
            level_for_scanner(self.scanner_mode)
        } else {
            VibratoLevel(0)
        }
    }

    /// Tone source in force: [`ToneWheelSource::resolve`] of the layer's source and model.
    pub fn effective_tone_source(&self) -> ToneWheelSource {
        self.tonewheel_source.resolve(self.model)
    }

    /// Single-lane view of [`Self::render_tonewheel_lanes`] (lane x).
    #[cfg(test)]
    pub(super) fn render_tonewheels(&mut self, inv_fs: f32) -> [f32; NUM_TONEWHEELS] {
        self.render_tonewheel_lanes(inv_fs).0[0]
    }

    /// Advances all 91 spinning tonewheels and returns calibrated + crosstalk-coupled bus
    /// waveforms of `lanes` lanes (1, or 2 while the detuned ensemble's stereo blend is active:
    /// `Firmware` source, `Firmware` link, level 1 or its gain ramp still running).
    ///
    /// Lane `x = m00 a + m01 b`, `y = m10 a + m11 b` with the smoothed matrix of
    /// [`LaneMixer`]; in steady state this is the oracle's mix stage (VERIFIED), the lane split
    /// before the linear pickup and crosstalk stages is equivalent to splitting after them.
    pub(super) fn render_tonewheel_lanes(
        &mut self,
        inv_fs: f32,
    ) -> ([[f32; NUM_TONEWHEELS]; 2], usize) {
        let mut raw = [[0.0f32; NUM_TONEWHEELS]; 2];
        let mut lanes = 1;
        let source = self.effective_tone_source();
        if source == ToneWheelSource::B3Wheels {
            let fine = clamp_fine_tune(self.fine_tune_cents);
            if self.b3_synced != (self.sample_rate, fine) {
                self.b3_wheels.retune(self.sample_rate, fine);
                self.b3_synced = (self.sample_rate, fine);
            }
            self.b3_wheels.advance(&mut raw[0]);
        } else if source == ToneWheelSource::FarfisaWheels {
            let fine = clamp_fine_tune(self.fine_tune_cents);
            if self.farfisa_synced != (self.sample_rate, fine) {
                self.farfisa_wheels.retune(self.sample_rate, fine);
                self.farfisa_synced = (self.sample_rate, fine);
            }
            self.farfisa_wheels.advance(&mut raw[0]);
        } else if source == ToneWheelSource::Firmware {
            if self.vibrato_link == VibratoLink::Firmware {
                self.lane_mixer.set_level(self.vibrato_level());
                let copies = self.lane_mixer.active();
                self.sync_bank(copies);
                if copies {
                    let (mut a, mut b) = ([0.0; NUM_TONEWHEELS], [0.0; NUM_TONEWHEELS]);
                    self.tone_bank.advance_copies(&mut a, &mut b);
                    let m = self.lane_mixer.step();
                    let [x, y] = &mut raw;
                    for (((xo, yo), &p), &q) in x.iter_mut().zip(y.iter_mut()).zip(&a).zip(&b) {
                        *xo = m[0][0] * p + m[0][1] * q;
                        *yo = m[1][0] * p + m[1][1] * q;
                    }
                    lanes = 2;
                } else {
                    self.tone_bank.advance(&mut raw[0]);
                }
            } else {
                self.sync_bank(self.detuned_copies);
                self.tone_bank.advance(&mut raw[0]);
            }
        } else {
            for (i, sig) in raw[0].iter_mut().enumerate() {
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
            return (raw, lanes);
        }

        // Apply per-tonewheel pickup calibration and -42 dB generator bin crosstalk matrix.
        let bleed = 0.008 * self.crosstalk_level.clamp(0.0, 2.0);
        let mut calibrated = [[0.0f32; NUM_TONEWHEELS]; 2];
        for (dst_lane, src_lane) in calibrated.iter_mut().zip(raw.iter()).take(lanes) {
            for (i, dst) in dst_lane.iter_mut().enumerate() {
                let pickup = if source == ToneWheelSource::B3Wheels
                    || (self.weight_mode == WeightMode::Firmware && !self.b3_bass_mode)
                {
                    1.0 // the firmware path has no per-wheel calibration (ASSUMED tables bypassed)
                } else {
                    self.pickup_calibration(i)
                };
                let partner_4oct = (i + 48) % NUM_TONEWHEELS;
                let partner_oct = (i + 12) % NUM_TONEWHEELS;
                *dst = src_lane[i] * pickup
                    + bleed * (0.75 * src_lane[partner_4oct] + 0.40 * src_lane[partner_oct]);
            }
        }
        (calibrated, lanes)
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
        legacy.tonewheel_source = ToneWheelSource::Legacy;
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
        legacy.tonewheel_source = ToneWheelSource::Legacy;
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
    fn fine_tune_retunes_the_firmware_bank() {
        let fs = 48_000.0;
        let mut base = OrganLayer::new(fs);
        base.model = OrganModel::Pipe1;
        let mut up = OrganLayer::new(fs);
        up.model = OrganModel::Pipe1;
        up.fine_tune_cents = 100.0; // one semitone: wheel 45 sounds like wheel 46
        let f_base = measured_hz(&mut base, 45, fs);
        let f_up = measured_hz(&mut up, 45, fs);
        assert!((f_base - 440.0).abs() <= 1.0, "{f_base}");
        assert!((f_up - 466.16).abs() <= 1.0, "{f_up}");
    }

    #[test]
    fn firmware_pitch_is_12_tet_not_gear_ratio() {
        let mut o = OrganLayer::new(48_000.0);
        o.model = OrganModel::Pipe1;
        o.render_tonewheels(1.0 / 48_000.0);
        // wheel 0: ET C1 32.7032 Hz (the gear ratio gives 32.6923 Hz, kept for Legacy only)
        assert!((o.tonewheel_freqs_hz[0] - 32.703_195_66).abs() < 1e-6);
        assert!((f64::from(o.tonewheel_freqs[0]) - o.tonewheel_freqs_hz[0]).abs() > 0.01);
    }

    #[test]
    fn legacy_models_keep_the_float_path() {
        // B3 (default), Vox, Farfisa (even when Firmware is forced): the float-phase sine path
        for (model, source) in [
            (OrganModel::B3, ToneWheelSource::ModelDefault),
            (OrganModel::Vox, ToneWheelSource::ModelDefault),
            (OrganModel::Farfisa, ToneWheelSource::Firmware),
            (OrganModel::Pipe1, ToneWheelSource::Legacy),
        ] {
            let mut o = OrganLayer::new(48_000.0);
            o.model = model;
            o.tonewheel_source = source;
            assert_eq!(o.effective_tone_source(), ToneWheelSource::Legacy);
            let first = o.render_tonewheels(1.0 / 48_000.0);
            assert_eq!(first[10], 0.0); // sin(0): phase is read before it advances
            assert_eq!(o.tone_bank.sample_rate(), 48_000.0, "{model:?}");
        }
    }

    #[test]
    fn pipe_models_use_the_bank_and_b3_does_not_by_default() {
        let fs = 48_000.0;
        let mk = |m: OrganModel, src: ToneWheelSource| {
            let mut o = OrganLayer::new(fs);
            o.model = m;
            o.tonewheel_source = src;
            o
        };
        let auto = ToneWheelSource::ModelDefault;
        let mut b3 = mk(OrganModel::B3, auto);
        // experiment: forcing the bank on B3 (attribution disproved) is still selectable
        let mut b3_exp = mk(OrganModel::B3, ToneWheelSource::Firmware);
        let mut p1 = mk(OrganModel::Pipe1, auto);
        let mut p2 = mk(OrganModel::Pipe2, auto);
        assert_eq!(b3.effective_tone_source(), ToneWheelSource::Legacy);
        assert_eq!(b3_exp.effective_tone_source(), ToneWheelSource::Firmware);
        assert_eq!(p1.effective_tone_source(), ToneWheelSource::Firmware);
        assert_eq!(p2.effective_tone_source(), ToneWheelSource::Firmware);
        let (mut d_b3, mut d_p1, mut d_p2) = (0.0f32, 0.0f32, 0.0f32);
        for _ in 0..2000 {
            let bank = b3_exp.render_tonewheels(1.0 / fs)[45];
            d_b3 += (bank - b3.render_tonewheels(1.0 / fs)[45]).abs();
            // Pipe layers carry the same bank waveform (no B3 pickup calibration on top)
            d_p1 += (bank - p1.render_tonewheels(1.0 / fs)[45]).abs();
            d_p2 += (bank - p2.render_tonewheels(1.0 / fs)[45]).abs();
        }
        assert!(d_b3 > 1.0, "B3 default keeps the legacy sine, not the bank");
        // B3 experiment adds pickup calibration and crosstalk, Pipe does not: close, not equal
        assert!(d_p1 < d_b3 && d_p2 == d_p1, "{d_p1} {d_p2} {d_b3}");
    }
}
