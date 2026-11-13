//! Vox tone source: the note bank (`vox_bank`), the weighted sum of the 96-word block and the
//! output stage (`vox_output`).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7. The output stage `0x1229B3` forms
//! `T = sum_k w_k Y_k` over the 84 note signals (the 12 sine buckets carry weight 0 here, see the
//! spec) and passes it through the three sections. The weights are delivered by the ARM
//! (`0xC0224468`, `docs/research/pno-full-pass.md` section 6); the key to oscillator map and the
//! per-key weights are not decoded, so a held key `n` drives oscillator `n - 24` with weight
//! 1.0 times its velocity gain (ASSUMED, the map used by the oracle runs of the research notes).

use super::vox_bank::{VoxBank, NOTES};
use super::vox_output::VoxOutput;

/// ASSUMED: weights ramp linearly to their targets in 2 ms (the firmware weight writes are
/// instantaneous; the ramp only avoids a click, the oscillators run freely in both).
const WEIGHT_RAMP_SECONDS: f32 = 0.002;

/// Vox tone source state (all arrays preallocated).
#[derive(Debug, Clone)]
pub struct VoxSource {
    bank: VoxBank,
    output: VoxOutput,
    weights: [f32; NOTES],
    block: [f32; NOTES],
    ramp_step: f32,
    fine_cents: f32,
    pitch: f32,
}

impl VoxSource {
    /// Source for engine rate `fs`.
    pub fn new(fs: f32) -> Self {
        let mut s = Self {
            bank: VoxBank::new(fs),
            output: VoxOutput::new(fs),
            weights: [0.0; NOTES],
            block: [0.0; NOTES],
            ramp_step: 0.0,
            fine_cents: 0.0,
            pitch: 1.0,
        };
        s.set_sample_rate(fs);
        s
    }

    /// Engine sample rate the source was set for.
    pub fn sample_rate(&self) -> f32 {
        self.bank.sample_rate()
    }

    /// Re-derives the rate dependent parts (states kept).
    pub fn set_sample_rate(&mut self, fs: f32) {
        self.bank.set_sample_rate(fs);
        self.output.set_sample_rate(fs);
        self.ramp_step = 1.0 / (WEIGHT_RAMP_SECONDS * fs.max(8000.0));
    }

    /// Fine Tune in cents (`DM(0xE20E0)`): pitch factor `2^(cents / 1200)`.
    pub fn set_fine_cents(&mut self, cents: f32) {
        if cents != self.fine_cents {
            self.fine_cents = cents;
            self.pitch = (f64::from(cents) / 1200.0).exp2() as f32;
        }
    }

    /// One sample for the target weights of the 84 oscillators and the vibrato depth (pitch
    /// fraction, `DM(0xE20E3)`).
    pub fn render(&mut self, targets: &[f32; NOTES], depth: f32) -> f32 {
        self.bank.step(depth, self.pitch, &mut self.block);
        let mut sum = 0.0f32;
        for ((w, &target), &y) in self.weights.iter_mut().zip(targets).zip(&self.block) {
            let d = target - *w;
            *w = if d.abs() <= self.ramp_step {
                target
            } else {
                *w + self.ramp_step.copysign(d)
            };
            sum += *w * y;
        }
        self.output.process(sum)
    }

    /// Current weight of oscillator `k` (test view).
    #[cfg(test)]
    pub(super) fn weight(&self, k: usize) -> f32 {
        self.weights[k]
    }

    /// `true` when every weight is zero and the output sections have decayed.
    pub fn is_idle(&self) -> bool {
        self.weights.iter().all(|&w| w == 0.0) && self.output.is_idle()
    }

    /// The note bank (test view).
    #[cfg(test)]
    pub(super) fn bank(&self) -> &VoxBank {
        &self.bank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &mut VoxSource, targets: &[f32; NOTES], n: usize) -> Vec<f32> {
        (0..n).map(|_| src.render(targets, 0.0)).collect()
    }

    #[test]
    fn silence_without_weights() {
        let mut s = VoxSource::new(48_000.0);
        let out = run(&mut s, &[0.0; NOTES], 5000);
        assert!(out.iter().all(|&v| v == 0.0));
        assert!(s.is_idle());
    }

    #[test]
    fn weights_ramp_in_two_milliseconds_and_release_to_exact_silence() {
        let mut s = VoxSource::new(48_000.0);
        let mut t = [0.0f32; NOTES];
        t[36] = 1.0;
        run(&mut s, &t, 47);
        assert!(s.weight(36) < 1.0);
        run(&mut s, &t, 100);
        assert_eq!(s.weight(36), 1.0);
        assert!(!s.is_idle());
        let out = run(&mut s, &[0.0; NOTES], 300_000);
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(out[out.len() - 1000..].iter().all(|&v| v == 0.0));
        assert!(s.is_idle());
    }

    #[test]
    fn fine_tune_scales_the_phase_advance() {
        let mut s = VoxSource::new(48_000.0);
        s.set_fine_cents(100.0);
        let p0 = s.bank().signed_phase(40);
        s.render(&[0.0; NOTES], 0.0);
        let p1 = s.bank().signed_phase(40);
        let mut s2 = VoxSource::new(48_000.0);
        s2.render(&[0.0; NOTES], 0.0);
        let q1 = s2.bank().signed_phase(40);
        assert_eq!(p0, 0.0);
        let ratio = f64::from(p1) / f64::from(q1);
        assert!((ratio - 2.0f64.powf(1.0 / 12.0)).abs() < 1e-4, "{ratio}");
    }

    #[test]
    fn a_chord_has_its_three_fundamentals_at_twelve_tet_ratios() {
        let fs = 48_000.0f32;
        let mut s = VoxSource::new(fs);
        let mut t = [0.0f32; NOTES];
        for n in [60usize, 64, 67] {
            t[n - 24] = 1.0;
        }
        let out = run(&mut s, &t, 48_000 * 2);
        let tail = &out[48_000..];
        let base = 261.6256 * 2.0f64.powf(-9.9 / 1200.0);
        let level = |hz: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            let n = tail.len();
            for (i, &v) in tail.iter().enumerate() {
                let w = 2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(fs);
                let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
                re += f64::from(v) * win * w.cos();
                im += f64::from(v) * win * w.sin();
            }
            re.hypot(im)
        };
        // the bank is 9.9 cents flat of 12-TET C (docs/research/pno-full-pass.md section 4)
        let peak = |center: f64| {
            (-30..=30)
                .map(|d| center * (1.0 + f64::from(d) * 0.0004))
                .fold((0.0, 0.0), |b, hz| {
                    let l = level(hz);
                    if l > b.0 {
                        (l, hz)
                    } else {
                        b
                    }
                })
                .1
        };
        let c = peak(base);
        let e = peak(base * 2.0f64.powf(4.0 / 12.0));
        let g = peak(base * 2.0f64.powf(7.0 / 12.0));
        assert!((e / c - 2.0f64.powf(4.0 / 12.0)).abs() < 2e-3, "{}", e / c);
        assert!((g / c - 2.0f64.powf(7.0 / 12.0)).abs() < 2e-3, "{}", g / c);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 1.0));
    }
}
