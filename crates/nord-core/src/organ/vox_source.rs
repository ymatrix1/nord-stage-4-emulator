//! Vox tone source: the note bank (`vox_bank`), the weighted sum of the 96-word block and the
//! output stage (`vox_output`).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7. The output stage `0x1229B3` forms
//! `T = sum_k w_k Y_k` over the 84 note signals and the 12 sine buckets (`vox_sines`) and passes it
//! through the three sections. The weights are delivered by the ARM
//! (`0xC0224468`, `docs/research/pno-full-pass.md` section 6); the key to row map and the per-key
//! weights are `vox_keys` (VERIFIED against the captured key-down, `docs/research/keydown-zones.md`
//! section 5): a held key `n` drives the rows `fold(n - 36 + I[slot])` with the default level
//! slots and no velocity.

use super::vox_bank::{VoxBank, NOTES};
use super::vox_keys::ROWS;
use super::vox_output::VoxOutput;
use super::vox_sines::{VoxSines, BUCKETS};

/// VERIFIED: gain word `DM(0x95FD1)` = `DM(G + 0xB)` of record 0 (`0x3F1EB100`, 0.61989) written by the
/// ARM boot for the default program. Found by bisecting the 29,758 boot words of the PNO core 1
/// state: with it the oracle chord is 4.15 dB below the install-only chord (RMS ratio 0.6199),
/// without it equal (`docs/research/organ-vox-click.md` section 3). That it follows the layer
/// level (about `(level / 127)^2`) is ASSUMED: only the default level was captured.
pub(super) const OUTPUT_LEVEL: f32 = 0.619_89;

/// Vox tone source state (all arrays preallocated).
///
/// The weights are applied the way the firmware writes them: instantly (VERIFIED: the output
/// stage `0x1229B3` reads `DM(0x95E46 + row)` as it is, no routine smooths it), so a key-down is
/// a step of the weight on a free-running oscillator.
#[derive(Debug, Clone)]
pub struct VoxSource {
    bank: VoxBank,
    sines: VoxSines,
    output: VoxOutput,
    block: [f32; NOTES],
    buckets: [f32; BUCKETS],
    active: bool,
    fine_cents: f32,
    pitch: f32,
}

impl VoxSource {
    /// Source for engine rate `fs`, oscillators at the image start phases.
    pub fn new(fs: f32) -> Self {
        let mut bank = VoxBank::new(fs);
        bank.load_start_state();
        Self {
            bank,
            sines: VoxSines::new(fs),
            output: VoxOutput::new(fs),
            block: [0.0; NOTES],
            buckets: [0.0; BUCKETS],
            active: false,
            fine_cents: 0.0,
            pitch: 1.0,
        }
    }

    /// Engine sample rate the source was set for.
    pub fn sample_rate(&self) -> f32 {
        self.bank.sample_rate()
    }

    /// Re-derives the rate dependent parts (states kept).
    pub fn set_sample_rate(&mut self, fs: f32) {
        self.bank.set_sample_rate(fs);
        self.sines.set_sample_rate(fs);
        self.output.set_sample_rate(fs);
    }

    /// Fine Tune in cents (`DM(0xE20E0)`): pitch factor `2^(cents / 1200)`.
    pub fn set_fine_cents(&mut self, cents: f32) {
        if cents != self.fine_cents {
            self.fine_cents = cents;
            self.pitch = (f64::from(cents) / 1200.0).exp2() as f32;
        }
    }

    /// One sample for the weights of the 96 block words (84 oscillators, 12 sine buckets) and the
    /// vibrato depth (pitch fraction, `DM(0xE20E3)`).
    pub fn render(&mut self, weights: &[f32; ROWS], depth: f32) -> f32 {
        let two_m = self.bank.step(depth, self.pitch, &mut self.block);
        self.sines.step(two_m, &mut self.buckets);
        let (notes, buckets) = weights.split_at(NOTES);
        let dot = |w: &[f32], y: &[f32]| w.iter().zip(y).map(|(a, b)| a * b).sum::<f32>();
        self.active = weights.iter().any(|&w| w != 0.0);
        OUTPUT_LEVEL
            * self
                .output
                .process(dot(notes, &self.block) + dot(buckets, &self.buckets))
    }

    /// `true` when the last weights were all zero and the output sections have decayed.
    pub fn is_idle(&self) -> bool {
        !self.active && self.output.is_idle()
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

    fn run(src: &mut VoxSource, w: &[f32; ROWS], n: usize) -> Vec<f32> {
        (0..n).map(|_| src.render(w, 0.0)).collect()
    }

    #[test]
    fn silence_without_weights() {
        let mut s = VoxSource::new(48_000.0);
        let out = run(&mut s, &[0.0; ROWS], 5000);
        assert!(out.iter().all(|&v| v == 0.0));
        assert!(s.is_idle());
    }

    #[test]
    fn weights_act_instantly_and_release_to_exact_silence() {
        let mut s = VoxSource::new(48_000.0);
        let mut w = [0.0f32; ROWS];
        w[36] = 1.0;
        // a step on a free-running oscillator: the first sample after the step is already weighted
        let first = s.render(&w, 0.0);
        assert!(first != 0.0, "weights are not ramped");
        run(&mut s, &w, 100);
        assert!(!s.is_idle());
        let out = run(&mut s, &[0.0; ROWS], 300_000);
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(out[out.len() - 1000..].iter().all(|&v| v == 0.0));
        assert!(s.is_idle());
    }

    #[test]
    fn bucket_weights_sound_the_sine_bank() {
        let mut s = VoxSource::new(48_000.0);
        let mut w = [0.0f32; ROWS];
        w[NOTES] = 1.0; // bucket 0 only
        let out = run(&mut s, &w, 4000);
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 1e-4 && peak < 0.5, "{peak}"); // the 4 kHz sines pass the 1.1 kHz low-passes at -50 dB
    }

    #[test]
    fn oscillators_start_at_the_image_phases() {
        let s = VoxSource::new(48_000.0);
        let k = 40;
        let want = f64::from(super::super::vox_image::NOTE_START[k] as i32) / 2_147_483_648.0;
        assert!((f64::from(s.bank().signed_phase(k)) - want).abs() < 1e-6);
        assert!(s.bank().signed_phase(k) != 0.0);
    }

    #[test]
    fn fine_tune_scales_the_phase_advance() {
        let mut s = VoxSource::new(48_000.0);
        s.set_fine_cents(100.0);
        let advance = |src: &mut VoxSource| {
            let p0 = src.bank().signed_phase(40);
            src.render(&[0.0; ROWS], 0.0);
            let d = f64::from(src.bank().signed_phase(40)) - f64::from(p0);
            d.rem_euclid(2.0)
        };
        let a = advance(&mut s);
        let b = advance(&mut VoxSource::new(48_000.0));
        let ratio = a / b;
        assert!((ratio - 2.0f64.powf(1.0 / 12.0)).abs() < 1e-3, "{ratio}");
    }

    #[test]
    fn a_chord_has_its_three_fundamentals_at_twelve_tet_ratios() {
        let fs = 48_000.0f32;
        let mut s = VoxSource::new(fs);
        let mut t = [0.0f32; ROWS];
        for n in [60usize, 64, 67] {
            t[n - 24] = 1.0;
        }
        let out = run(&mut s, &t, 48_000 * 2);
        let tail = &out[48_000..];
        let base = 261.6256 * 2.0f64.powf(0.0);
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
        // the bank is on C1 at the firmware frame rate (docs/research/dsp-frame-rate.md)
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
