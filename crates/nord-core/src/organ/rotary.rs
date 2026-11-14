//! Model 122 rotary speaker: tube drive, 800 Hz crossover, Doppler delays and cabinet reflections.
//!
//! Spec: `docs/specs/organ-engine.md` section 4. Crossover and balance steps are VERIFIED
//! (`types.rs`); rotor speeds, excursions and reflections are ASSUMED.

use super::rotor_stop::{clamp_stop_angle, ease_phase, STOP_ANGLE_RAD};
use super::types::{ROTARY_BUFFER_SIZE, ROTARY_CROSSOVER_HZ};
use super::{OrganLayer, RotarySpeed};
use std::f32::consts::TAU;

impl OrganLayer {
    /// Copies the shared Rotary Speaker controls of `src` (speed, drive, mic, angle, stop mode).
    ///
    /// Manual p.48: the Rotary is one instance per Program; its speed and drive are common to every
    /// sound routed through it. Used by the engine's `TO ROTARY` bus.
    pub fn mirror_rotary_settings(&mut self, src: &OrganLayer) {
        self.rotary_speed = src.rotary_speed;
        self.rotary_drive = src.rotary_drive;
        self.rotary_drive_unity = src.rotary_drive_unity;
        self.rotary_drive_scaled = src.rotary_drive_scaled;
        self.rotary_close_mic = src.rotary_close_mic;
        self.rotary_mic_angle_deg = src.rotary_mic_angle_deg;
        self.rotary_stop_mode = src.rotary_stop_mode;
        self.rotary_stop_angle = src.rotary_stop_angle;
        self.globals = src.globals;
    }

    /// Runs a mono external signal (a Piano / Synth layer routed `TO ROTARY`) through the rotary.
    ///
    /// Allocation free; uses the layer's own rotor state, so call it on a dedicated bus instance.
    #[inline]
    pub fn process_external_rotary(&mut self, x: f32) -> [f32; 2] {
        self.process_rotary(x, 1.0 / self.sample_rate)
    }

    /// The firmware Leslie processor on one organ sample: the engine's ramped rotor rates become
    /// the three speed words by the ARM's own law (`tools/arm_leslie_speeds.py`: `0xB2F25` rotor,
    /// `0xB2F26` horn, `0xB2FA1` follows the horn), the organ signal goes in the left lane (the
    /// right lane of the organ bus is silent, `docs/research/fx-leslie.md`).
    ///
    /// VERIFIED (ARM state block, `docs/research/arm-part-activation.md`): the firmware speeds are
    /// horn 0.80 Hz slow / 7.0 Hz fast and rotor 0.73 / 6.4 Hz, `word = int(hz * 8388607 /
    /// 11025) << 8`. The engine's ramp (its own time constants) supplies the speed in between:
    /// the fraction of the engine's slow to fast range is mapped onto the firmware range.
    fn process_leslie_firmware(&mut self, x: f32) -> [f32; 2] {
        const FS_DSP: f64 = 44_444_000.0 / 1024.0;
        const SCALE: f64 = 8_388_607.0 / 11_025.0;
        const HORN_FOLLOW: f64 = 0.5748; // EMPIRICAL, fits 0xB2FA1 at 0.80 and 0.86 Hz
                                         // (engine slow, engine fast, firmware slow, firmware fast) in Hz
        const HORN: (f64, f64, f64, f64) = (0.80, 6.85, 0.80, 7.0);
        const ROTOR: (f64, f64, f64, f64) = (0.67, 5.70, 0.73, 6.4);
        let arm_hz = |r: f64, (es, ef, fs, ff): (f64, f64, f64, f64)| {
            if r <= es {
                r * fs / es
            } else {
                fs + (r - es) / (ef - es) * (ff - fs)
            }
        };
        // the firmware words mean Hz at its own frame rate; at the engine rate they scale
        let k = FS_DSP / f64::from(self.sample_rate);
        let word = |hz: f64| (((hz * SCALE).trunc().max(0.0) as u32) << 8) as f64 * k;
        let horn = arm_hz(f64::from(self.horn_rate_hz), HORN);
        let rotor = arm_hz(f64::from(self.drum_rate_hz), ROTOR);
        let gain = self.leslie_gain;
        if let Some(l) = self.leslie_fw.as_mut() {
            l.set_speed_words([
                word(rotor).round() as u32,
                word(horn).round() as u32,
                word(horn * HORN_FOLLOW).round() as u32,
            ]);
            let y = l.process(x, 0.0);
            return [y[0] * gain, y[1] * gain];
        }
        [0.0; 2]
    }

    /// Processes Model 122 Rotary Speaker with Doppler delay, Close Mic, Angle, Stop Mode, and cabinet reflections.
    pub(super) fn process_rotary(&mut self, scanner_out: f32, inv_fs: f32) -> [f32; 2] {
        let drive = if self.rotary_drive_scaled {
            (self.rotary_drive / 10.0).clamp(0.0, 1.0)
        } else {
            self.rotary_drive
        };
        let gain = 1.0 + 3.0 * drive;
        let driven = (gain * scanner_out).tanh();
        let driven = if self.rotary_drive_unity {
            driven / gain
        } else {
            driven
        };
        let alpha = (TAU * ROTARY_CROSSOVER_HZ * inv_fs).clamp(0.01, 0.99);
        self.lp_state += alpha * (driven - self.lp_state);
        let bass = self.lp_state * self.globals.bass_gain;
        let treble = (driven - self.lp_state) * self.globals.horn_gain;

        let effective_speed = if self.rotary_stop_mode {
            RotarySpeed::Stop
        } else {
            self.rotary_speed
        };
        let (target_horn, target_drum) = match effective_speed {
            RotarySpeed::Stop => (0.0, 0.0),
            RotarySpeed::Slow => (0.80, 0.67),
            RotarySpeed::Fast => (6.85, 5.70),
        };
        // Rotary Horn/Rotor Speed and Acc menu entries (factors are exactly 1.0 by default).
        let target_horn = target_horn * self.globals.horn_speed_scale;
        let target_drum = target_drum * self.globals.rotor_speed_scale;
        self.horn_rate_hz += (target_horn - self.horn_rate_hz)
            * (inv_fs / (0.7 * self.globals.horn_tau_scale)).min(1.0);
        self.drum_rate_hz += (target_drum - self.drum_rate_hz)
            * (inv_fs / (4.2 * self.globals.rotor_tau_scale)).min(1.0);

        if self.leslie_fw.is_some() {
            return self.process_leslie_firmware(scanner_out);
        }
        // Stopped: with an ANGLE picked the horn and rotor settle at that rest phase (manual p.53);
        // with Free they keep the tiny residual rate and stay wherever they happen to stop.
        let rest = (effective_speed == RotarySpeed::Stop && self.rotary_stop_angle > 0)
            .then(|| STOP_ANGLE_RAD[usize::from(clamp_stop_angle(self.rotary_stop_angle))]);
        match rest {
            Some(t) if self.horn_rate_hz < 0.05 => {
                self.horn_phase = ease_phase(self.horn_phase, t, inv_fs * 6.0);
            }
            _ => self.horn_phase = (self.horn_phase + TAU * self.horn_rate_hz * inv_fs) % TAU,
        }
        match rest {
            Some(t) if self.drum_rate_hz < 0.05 => {
                self.drum_phase = ease_phase(self.drum_phase, t, inv_fs * 3.0);
            }
            _ => self.drum_phase = (self.drum_phase + TAU * self.drum_rate_hz * inv_fs) % TAU,
        }

        // Write crossover bands into pre-allocated Doppler / reflection buffers
        self.rotary_horn_buf[self.rotary_write_idx] = treble;
        self.rotary_drum_buf[self.rotary_write_idx] = bass;

        let angle_rad = self.rotary_mic_angle_deg.clamp(0.0, 180.0).to_radians();
        let (doppler_ms, beam_depth, refl_gain) = if self.rotary_close_mic {
            (0.55, 0.44, 0.05)
        } else {
            (0.35, 0.26, 0.18)
        };
        let base_delay = 0.002 * self.sample_rate;
        let exc_horn = doppler_ms * 0.001 * self.sample_rate;
        let exc_drum = (doppler_ms * 0.35) * 0.001 * self.sample_rate;

        let delay_hl = base_delay + exc_horn * (1.0 - self.horn_phase.cos());
        let delay_hr = base_delay + exc_horn * (1.0 - (self.horn_phase + angle_rad).cos());
        let delay_dl = base_delay + exc_drum * (1.0 - self.drum_phase.cos());
        let delay_dr = base_delay + exc_drum * (1.0 - (self.drum_phase + angle_rad).cos());

        let horn_l_raw = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            delay_hl,
        );
        let horn_r_raw = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            delay_hr,
        );
        let drum_l_raw = Self::read_ring_interpolated(
            self.rotary_drum_buf.as_ref(),
            self.rotary_write_idx,
            delay_dl,
        );
        let drum_r_raw = Self::read_ring_interpolated(
            self.rotary_drum_buf.as_ref(),
            self.rotary_write_idx,
            delay_dr,
        );

        // Cabinet early reflections (louder Horn cross-reflections in Normal mic distance)
        let refl_l = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            0.0034 * self.sample_rate,
        ) * refl_gain;
        let refl_r = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            0.0046 * self.sample_rate,
        ) * refl_gain;

        self.rotary_write_idx = (self.rotary_write_idx + 1) % ROTARY_BUFFER_SIZE;

        let beam_l = (1.0 - 0.5 * beam_depth) + 0.5 * beam_depth * self.horn_phase.cos();
        let beam_r =
            (1.0 - 0.5 * beam_depth) + 0.5 * beam_depth * (self.horn_phase + angle_rad).cos();
        let drum_beam_l = 0.85 + 0.15 * self.drum_phase.cos();
        let drum_beam_r = 0.85 + 0.15 * (self.drum_phase + angle_rad).cos();

        [
            horn_l_raw * beam_l + drum_l_raw * drum_beam_l + refl_r,
            horn_r_raw * beam_r + drum_r_raw * drum_beam_r + refl_l,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_angle_close_mic_gives_stereo_and_stop_mode_brakes() {
        let mut close_mic = OrganLayer::new(48_000.0);
        close_mic.rotary_speed = RotarySpeed::Fast;
        close_mic.rotary_close_mic = true;
        close_mic.rotary_mic_angle_deg = 180.0;
        close_mic.note_on(60);

        let mut stopped = OrganLayer::new(48_000.0);
        stopped.rotary_speed = RotarySpeed::Fast;
        stopped.rotary_stop_mode = true;
        stopped.note_on(60);

        let mut diff_lr_close = 0.0f32;
        for i in 0..96_000 {
            let [l, r] = close_mic.render_frame();
            if i < 4096 {
                diff_lr_close += (l - r).abs();
            }
            stopped.render_frame();
        }
        assert!(diff_lr_close > 0.5, "stereo separation too low");
        assert!(
            stopped.horn_rate_hz < 0.06 && stopped.drum_rate_hz < 0.45,
            "horn={} drum={}",
            stopped.horn_rate_hz,
            stopped.drum_rate_hz
        );
    }

    #[test]
    fn unity_drive_law_keeps_the_small_signal_gain_and_the_clip_level() {
        let level = |unity: bool, amp: f32| {
            let mut o = OrganLayer::new(48_000.0);
            o.rotary_drive = 2.44;
            o.rotary_drive_unity = unity;
            let mut peak = 0.0f32;
            for i in 0..4096 {
                let x = amp * (i as f32 * 0.05).sin();
                let [l, _] = o.process_rotary(x, 1.0 / 48_000.0);
                if i > 2048 {
                    peak = peak.max(l.abs());
                }
            }
            peak
        };
        // engine law: +18 dB on a quiet input; unity law: about the input level (rotary losses aside)
        assert!(level(false, 0.001) > 5.0 * level(true, 0.001));
        assert!(level(true, 0.001) < 0.002);
        // a loud input clips below 1/g under the unity law
        assert!(level(true, 4.0) < 0.2);
    }

    #[test]
    fn external_rotary_is_stereo_and_mirrors_settings() {
        let mut src = OrganLayer::new(48_000.0);
        src.rotary_speed = RotarySpeed::Fast;
        src.rotary_drive = 0.9;
        src.rotary_close_mic = true;
        let mut bus = OrganLayer::new(48_000.0);
        bus.mirror_rotary_settings(&src);
        assert_eq!(bus.rotary_speed, RotarySpeed::Fast);
        assert_eq!(bus.rotary_drive, 0.9);
        assert!(bus.rotary_close_mic);
        let mut diff = 0.0f32;
        for i in 0..8192 {
            let [l, r] = bus.process_external_rotary(0.5 * (i as f32 * 0.07).sin());
            assert!(l.is_finite() && r.is_finite());
            diff += (l - r).abs();
        }
        assert!(diff > 1.0, "rotary output is not stereo");
    }

    #[test]
    fn rotary_output_is_finite_for_loud_input() {
        let mut o = OrganLayer::new(44_100.0);
        o.rotary_drive = 1.0;
        for i in 0..2048 {
            let [l, r] = o.process_rotary(4.0 * (i as f32 * 0.05).sin(), 1.0 / 44_100.0);
            assert!(l.is_finite() && r.is_finite());
        }
        assert_eq!(ROTARY_BUFFER_SIZE, 1024);
        assert_eq!(ROTARY_CROSSOVER_HZ, 800.0);
    }

    #[test]
    fn scaled_drive_flag_reads_the_preset_unit_without_rewriting_it() {
        let run = |drive: f32, scaled: bool| {
            let mut o = OrganLayer::new(48_000.0);
            o.rotary_drive = drive;
            o.rotary_drive_scaled = scaled;
            (0..512)
                .map(|i| o.process_rotary(0.3 * (i as f32 * 0.05).sin(), 1.0 / 48_000.0)[0])
                .collect::<Vec<f32>>()
        };
        assert_eq!(run(2.44, true), run(0.244, false));
        assert_ne!(run(2.44, true), run(2.44, false));
        let mut dst = OrganLayer::new(48_000.0);
        let mut src = OrganLayer::new(48_000.0);
        src.rotary_drive_scaled = true;
        dst.mirror_rotary_settings(&src);
        assert!(dst.rotary_drive_scaled);
    }

    /// Runs a stopped rotary with the given angle and returns the settled (horn, drum) phases.
    fn settled_phases(angle: u8, start: f32) -> (f32, f32) {
        let mut o = OrganLayer::new(48_000.0);
        o.rotary_stop_mode = true;
        o.rotary_stop_angle = angle;
        o.horn_phase = start;
        o.drum_phase = start;
        // already braked: the rate ramp (4.2 s time constant) is tested elsewhere
        o.horn_rate_hz = 0.0;
        o.drum_rate_hz = 0.0;
        for i in 0..(48_000 * 3) {
            o.process_rotary(0.1 * (i as f32 * 0.05).sin(), 1.0 / 48_000.0);
        }
        (o.horn_phase, o.drum_phase)
    }

    #[test]
    fn stop_angle_parks_horn_and_rotor_at_the_picked_position_from_any_start() {
        for angle in 1..=6u8 {
            let want = STOP_ANGLE_RAD[usize::from(angle)];
            for start in [0.5_f32, 2.0, 4.0, 6.0] {
                let (h, d) = settled_phases(angle, start);
                let err = |p: f32| {
                    let e = (p - want).rem_euclid(TAU);
                    e.min(TAU - e)
                };
                assert!(
                    err(h) < 1e-2 && err(d) < 1e-2,
                    "angle {angle} start {start}: h={h} d={d}"
                );
            }
        }
    }

    #[test]
    fn free_stop_keeps_the_phase_it_stopped_at_and_is_deterministic() {
        let a = settled_phases(0, 2.0);
        assert_eq!(a, settled_phases(0, 2.0));
        let b = settled_phases(0, 4.0);
        assert_ne!(a.0, b.0, "Free must not pull both starts to one angle");
    }

    #[test]
    fn mirror_copies_the_stop_angle() {
        let mut src = OrganLayer::new(48_000.0);
        src.rotary_stop_angle = 4;
        let mut dst = OrganLayer::new(48_000.0);
        dst.mirror_rotary_settings(&src);
        assert_eq!(dst.rotary_stop_angle, 4);
    }
}
