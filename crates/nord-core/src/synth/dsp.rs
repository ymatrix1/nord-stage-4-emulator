//! Synth DSP primitives: FM ratio table, PolyBLEP, unison panning and the ZDF filter.

use super::*;

/// Classic FM harmonic and inharmonic carrier:modulator ratios.
pub const FM_RATIOS: [f32; 16] = [
    0.5,
    1.0,
    1.5,
    2.0,
    3.0,
    4.0,
    5.0,
    6.0,
    7.0,
    8.0,
    std::f32::consts::SQRT_2,
    std::f32::consts::E,
    PI,
    1.25,
    2.5,
    3.5,
];

/// Left/right gains of a unison voice: full level in its strong channel, `weak` in the other.
#[inline]
pub(super) fn unison_pan(strong_left: bool, weak: f32, norm: f32) -> (f32, f32) {
    if strong_left {
        (norm, norm * weak)
    } else {
        (norm * weak, norm)
    }
}

/// 2-point Polynomial Band-Limited Step (`PolyBLEP`) anti-aliasing residual.
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Topology-preserving Zero-Delay Feedback (`ZDF`) 4-pole Transistor Ladder & SVF filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZdfFilter {
    s: [f32; 4],
}

impl ZdfFilter {
    /// Processes one sample through the selected ZDF filter model.
    pub fn process(
        &mut self,
        input: f32,
        model: FilterModel,
        cutoff_hz: f32,
        resonance: f32,
        drive: f32,
        sample_rate: f32,
    ) -> f32 {
        let fc = cutoff_hz.clamp(14.0, 0.48 * sample_rate);
        let g = (PI * fc / sample_rate).tan();
        let g1 = g / (1.0 + g);

        match model {
            FilterModel::Ladder24 | FilterModel::Lp24 => {
                let k = (resonance.clamp(0.0, 1.0)) * 3.85;
                let big_g = g1 * g1 * g1 * g1;
                let s_sum = (g1 * g1 * g1) * (self.s[0] / (1.0 + g))
                    + (g1 * g1) * (self.s[1] / (1.0 + g))
                    + g1 * (self.s[2] / (1.0 + g))
                    + (self.s[3] / (1.0 + g));
                let driven = ((1.0 + 2.0 * drive) * input).tanh();
                let mut u = (driven - k * s_sum) / (1.0 + k * big_g);
                for st in &mut self.s {
                    let v = (u - *st) * g1;
                    let y = v + *st;
                    *st = y + v;
                    u = y;
                }
                u
            }
            FilterModel::Lp12 | FilterModel::BandPass | FilterModel::HighPass => {
                let r = (1.0 - 0.9 * resonance.clamp(0.0, 1.0)).max(0.08);
                let hp =
                    (input - (2.0 * r + g) * self.s[0] - self.s[1]) / (1.0 + 2.0 * r * g + g * g);
                let bp = g * hp + self.s[0];
                self.s[0] = g * hp + bp;
                let lp = g * bp + self.s[1];
                self.s[1] = g * bp + lp;
                match model {
                    FilterModel::Lp12 => lp,
                    FilterModel::BandPass => bp,
                    FilterModel::HighPass => hp,
                    _ => lp,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polyblep_zero_outside_discontinuity() {
        assert_eq!(poly_blep(0.5, 0.01), 0.0);
        assert!(poly_blep(0.002, 0.01) < 0.0);
        assert!(poly_blep(0.998, 0.01) > 0.0);
    }

    #[test]
    fn test_zdf_ladder24_attenuates_high_frequencies() {
        let mut filter = ZdfFilter::default();
        let fs = 48_000.0;
        let mut energy = 0.0f32;
        for n in 0..512 {
            let x = (TAU * 12_000.0 * (n as f32) / fs).sin();
            let y = filter.process(x, FilterModel::Ladder24, 400.0, 0.2, 0.0, fs);
            assert!(y.is_finite());
            if n >= 128 {
                energy += y * y;
            }
        }
        let rms = (energy / 384.0).sqrt();
        assert!(
            rms < 0.02,
            "12 kHz signal must be strongly attenuated by 400 Hz 24dB Ladder (rms={rms})"
        );
    }
}
