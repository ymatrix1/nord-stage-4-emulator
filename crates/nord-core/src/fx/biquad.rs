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
