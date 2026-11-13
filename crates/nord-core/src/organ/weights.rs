//! Firmware tonewheel weights: per-wheel targets `wB` from the held keys and drawbars, and the
//! one-pole smoothing the DSP mix stage applies to them.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.5. Research: `docs/research/organ-weights.md`
//! (targets, curve, smoothing coefficients) and `docs/research/organ-oracle-audio.md` section 3
//! (mix stage). Real-time safe: fixed arrays, no allocation, no locks.

use super::drawbar::{foldback_tonewheel, DRAWBAR_OFFSETS, FIRMWARE_DRAWBAR_CURVE};
use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;
use super::types::NUM_TONEWHEELS;

/// How the per-wheel drawbar weights are formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeightMode {
    /// Instant per-key gains with the ASSUMED `0.08` bus scale and pickup calibration (default).
    #[default]
    Legacy,
    /// Firmware weights: `wB` summed per wheel over keys and drawbars, one-pole smoothed per
    /// wheel, absolute scale `2^-23`, no pickup calibration.
    Firmware,
}

/// Scale of the integer weight sums: `wB = sum * 2^-23`.
///
/// VERIFIED: sender conversion `0xC001FF18` (`N * 2^-23`), 128-note bit-exact check
/// (`python tools/arm_organ_weights.py verify`, `docs/research/organ-weights.md` section 2).
pub const WEIGHT_SCALE: f32 = 1.0 / 8_388_608.0;

/// `wB` of one key at velocity 127 on a drawbar at position 8: `127 * ((127 * 127) >> 4) * 2^-23`.
///
/// VERIFIED: 0.0152607 = `organ-weight-curve.csv` row 8 (`wB_bits` equal to the model).
pub const UNIT_WEIGHT: f32 = 127.0 * 1008.0 * WEIGHT_SCALE;

/// Smoothing coefficient `a` the key-down handler sends (`0x3C1DF380`, 0.0096406).
///
/// VERIFIED: ARM run, independent of note and velocity (`docs/research/organ-weights.md` s2).
pub const A_KEY_DOWN_BITS: u32 = 0x3C1D_F380;
/// Smoothing coefficient `a` of a full update (`0x3B8BF800`, 0.0042715).
///
/// VERIFIED: ARM run, every wheel (`docs/research/organ-weights.md` section 2).
pub const A_UPDATE_BITS: u32 = 0x3B8B_F800;

/// Gap below which a wheel weight snaps onto its target (6.5e-5 of one key, -84 dB; the f32 fixed
/// point of the one-pole stalls about 2e-7 off at 48 kHz).
const SNAP: f32 = 1e-6;

/// `a` of a key-down.
pub fn a_key_down() -> f64 {
    f64::from(f32::from_bits(A_KEY_DOWN_BITS))
}

/// `a` of a full update.
pub fn a_update() -> f64 {
    f64::from(f32::from_bits(A_UPDATE_BITS))
}

/// Per-sample keep factor `(1 - a)^(F / fs)` of a per-frame coefficient `a` (`wA <- a wB + b wA`,
/// `a + b = 1`), so the ramp lasts the same time in seconds at any sample rate. Exactly `1 - a`
/// at the firmware frame rate.
///
/// ASSUMED: frame rate 43,156.4 Hz (`lane_mix::FIRMWARE_FRAME_RATE_HZ`).
pub fn keep_for(a: f64, fs: f32) -> f32 {
    (1.0 - a).powf(FIRMWARE_FRAME_RATE_HZ / f64::from(fs.max(1.0))) as f32
}

/// Velocity term of the firmware sum: `(vel * 127) >> 4` (taper 127, no tapering).
pub fn velocity_term(velocity: u8) -> u32 {
    (u32::from(velocity.min(127)) * 127) >> 4
}

/// Integer weight contribution `d7 * ((vel * 127) >> 4)` of one key on one drawbar position.
pub fn weight_sum(level: u8, velocity: u8) -> u32 {
    u32::from(FIRMWARE_DRAWBAR_CURVE[usize::from(level.min(8))]) * velocity_term(velocity)
}

/// Wheel of drawbar `j` of a key sounding `midi` (already octave shifted).
pub fn wheel_of(midi: i32, drawbar: usize) -> usize {
    foldback_tonewheel(midi + DRAWBAR_OFFSETS[drawbar])
}

/// Targets `wB[w]` of every wheel: `2^-23 * sum over keys and drawbars on w of d7 * ((vel*127)>>4)`.
/// `keys` are `(midi, velocity)` of the held keys; chords add on shared wheels.
pub fn targets<I>(keys: I, drawbars: &[u8; 9]) -> [f32; NUM_TONEWHEELS]
where
    I: IntoIterator<Item = (i32, u8)>,
{
    let mut sums = [0u32; NUM_TONEWHEELS];
    for (midi, vel) in keys {
        for (j, &level) in drawbars.iter().enumerate() {
            sums[wheel_of(midi, j)] += weight_sum(level, vel);
        }
    }
    sums.map(|s| s as f32 * WEIGHT_SCALE)
}

/// Smoothed per-wheel weights `wA` moving towards `wB`.
#[derive(Debug, Clone)]
pub struct WheelWeights {
    wa: [f32; NUM_TONEWHEELS],
    wb: [f32; NUM_TONEWHEELS],
    fast: [bool; NUM_TONEWHEELS],
    keep_fast: f32,
    keep_slow: f32,
    settled: bool,
}

impl WheelWeights {
    /// All weights zero, settled, for engine rate `fs`.
    pub fn new(fs: f32) -> Self {
        Self {
            wa: [0.0; NUM_TONEWHEELS],
            wb: [0.0; NUM_TONEWHEELS],
            fast: [false; NUM_TONEWHEELS],
            keep_fast: keep_for(a_key_down(), fs),
            keep_slow: keep_for(a_update(), fs),
            settled: true,
        }
    }

    /// Recomputes the per-sample factors for a new engine rate.
    pub fn set_rate(&mut self, fs: f32) {
        self.keep_fast = keep_for(a_key_down(), fs);
        self.keep_slow = keep_for(a_update(), fs);
    }

    /// Marks `wheel` as written by a key-down (`a = 0.0096`) until the next full update.
    pub fn mark_key_down(&mut self, wheel: usize) {
        self.fast[wheel.min(NUM_TONEWHEELS - 1)] = true;
    }

    /// A full update: every wheel uses `a = 0.0043` from now on, with the new targets.
    #[cfg(test)]
    pub fn full_update(&mut self, targets: &[f32; NUM_TONEWHEELS]) {
        self.clear_fast();
        self.set_targets(targets);
    }

    /// Returns every wheel to the full-update coefficient `a = 0.0043`.
    pub fn clear_fast(&mut self) {
        self.fast = [false; NUM_TONEWHEELS];
    }

    /// New targets without touching the per-wheel coefficients (after a key-down).
    pub fn set_targets(&mut self, targets: &[f32; NUM_TONEWHEELS]) {
        self.wb = *targets;
        self.settled = false;
    }

    /// Smoothed weights `wA`.
    #[cfg(test)]
    pub fn current(&self) -> &[f32; NUM_TONEWHEELS] {
        &self.wa
    }

    /// `true` while any wheel is still moving.
    pub fn active(&self) -> bool {
        !self.settled
    }

    /// Advances one engine sample: `wA <- (1 - k) wB + k wA` per wheel (post-update value is used).
    pub fn step(&mut self) -> &[f32; NUM_TONEWHEELS] {
        if !self.settled {
            let mut done = true;
            for ((wa, &wb), &fast) in self.wa.iter_mut().zip(&self.wb).zip(&self.fast) {
                let k = if fast { self.keep_fast } else { self.keep_slow };
                *wa = (1.0 - k) * wb + k * *wa;
                if (*wa - wb).abs() < SNAP {
                    *wa = wb;
                } else {
                    done = false;
                }
            }
            self.settled = done;
        }
        &self.wa
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_weight_is_the_verified_single_key_value() {
        assert!((UNIT_WEIGHT - 0.015_260_7).abs() < 1e-7, "{UNIT_WEIGHT}");
        let w = targets([(60, 127)], &[0, 0, 8, 0, 0, 0, 0, 0, 0]);
        assert_eq!(w[36], UNIT_WEIGHT);
        assert_eq!(w.iter().filter(|&&x| x != 0.0).count(), 1);
    }

    #[test]
    fn curve_velocity_and_chords_follow_the_formula() {
        // organ-weight-curve.csv: key 60, drawbar 8' at positions 0..8
        let want = [
            0.0, 3.6049e-4, 7.2098e-4, 1.5621e-3, 2.6436e-3, 4.6864e-3, 7.4501e-3, 1.04542e-2,
            1.52607e-2,
        ];
        for (level, &w) in want.iter().enumerate() {
            let t = targets([(60, 127)], &[0, 0, level as u8, 0, 0, 0, 0, 0, 0]);
            assert!(
                (t[36] - w).abs() < 1e-8 + 1e-4 * w,
                "level {level}: {}",
                t[36]
            );
        }
        // velocities 1, 16, 32, 64 (organ-weights.md section 5)
        for (vel, w) in [
            (1u8, 1.0598e-4f32),
            (16, 1.9227e-3),
            (32, 3.8455e-3),
            (64, 7.6909e-3),
        ] {
            let t = targets([(60, vel)], &[0, 0, 8, 0, 0, 0, 0, 0, 0]);
            assert!((t[36] - w).abs() < 1e-4 * w, "vel {vel}: {}", t[36]);
        }
        // C4 E4 G4 with 888: wheel 43 = G4 8' + C4 5 1/3' carries twice the single weight
        let t = targets(
            [(60, 127), (64, 127), (67, 127)],
            &[8, 8, 8, 0, 0, 0, 0, 0, 0],
        );
        assert_eq!(t[43], 2.0 * UNIT_WEIGHT);
        assert_eq!(t[24], UNIT_WEIGHT);
        assert_eq!(t[NUM_TONEWHEELS - 1], 0.0);
    }

    #[test]
    fn step_response_follows_the_one_pole_for_both_coefficients() {
        for (fast, a) in [(true, a_key_down()), (false, a_update())] {
            let mut w = WheelWeights::new(FIRMWARE_FRAME_RATE_HZ as f32);
            let mut t = [0.0; NUM_TONEWHEELS];
            t[10] = 1.0;
            if fast {
                w.mark_key_down(10);
                w.set_targets(&t);
            } else {
                w.full_update(&t);
            }
            for n in 1..=300 {
                let got = f64::from(w.step()[10]);
                let want = 1.0 - (1.0 - a).powi(n);
                assert!((got - want).abs() < 2e-6, "a {a} n {n}: {got} vs {want}");
            }
            // 1/e of the gap after 1/a frames
            let mut w2 = WheelWeights::new(FIRMWARE_FRAME_RATE_HZ as f32);
            if fast {
                w2.mark_key_down(10);
            }
            w2.set_targets(&t);
            let n = (1.0 / a).round() as usize;
            let mut got = 0.0;
            for _ in 0..n {
                got = w2.step()[10];
            }
            assert!(
                (f64::from(got) - (1.0 - (-1.0f64).exp())).abs() < 0.01,
                "{got}"
            );
        }
    }

    #[test]
    fn same_seconds_at_other_sample_rates_and_settles_to_exact_targets() {
        let k48 = f64::from(keep_for(a_update(), 48_000.0));
        let tau_s = -1.0 / (48_000.0 * k48.ln());
        let tau_fw = -1.0 / (FIRMWARE_FRAME_RATE_HZ * (1.0 - a_update()).ln());
        assert!((tau_s - tau_fw).abs() / tau_fw < 1e-5);
        let mut w = WheelWeights::new(48_000.0);
        let mut t = [0.0; NUM_TONEWHEELS];
        t[3] = UNIT_WEIGHT;
        w.full_update(&t);
        assert!(w.active());
        for _ in 0..20_000 {
            w.step();
        }
        assert_eq!(w.current()[3], UNIT_WEIGHT);
        assert!(!w.active());
        w.full_update(&[0.0; NUM_TONEWHEELS]);
        for _ in 0..20_000 {
            w.step();
        }
        assert_eq!(w.current()[3], 0.0);
    }
}
