//! Voice gain de-zipper of the synth DSP (`SY1SY2`): one pole per output frame toward a target
//! that the ARM writes at its 1 kHz control tick.
//!
//! Spec: `docs/specs/synth-engine.md` section 3.2; evidence `docs/research/synth-voice-control.md`
//! section 4 (series `docs/research/data/synth-sy-gain-slew.csv`).
//!
//! Opt-in (`SynthLayer::gain_dezipper`, default off): `v.gain` is the ARM-side target, latched to
//! `gain_target` every control tick (and at note-on); the gain applied to the voice is the state
//! `s' = t + 0.979 (s - t)`, evaluated once per frame.

use super::*;

/// VERIFIED: smoother pole of the SY voice gain, image `b[10] = 0x3F7A9FBE = 0.97899997`; the
/// oracle slew of 80 frames has ratios 0.9789994 .. 0.9790003 (mean 0.97899981), see
/// `docs/research/data/synth-sy-gain-slew.csv`; the engine reproduces the first frames of that
/// series within 2e-6 (`tests::slew_matches_the_oracle_series`).
pub const SY_GAIN_POLE: f32 = 0.979;

/// VERIFIED: audio frame rate of the SY DSP is 43,402.34 Hz, not 48 kHz: the SY saw fade words
/// `B` and `A` are 5 kHz and 15 kHz divided by 43,402.0 to float32 precision
/// (`hw_saw::tests::fade_constants_are_5_and_15_khz_at_the_dsp_frame_rate`), and the ARM holds the
/// sample-rate literal `0x47298A58` (`docs/research/dsp-frame-rate.md`). The pole is per frame, so
/// 47.1 frames are 1.09 ms.
pub const SY_FRAME_RATE_HZ: f32 = crate::organ::FIRMWARE_FRAME_RATE_HZ as f32;

/// ASSUMED: the ARM writes gain targets at a 1 kHz timer tick (timer irq 22, 1 ms,
/// `docs/research/arm-organ-messages.md` section 10.1); the tick phase relative to notes is unknown.
pub const CONTROL_TICK_HZ: f32 = 1_000.0;

/// Per-frame pole at engine rate `fs`: the hardware pole stretched to the same time constant,
/// `0.979^(43402.34 / fs)` (`0.979` at the SY frame rate).
#[inline]
pub fn gain_pole_for(fs: f32) -> f32 {
    SY_GAIN_POLE.powf(SY_FRAME_RATE_HZ / fs)
}

impl SynthVoice {
    /// One smoothing step toward `gain_target`; returns the new applied gain.
    #[inline]
    pub(super) fn smoothed_gain(&mut self, pole: f32) -> f32 {
        self.gain_state = self.gain_target + pole * (self.gain_state - self.gain_target);
        self.gain_state
    }
}

impl SynthLayer {
    /// Advances the control tick; on each tick every voice's target takes its current `gain`.
    #[inline]
    pub(super) fn control_tick(&mut self) {
        self.ctrl_counter += 1;
        let period = (self.sample_rate / CONTROL_TICK_HZ).round().max(1.0) as u32;
        if self.ctrl_counter >= period {
            self.ctrl_counter = 0;
            for v in &mut self.voices {
                v.gain_target = v.gain;
            }
        }
    }

    /// Changes the gain of every sounding voice on `note` (the ARM writing a new target); with the
    /// de-zipper on the applied gain follows at the next control tick through the smoother.
    pub fn set_voice_gain(&mut self, note: u8, gain: f32) {
        let gain = gain.clamp(0.0, 1.0);
        self.held_gains[usize::from(note.min(127))] = gain;
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.active && v.note == note)
        {
            v.gain = gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pole_is_the_hardware_value_at_the_frame_rate_and_keeps_the_time_constant() {
        assert!((gain_pole_for(SY_FRAME_RATE_HZ) - 0.979).abs() < 1e-7);
        // same time constant in seconds: tau = -1 / (fs ln pole)
        let tau = |fs: f32| -1.0 / (fs * gain_pole_for(fs).ln());
        assert!((tau(96_000.0) - tau(48_000.0)).abs() / tau(48_000.0) < 1e-4);
        assert!((tau(44_100.0) - tau(48_000.0)).abs() / tau(48_000.0) < 1e-4);
        assert!((tau(48_000.0) * 1000.0 - 1.083).abs() < 0.01); // ms: 47.1 frames at 43.4 kHz
    }

    #[test]
    fn slew_matches_the_oracle_series() {
        // docs/research/data/synth-sy-gain-slew.csv: state 0.9895, 0.97922045, 0.9691568, ...
        let mut v = SynthVoice {
            gain_state: 1.0,
            gain_target: 0.5,
            ..SynthVoice::default()
        };
        let want = [0.9895, 0.97922045, 0.9691568, 0.95930445];
        for w in want {
            let s = v.smoothed_gain(SY_GAIN_POLE);
            assert!((s - w).abs() < 2e-6, "{s} vs {w}");
        }
    }

    #[test]
    fn step_response_has_the_documented_time_constant_at_48k_and_96k() {
        for fs in [48_000.0f32, 96_000.0] {
            let mut s = SynthLayer::new(fs);
            s.gain_dezipper = true;
            s.note_on(60, 100);
            for _ in 0..(fs as usize / 10) {
                s.render_frame();
            }
            let i = s.voices.iter().position(|v| v.active).unwrap();
            assert!((s.voices[i].gain_state - 1.0).abs() < 1e-3);
            s.set_voice_gain(60, 0.0);
            // the target latches within one control period, then the state falls with tau
            let mut n_e = None;
            for n in 0..(fs as usize / 50) {
                s.render_frame();
                if n_e.is_none() && s.voices[i].gain_state < (-1.0f32).exp() {
                    n_e = Some(n);
                }
            }
            let t_ms = n_e.expect("gain must fall below 1/e") as f32 / fs * 1000.0;
            // 1.08 ms time constant plus up to one 1 ms control tick of latency
            assert!((0.9..2.3).contains(&t_ms), "fs {fs}: 1/e after {t_ms} ms");
        }
    }

    #[test]
    fn target_waits_for_the_control_tick() {
        let mut s = SynthLayer::new(48_000.0);
        s.gain_dezipper = true;
        s.note_on(60, 100);
        s.set_voice_gain(60, 0.25);
        let i = s.voices.iter().position(|v| v.active).unwrap();
        assert_eq!(s.voices[i].gain_target, 1.0);
        for _ in 0..48 {
            s.render_frame();
        }
        assert_eq!(s.voices[i].gain_target, 0.25);
    }

    #[test]
    fn dezipper_off_is_the_legacy_path() {
        let mut a = SynthLayer::new(48_000.0);
        let mut b = SynthLayer::new(48_000.0);
        b.gain_dezipper = false;
        a.note_on(60, 100);
        b.note_on(60, 100);
        for _ in 0..256 {
            assert_eq!(a.render_frame(), b.render_frame());
        }
    }
}
