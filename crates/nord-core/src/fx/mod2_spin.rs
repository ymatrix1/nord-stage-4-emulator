//! `Mod 2` types `Spin` (neutral rotating-speaker style effect) and `Ensemble` (three cross-fed modulated lines).
//!
//! Manual (OS v1.6x p.50-51): SPIN emulates a rotating speaker with a more neutral character and without separate
//! rotor and horn; Rate changes ramp (the speed changes gradually); VARIATION shifts the movement from mostly the
//! treble range to lower frequencies. ENS is modelled after a vintage Eminent organ effect (three modulated delay
//! lines, cross-connected), VARIATION = mono. No firmware DSP is decoded: every number is `ASSUMED`/`EMPIRICAL`.

use super::*;
use crate::phase_wrap::wrap_pos;

/// ASSUMED: speed ramp time constant 0.8 s (exponential approach to the Rate knob). Rotary speaker cabinets take
/// roughly 1-5 s between slow and fast (https://en.wikipedia.org/wiki/Leslie_speaker); confirm by recording a
/// Spin rate jump on a Stage 4 and fitting the ramp.
const SPIN_RAMP_TAU_S: f32 = 0.8;
/// EMPIRICAL: crossover 800 Hz between the modulated "rotor" low band and "horn" high band, the classic rotary
/// split (https://en.wikipedia.org/wiki/Leslie_speaker). Not compared to the hardware.
const SPIN_XOVER_HZ: f32 = 800.0;
/// ASSUMED: amplitude modulation depth at AMOUNT = 1 for the emphasised band and the other band
/// (normal: treble emphasised; VARIATION: low band emphasised).
const SPIN_AM_MAIN: f32 = 0.6;
const SPIN_AM_OTHER: f32 = 0.15;
/// ASSUMED: Doppler delay base and depth in ms at AMOUNT = 1 (scaled by AMOUNT), L/R phase offset 90 degrees
/// (two virtual microphones).
const SPIN_DELAY_BASE_MS: f32 = 0.8;
const SPIN_DELAY_DEPTH_MS: f32 = 0.25;

/// ASSUMED: Ensemble modulated-line centre delay and depth in ms; mix at AMOUNT = 1. Eminent-style three-phase
/// chorus (https://en.wikipedia.org/wiki/String_ensemble), lines 120 degrees apart.
const ENS_BASE_MS: f32 = 9.0;
const ENS_DEPTH_MS: f32 = 3.0;
const ENS_WET: f32 = 0.6;

/// DSP state of `Spin` (preallocated, `Copy`).
#[derive(Debug, Clone, Copy)]
pub(super) struct SpinState {
    /// Current (ramped) rotation speed in Hz.
    rate_hz: f32,
    lp: [f32; 2],
}

impl SpinState {
    pub(super) const fn new() -> Self {
        Self {
            rate_hz: 0.8,
            lp: [0.0; 2],
        }
    }
}

impl LayerFxChain {
    /// Current ramped Spin rotation speed in Hz (diagnostic / test hook).
    pub fn spin_rate_hz(&self) -> f32 {
        self.spin.rate_hz
    }

    pub(super) fn process_spin(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        let amt = self.mod2_amount.clamp(0.0, 1.0);
        let w = self.mod2_write_idx;
        self.mod2_buf_l[w] = l;
        self.mod2_buf_r[w] = r;
        self.mod2_write_idx = (w + 1) % MOD2_LINE_LEN;
        // ramped rate change
        let s = &mut self.spin;
        s.rate_hz += (self.mod2_rate_hz - s.rate_hz) * (inv_fs / SPIN_RAMP_TAU_S).min(1.0);
        self.mod2_phase = wrap_pos(self.mod2_phase + TAU * s.rate_hz * inv_fs, TAU);
        if amt < 1e-4 {
            return [l, r];
        }
        let (hi_depth, lo_depth) = if self.mod2_variation {
            (SPIN_AM_OTHER, SPIN_AM_MAIN)
        } else {
            (SPIN_AM_MAIN, SPIN_AM_OTHER)
        };
        let a_lp = (TAU * SPIN_XOVER_HZ * inv_fs).min(1.0);
        let mut out = [0.0f32; 2];
        for (ch, o) in out.iter_mut().enumerate() {
            let m = (self.mod2_phase + ch as f32 * std::f32::consts::FRAC_PI_2).sin();
            let d = (SPIN_DELAY_BASE_MS + SPIN_DELAY_DEPTH_MS * m) * amt * 0.001 * self.sample_rate;
            let buf = if ch == 0 {
                &self.mod2_buf_l
            } else {
                &self.mod2_buf_r
            };
            let x = read_ring_interpolated(buf, self.mod2_write_idx, d + 1.0);
            let lp = &mut self.spin.lp[ch];
            *lp += a_lp * (x - *lp);
            let hi = x - *lp;
            *o = *lp * (1.0 + lo_depth * amt * m) + hi * (1.0 + hi_depth * amt * m);
        }
        out
    }

    pub(super) fn process_ensemble(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        let amt = self.mod2_amount.clamp(0.0, 1.0);
        let w = self.mod2_write_idx;
        let mono = 0.5 * (l + r);
        self.mod2_buf_l[w] = mono;
        self.mod2_write_idx = (w + 1) % MOD2_LINE_LEN;
        self.mod2_phase = wrap_pos(self.mod2_phase + TAU * self.mod2_rate_hz * inv_fs, TAU);
        let fs = self.sample_rate;
        let ph = self.mod2_phase;
        let wr = self.mod2_write_idx;
        let tap = |buf: &[f32], k: f32| {
            let d = (ENS_BASE_MS + ENS_DEPTH_MS * (ph + k * TAU / 3.0).sin()) * 0.001 * fs;
            read_ring_interpolated(buf, wr, d)
        };
        let t = [
            tap(&self.mod2_buf_l, 0.0),
            tap(&self.mod2_buf_l, 1.0),
            tap(&self.mod2_buf_l, 2.0),
        ];
        // VARIATION: mono; normal: lines spread over L/R (centre line shared).
        let (wl, wr_) = if self.mod2_variation {
            let v = (t[0] + t[1] + t[2]) / 3.0;
            (v, v)
        } else {
            (0.6 * t[0] + 0.4 * t[1], 0.6 * t[2] + 0.4 * t[1])
        };
        let wet = ENS_WET * amt;
        [l * (1.0 - wet) + wl * wet, r * (1.0 - wet) + wr_ * wet]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn chain(t: Mod2Type) -> LayerFxChain {
        let mut fx = LayerFxChain::new(FS);
        fx.reverb_model = ReverbModel::Off;
        fx.mod2_type = t;
        fx.mod2_amount = 1.0;
        fx
    }

    /// Envelope of the left channel in 2 ms peak windows for a sine of `hz`.
    fn envelope(fx: &mut LayerFxChain, hz: f32, seconds: f32) -> Vec<f32> {
        let n = (seconds * FS) as usize;
        let win = 96;
        let mut env = Vec::new();
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = 0.5 * (TAU * hz * i as f32 / FS).sin();
            peak = peak.max(fx.process_frame([x, x])[0].abs());
            if (i + 1) % win == 0 {
                env.push(peak);
                peak = 0.0;
            }
        }
        env
    }

    /// Magnitude of the envelope's component at `f` Hz (window = 2 ms).
    fn bin(env: &[f32], f: f32) -> f32 {
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, v) in env.iter().enumerate() {
            let t = i as f32 * 96.0 / FS;
            re += (v - mean) * (TAU * f * t).cos();
            im += (v - mean) * (TAU * f * t).sin();
        }
        (re * re + im * im).sqrt() / env.len() as f32
    }

    fn depth(env: &[f32]) -> f32 {
        let max = env.iter().cloned().fold(0.0, f32::max);
        let min = env.iter().cloned().fold(f32::MAX, f32::min);
        (max - min) / (max + min)
    }

    #[test]
    fn spin_modulates_amplitude_at_the_set_rate() {
        let mut fx = chain(Mod2Type::Spin);
        fx.mod2_rate_hz = 4.0;
        fx.spin.rate_hz = 4.0; // settled
        let env = envelope(&mut fx, 3000.0, 3.0);
        let env = &env[env.len() / 4..];
        let at_rate = bin(env, 4.0);
        assert!(
            at_rate > 4.0 * bin(env, 2.7),
            "{at_rate} vs {}",
            bin(env, 2.7)
        );
        assert!(at_rate > 4.0 * bin(env, 6.0));
        assert!(depth(env) > 0.2);
    }

    #[test]
    fn spin_rate_changes_ramp_gradually_to_the_target() {
        let mut fx = chain(Mod2Type::Spin);
        fx.mod2_rate_hz = 1.0;
        fx.spin.rate_hz = 1.0;
        fx.mod2_rate_hz = 6.0;
        let mut prev = fx.spin_rate_hz();
        let mut max_step = 0.0f32;
        let mut at_1s = 0.0;
        for n in 0..(6.0 * FS) as usize {
            fx.process_frame([0.1, 0.1]);
            let cur = fx.spin_rate_hz();
            assert!(cur >= prev);
            max_step = max_step.max(cur - prev);
            prev = cur;
            if n == FS as usize {
                at_1s = cur;
            }
        }
        assert!(at_1s > 2.0 && at_1s < 5.0, "after 1 s: {at_1s}");
        assert!(max_step < 2e-4, "jump {max_step}");
        assert!(prev > 5.9, "final {prev}");
    }

    #[test]
    fn spin_variation_moves_the_modulation_to_lower_frequencies() {
        let measure = |var: bool, hz: f32| {
            let mut fx = chain(Mod2Type::Spin);
            fx.mod2_variation = var;
            fx.mod2_rate_hz = 4.0;
            fx.spin.rate_hz = 4.0;
            let env = envelope(&mut fx, hz, 2.0);
            depth(&env[env.len() / 4..])
        };
        assert!(measure(false, 3000.0) > 2.0 * measure(true, 3000.0));
        assert!(measure(true, 150.0) > 2.0 * measure(false, 150.0));
    }

    #[test]
    fn spin_amount_zero_passes_through() {
        let mut fx = chain(Mod2Type::Spin);
        fx.mod2_amount = 0.0;
        for n in 0..1000 {
            let x = 0.4 * (n as f32 * 0.2).sin();
            assert_eq!(fx.process_frame([x, x]), [x, x]);
        }
    }

    #[test]
    fn ensemble_has_three_taps_and_variation_is_mono() {
        let impulse = |var: bool| {
            let mut fx = chain(Mod2Type::Ensemble);
            fx.mod2_rate_hz = 0.1;
            fx.mod2_variation = var;
            (0..1500)
                .map(|n| fx.process_frame(if n == 0 { [1.0, 1.0] } else { [0.0, 0.0] }))
                .collect::<Vec<_>>()
        };
        let stereo = impulse(false);
        let mut clusters = 0;
        let mut inside = false;
        for f in stereo.iter().skip(2) {
            let hit = f[0].abs() + f[1].abs() > 1e-4;
            if hit && !inside {
                clusters += 1;
            }
            inside = hit;
        }
        assert_eq!(clusters, 3);
        assert!(stereo.iter().any(|f| (f[0] - f[1]).abs() > 1e-3));
        assert!(impulse(true).iter().all(|f| (f[0] - f[1]).abs() < 1e-6));
    }

    #[test]
    fn ensemble_amount_zero_passes_through_and_stays_finite() {
        let mut fx = chain(Mod2Type::Ensemble);
        fx.mod2_amount = 0.0;
        for n in 0..1000 {
            let x = 0.4 * (n as f32 * 0.2).sin();
            assert_eq!(fx.process_frame([x, x]), [x, x]);
        }
        fx.mod2_amount = 1.0;
        assert!((0..9600).all(|n| fx.process_frame([(n as f32 * 0.1).sin(); 2])[0].is_finite()));
    }
}
