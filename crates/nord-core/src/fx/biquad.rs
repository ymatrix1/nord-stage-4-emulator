//! RBJ biquad designs and the firmware resonance table.

/// Hardware resonance (`Q`) table length (`0x2f970c` in OS v1.66 `bank0_CODE.bin`).
pub const FW_Q_STEPS: usize = 128;

/// Firmware log-scale resonance: `Q(idx) = 0.1 * 100^(idx / 127)` (`0.1 .. 10.0`).
///
/// VERIFIED: all 128 `f32` entries at `0x2f970c` of OS v1.66 `bank0_CODE.bin`, max rel. error `6.4e-8`.
/// Check: Python `struct.unpack('<128f')` vs formula (see `docs/research/sharc-dsp-and-preset-hash-teardown.md`);
/// unit test `test_firmware_q_table_endpoints_and_monotonic` pins endpoints only.
#[inline]
pub fn firmware_q(idx: usize) -> f32 {
    let i = idx.min(FW_Q_STEPS - 1) as f64;
    (0.1 * 100.0f64.powf(i / 127.0)) as f32
}

/// Filter shapes for [`Biquad`] designs (RBJ Audio-EQ-Cookbook forms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BiquadKind {
    LowShelf,
    HighShelf,
    Peak,
    LowPass,
    HighPass,
}

/// Designs normalized 2nd-order coefficients `[b0, b1, b2, a1, a2]` (`a0 = 1`).
///
/// `fc` is clamped to `[10, 0.49 * fs]` and `q` to `[0.1, 10]` so poles stay inside the unit circle.
pub(super) fn design_biquad(kind: BiquadKind, fc: f32, fs: f32, q: f32, gain_db: f32) -> [f32; 5] {
    let fs = f64::from(fs);
    let fc = f64::from(fc).clamp(10.0, 0.49 * fs);
    let q = f64::from(q).clamp(0.1, 10.0);
    let a = 10.0f64.powf(f64::from(gain_db) / 40.0);
    let w0 = std::f64::consts::TAU * fc / fs;
    let (sin_w, cos_w) = w0.sin_cos();
    let alpha = sin_w / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match kind {
        BiquadKind::Peak => (
            1.0 + alpha * a,
            -2.0 * cos_w,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w,
            1.0 - alpha / a,
        ),
        BiquadKind::LowShelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w + t),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w),
                a * ((a + 1.0) - (a - 1.0) * cos_w - t),
                (a + 1.0) + (a - 1.0) * cos_w + t,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w),
                (a + 1.0) + (a - 1.0) * cos_w - t,
            )
        }
        BiquadKind::HighShelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w + t),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w),
                a * ((a + 1.0) + (a - 1.0) * cos_w - t),
                (a + 1.0) - (a - 1.0) * cos_w + t,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w),
                (a + 1.0) - (a - 1.0) * cos_w - t,
            )
        }
        BiquadKind::LowPass => (
            (1.0 - cos_w) * 0.5,
            1.0 - cos_w,
            (1.0 - cos_w) * 0.5,
            1.0 + alpha,
            -2.0 * cos_w,
            1.0 - alpha,
        ),
        BiquadKind::HighPass => (
            (1.0 + cos_w) * 0.5,
            -(1.0 + cos_w),
            (1.0 + cos_w) * 0.5,
            1.0 + alpha,
            -2.0 * cos_w,
            1.0 - alpha,
        ),
    };
    [
        (b0 / a0) as f32,
        (b1 / a0) as f32,
        (b2 / a0) as f32,
        (a1 / a0) as f32,
        (a2 / a0) as f32,
    ]
}

/// Stereo transposed direct-form II biquad (shared coefficients, per-channel state).
#[derive(Debug, Clone, Copy)]
pub(super) struct Biquad {
    pub(super) c: [f32; 5],
    pub(super) z: [[f32; 2]; 2],
}

impl Biquad {
    pub(super) const fn identity() -> Self {
        Self {
            c: [1.0, 0.0, 0.0, 0.0, 0.0],
            z: [[0.0; 2]; 2],
        }
    }

    #[inline]
    pub(super) fn set(&mut self, kind: BiquadKind, fc: f32, fs: f32, q: f32, gain_db: f32) {
        self.c = design_biquad(kind, fc, fs, q, gain_db);
    }

    #[inline]
    pub(super) fn process(&mut self, ch: usize, x: f32) -> f32 {
        let [b0, b1, b2, a1, a2] = self.c;
        let z = &mut self.z[ch];
        let y = b0 * x + z[0];
        z[0] = b1 * x - a1 * y + z[1];
        z[1] = b2 * x - a2 * y;
        if y.is_finite() {
            y
        } else {
            *z = [0.0; 2];
            0.0
        }
    }
}

/// Butterworth low pass at `fs / 4` read from the FX DSP image (`b0 b1 b2 a1 a2`, float32 words).
///
/// VERIFIED: PM `0x9437A` (the coefficients routine `0x120FDB` reads at `I9`), checked by
/// `tools/fx_biquad_check.py` (`--golden` regenerates the vectors) and the test
/// `ext40_reference_matches_the_firmware_golden_vectors`, 0 differing words. The design values are
/// `b0 = b2 = 0.292893`, `b1 = 0.585786`, `a1 = 0`, `a2 = 0.171573` (`docs/research/fx-dsp-map.md`
/// section 6); `b2` differs from `b0` by 7 ulp in the image, kept as found. The set at `0x9437F`
/// is the same filter. Used by `fw_rvb_wrap` (the reverb half-rate wrapper); the master bus has no 4-biquad IIR.
pub(super) const FW_BUTTERWORTH_FS4_WORDS: [u32; 5] =
    [0x3E95_F619, 0x3F15_F619, 0x3E95_F612, 0, 0x3E2F_B0D5];

/// The DSP routine's transposed direct form II arithmetic, bit for bit (test-only reference).
///
/// VERIFIED: 40-bit register model of `0x120FDB` (`tools/fx_biquad_check.py`): every product and sum
/// rounded to 32 significant bits, ties to even (`round_sig`); the stage reads and writes memory
/// words, so the output sample and the final state are truncated to 24 significant bits. Not the
/// render path (f64 ops, slow); [`Biquad::process`] is the fast f32 default.
#[cfg(test)]
pub(super) mod ext40_ref {
    use crate::sinc::ext40::round_sig;

    /// Register-format rounding: 32 significant bits, nearest even.
    fn reg(v: f64) -> f64 {
        round_sig(v, 32, false)
    }

    /// Memory-store truncation: upper 32 bits of the register (24 significant bits).
    fn store(v: f64) -> f64 {
        round_sig(v, 24, true)
    }

    /// One stage over `x` in place. `state` is `[s1, s2]` as memory words, loaded once and stored
    /// once per call (the routine does this per block of 4 frames).
    pub(in crate::fx) fn stage(x: &mut [f32], c: &[f32; 5], state: &mut [f32; 2]) {
        let [b0, b1, b2, a1, a2] = c.map(f64::from);
        let (mut s1, mut s2) = (f64::from(state[0]), f64::from(state[1]));
        for v in x.iter_mut() {
            let xi = f64::from(*v);
            let y = reg(reg(b0 * xi) + s1);
            let t = reg(reg(b1 * xi) + s2);
            s1 = reg(t - reg(a1 * y));
            s2 = reg(reg(b2 * xi) - reg(a2 * y));
            *v = store(y) as f32;
        }
        *state = [store(s1) as f32, store(s2) as f32];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::support::*;

    #[test]
    fn test_firmware_q_table_endpoints_and_monotonic() {
        assert!((firmware_q(0) - 0.1).abs() < 1e-6);
        assert!((firmware_q(127) - 10.0).abs() < 1e-4);
        assert!((firmware_q(64) - 1.018_296).abs() < 1e-4);
        assert_eq!(firmware_q(500), firmware_q(127));
        assert!((1..FW_Q_STEPS).all(|i| firmware_q(i) > firmware_q(i - 1)));
    }

    #[test]
    fn test_biquad_eq_frequency_response() {
        // Bass +12 dB shelf: boosts 40 Hz by ~12 dB, leaves 4 kHz flat
        let mut fx = eq_only(12.0, 0.0, 1000.0, 0.0);
        assert!((tone_gain_db(&mut fx, 40.0) - 12.0).abs() < 1.5);
        let mut fx = eq_only(12.0, 0.0, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 4000.0).abs() < 0.5);
        // Treble -9 dB shelf
        let mut fx = eq_only(0.0, 0.0, 1000.0, -9.0);
        assert!((tone_gain_db(&mut fx, 12_000.0) + 9.0).abs() < 1.5);
        let mut fx = eq_only(0.0, 0.0, 1000.0, -9.0);
        assert!(tone_gain_db(&mut fx, 200.0).abs() < 0.5);
        // Mid peak +10 dB @ 2 kHz is exact at center and narrow (2nd-order)
        let mut fx = eq_only(0.0, 10.0, 2000.0, 0.0);
        assert!((tone_gain_db(&mut fx, 2000.0) - 10.0).abs() < 0.3);
        let mut fx = eq_only(0.0, 10.0, 2000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 200.0).abs() < 1.0);
    }

    #[test]
    fn test_biquad_eq_preserves_stereo_image() {
        let mut fx = eq_only(6.0, 6.0, 1000.0, 6.0);
        let mut right_energy = 0.0f32;
        for i in 0..2000 {
            let x = (i as f32 * 0.07).sin() * 0.3;
            let [_, r] = fx.process_frame([x, 0.0]);
            right_energy += r * r;
        }
        assert!(
            right_energy < 1e-12,
            "silent right channel must stay silent"
        );
    }

    #[test]
    fn test_biquad_stable_at_extremes() {
        for (fc, db) in [
            (10.0, 15.0),
            (200.0, -15.0),
            (24_000.0, 15.0),
            (8000.0, 15.0),
        ] {
            for kind in [
                BiquadKind::LowShelf,
                BiquadKind::HighShelf,
                BiquadKind::Peak,
                BiquadKind::LowPass,
                BiquadKind::HighPass,
            ] {
                let c = design_biquad(kind, fc, 48_000.0, 10.0, db);
                assert!(c.iter().all(|v| v.is_finite()));
                // Stability triangle: |a2| < 1, |a1| < 1 + a2
                assert!(
                    c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4],
                    "{kind:?} {fc} {db}"
                );
            }
        }
    }
}
