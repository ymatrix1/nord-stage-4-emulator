//! Smoothed lane mixer of the firmware mix stage: the four group gains `g5..g8` and the output
//! level move towards their targets with `s = 0.01 t + 0.99 s` per firmware frame.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.4. Research: `docs/research/organ-oracle-audio.md`
//! sections 3 and 5. The mixer is real-time safe: a few `f32` states, no allocation.

use super::vibrato_bank::{gain_value, GAINS_DETUNED, GAINS_EXACT, SMOOTH_KEEP};
use super::vibrato_link::VibratoLevel;

/// Frame rate of the firmware bank in Hz, used to convert the per-frame smoother to the engine
/// sample rate.
///
/// ASSUMED: 43,156.4 Hz, the rate at which wheel 0 of the exact ARM table equals the engine's
/// C1 (`firmware_bank::implied_frame_rate`, `organ_ref` default rate). The real DSP frame rate is
/// not known from the image; the only consequence is the time constant of the ramp
/// (100 frames = 2.32 ms here), which the VERIFIED coefficient fixes in frames, not seconds.
pub const FIRMWARE_FRAME_RATE_HZ: f64 = 43_156.4;

/// Gain differences below this (-86 dB; the f32 fixed point of the one-pole sits about 7e-6 off
/// the target) snap to the target, which settles the ramp.
const SNAP: f32 = 5e-5;

/// Per-sample keep factor equivalent to `SMOOTH_KEEP` per firmware frame: `0.99^(F / fs)`, so the
/// ramp lasts the same time in seconds at any engine sample rate. Exactly `0.99` when the engine
/// runs at the firmware frame rate.
pub fn keep_for_rate(fs: f32) -> f32 {
    let ratio = FIRMWARE_FRAME_RATE_HZ / f64::from(fs.max(1.0));
    f64::from(SMOOTH_KEEP).powf(ratio) as f32
}

/// Gains `[g5, g6, g7, g8]` programmed for a vibrato-enable level.
///
/// VERIFIED: level 1 sends [`GAINS_DETUNED`], any other level [`GAINS_EXACT`] (`0xC022D9F0`).
pub fn gains_for(level: VibratoLevel) -> [u32; 4] {
    if level.detuned() {
        GAINS_DETUNED
    } else {
        GAINS_EXACT
    }
}

/// Smoothed 2x2 lane matrix `[[g5, g8], [g7, g6]]` times the smoothed level.
#[derive(Debug, Clone)]
pub struct LaneMixer {
    gains: [f32; 4],
    target: [f32; 4],
    level: f32,
    keep: f32,
    settled: bool,
}

impl LaneMixer {
    /// Starts settled on the exact-table gains (copy 0 on both lanes, level 1).
    pub fn new(fs: f32) -> Self {
        let target = GAINS_EXACT.map(gain_value);
        Self {
            gains: target,
            target,
            level: 1.0,
            keep: keep_for_rate(fs),
            settled: true,
        }
    }

    /// Recomputes the per-sample smoothing factor for a new engine sample rate.
    pub fn set_rate(&mut self, fs: f32) {
        self.keep = keep_for_rate(fs);
    }

    /// Per-sample keep factor in use.
    pub fn keep(&self) -> f32 {
        self.keep
    }

    /// Selects the gain targets of a vibrato-enable level; the states move towards them.
    pub fn set_level(&mut self, level: VibratoLevel) {
        let target = gains_for(level).map(gain_value);
        if target != self.target {
            self.target = target;
            self.settled = false;
        }
    }

    /// `true` while the states are still moving or not on the exact-table gains.
    pub fn active(&self) -> bool {
        !self.settled || self.target != GAINS_EXACT.map(gain_value)
    }

    /// Advances one engine sample and returns the matrix `[[x<-copy0, x<-copy1],
    /// [y<-copy0, y<-copy1]]` scaled by the smoothed level. Post-update values are used, as in
    /// the firmware (VERIFIED: sentinel-word run).
    pub fn step(&mut self) -> [[f32; 2]; 2] {
        if !self.settled {
            let k = self.keep;
            let mut done = true;
            for (s, &t) in self.gains.iter_mut().zip(self.target.iter()) {
                *s = (1.0 - k) * t + k * *s;
                if (*s - t).abs() < SNAP {
                    *s = t;
                } else {
                    done = false;
                }
            }
            self.level = (1.0 - k) + k * self.level;
            if (self.level - 1.0).abs() < SNAP {
                self.level = 1.0;
            }
            self.settled = done && self.level == 1.0;
        }
        let [g5, g6, g7, g8] = self.gains.map(|g| g * self.level);
        [[g5, g8], [g7, g6]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_is_the_firmware_coefficient_at_the_firmware_rate() {
        let k = keep_for_rate(FIRMWARE_FRAME_RATE_HZ as f32);
        assert!((k - 0.99).abs() < 1e-6, "{k}");
        // same time constant in seconds: 100 frames at 43,156.4 Hz = 2.317 ms
        let k48 = f64::from(keep_for_rate(48_000.0));
        let tau_s = -1.0 / (48_000.0 * k48.ln());
        let tau_fw = -1.0 / (FIRMWARE_FRAME_RATE_HZ * 0.99f64.ln());
        assert!((tau_s - tau_fw).abs() / tau_fw < 1e-5, "{tau_s} {tau_fw}");
    }

    #[test]
    fn step_response_follows_the_one_pole() {
        let mut m = LaneMixer::new(FIRMWARE_FRAME_RATE_HZ as f32);
        m.set_level(VibratoLevel::DETUNED);
        let k = f64::from(m.keep());
        // g5 goes 1.0 -> 0.75: error decays as keep^n, 1/e after about 100 frames
        let mut first = 0.0;
        let mut at_tau = 0.0;
        for n in 1..=100 {
            let g5 = m.step()[0][0];
            if n == 1 {
                first = g5;
            }
            at_tau = g5;
        }
        let want1 = 0.75 + 0.25 * k;
        assert!((f64::from(first) - want1).abs() < 2e-6, "{first}");
        let frac = (f64::from(at_tau) - 0.75) / 0.25;
        assert!((frac - (-1.0f64).exp()).abs() < 0.01, "{frac}");
    }

    #[test]
    fn settles_on_both_gain_sets() {
        let mut m = LaneMixer::new(48_000.0);
        assert!(!m.active());
        m.set_level(VibratoLevel::DETUNED);
        assert!(m.active());
        for _ in 0..5000 {
            m.step();
        }
        assert_eq!(m.step(), [[0.75, 0.375], [0.375, 0.75]]);
        m.set_level(VibratoLevel(0));
        assert!(m.active());
        for _ in 0..5000 {
            m.step();
        }
        let e = m.step();
        assert!((e[0][0] - 1.0).abs() < 1e-6 && (e[1][0] - 1.0).abs() < 1e-6);
        assert_eq!((e[0][1], e[1][1]), (0.0, 0.0));
        assert!(!m.active());
    }

    #[test]
    fn gains_for_selects_by_level() {
        assert_eq!(gains_for(VibratoLevel(1)), GAINS_DETUNED);
        for l in [0, 2, 3, 7] {
            assert_eq!(gains_for(VibratoLevel(l)), GAINS_EXACT);
        }
    }
}
