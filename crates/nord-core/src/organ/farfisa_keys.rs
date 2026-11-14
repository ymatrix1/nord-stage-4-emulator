//! Farfisa key stage: the processor `0x122B85` as key-down weights and the measured line gain.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.8. Research: `docs/research/organ-farfisa-bass.md`
//! s7 / s8. Opt-in (`OrganLayer::farfisa_processor`, `--farfisa-processor on`), only on the
//! `Farfisa` model with `ToneWheelSource::FarfisaWheels`.
//!
//! What the firmware does per key `n` (VERIFIED, `keydown-organ-models.csv`, notes 36, 60, 84): the
//! key-down writes the weight `1/48` at `DM 0x95FE4 + n` (sum 0, index `n - 24`) and at
//! `+ 0x62` (sum 1, index `n - 20`) and `+ 0xD0` (sum 2). With the installed gains sums 2 and 3 give
//! exactly 0.0 output, sum 0 reads ring slot `idx` (note `n - 12`, the 16' row) and sum 1 slot
//! `idx + 8` (note `n`, the 8' row) of the generator ring ([`super::farfisa_wheels`], slot `s` =
//! note `12 + s`). The processor is linear and time invariant in the ring and the weights
//! (0.00 dB, `farfisa_proc_gain.rs`), so the stage is
//!
//! `y = sum_keys g_key / 48 * ( G0 * wheel(n - 12) + G1 * wheel(n) )`
//!
//! with `G0`, `G1` the measured per-sum line gains. Tags: weight `1/48` VERIFIED; gain tables
//! EMPIRICAL (magnitude only, no phase); the linear-phase FIR realisation of the gain, the 2 ms key
//! ramp (as `b3_weights.rs`) and the host-rate re-use of the tables (they are in Hz at the DSP
//! frame rate anchor, pitch is kept at any rate) ASSUMED. Not modelled: the bias words, the Farfisa
//! tab selection (key-down writes the same three weights for every tab: the registers are not in
//! this path as far as captured), the vibrato.

use super::farfisa_proc_gain::farfisa_proc_gain;
use super::gain_fir;
use super::types::NUM_TONEWHEELS;
use super::{OrganLayer, OrganModel, ToneWheelSource};

/// VERIFIED (`keydown-organ-models.csv`, model 2): the key-down weight of sums 0 and 1.
pub(super) const KEY_WEIGHT: f32 = 1.0 / 48.0;
/// Taps of each gain FIR (odd, linear phase). ASSUMED: 255 resolves the 1/6 octave table down to
/// about 190 Hz at 48 kHz, the tables are smooth (a few dB per octave) below that.
pub(super) const TAPS: usize = 255;
/// Wheel 0 of the bus is note 24 (`farfisa_wheels.rs`).
const FIRST_NOTE: i32 = 24;
/// ASSUMED: key weight ramp time constant in seconds (the B3 weight ramp, `b3_weights.rs`).
const RAMP_TAU: f32 = 0.002;

/// State of the key stage: smoothed key weights and the two gain filters with their histories.
#[derive(Debug, Clone)]
pub(super) struct FarfisaKeys {
    fir: [Vec<f32>; 2],
    hist: [Vec<f32>; 2],
    pos: usize,
    gain: [f32; 128],
    coef: f32,
    designed_for: f32,
    ring_down: usize,
}

/// Wheel index of `note`, `None` outside the divider's bus (notes 24..=114).
fn wheel_of(note: i32) -> Option<usize> {
    let w = note - FIRST_NOTE;
    (0..NUM_TONEWHEELS as i32)
        .contains(&w)
        .then_some(w as usize)
}

impl FarfisaKeys {
    /// All keys off; the filters are designed on first use (`designed_for` 0). Allocates here only.
    pub(super) fn new() -> Self {
        Self {
            fir: [vec![0.0; TAPS], vec![0.0; TAPS]],
            hist: [vec![0.0; 2 * TAPS], vec![0.0; 2 * TAPS]],
            pos: 0,
            gain: [0.0; 128],
            coef: 1.0,
            designed_for: 0.0,
            ring_down: 0,
        }
    }

    /// Redesigns both FIRs in place (frequency sampling of the measured magnitude, Hann window).
    fn design(&mut self, sample_rate: f32) {
        for (sum, fir) in self.fir.iter_mut().enumerate() {
            gain_fir::design(fir, sample_rate, |hz| farfisa_proc_gain(sum, hz));
        }
        self.coef = 1.0 - (-1.0 / (RAMP_TAU * sample_rate)).exp();
        self.designed_for = sample_rate;
    }

    /// `true` while a key weight or the filters still carry signal.
    pub(super) fn active(&self) -> bool {
        self.ring_down > 0
    }

    /// One output sample (before the level law) from the 91 bus wheels and the held keys.
    pub(super) fn process(
        &mut self,
        held: &[bool; 128],
        key_gain: &[f32; 128],
        octave_semis: i32,
        wheels: &[f32; NUM_TONEWHEELS],
        sample_rate: f32,
    ) -> f32 {
        if (sample_rate - self.designed_for).abs() > 0.5 {
            self.design(sample_rate);
        }
        let mut x = [0.0f32; 2];
        let mut any = false;
        for note in 0..128usize {
            let target = if held[note] { key_gain[note] } else { 0.0 };
            let g = &mut self.gain[note];
            *g += (target - *g) * self.coef;
            if target == 0.0 && *g < 1e-5 {
                *g = 0.0;
                continue;
            }
            any = true;
            let key = note as i32 + octave_semis;
            if let Some(w) = wheel_of(key - 12) {
                x[0] += *g * wheels[w];
            }
            if let Some(w) = wheel_of(key) {
                x[1] += *g * wheels[w];
            }
        }
        if any {
            self.ring_down = TAPS;
        } else if self.ring_down > 0 {
            self.ring_down -= 1;
        }
        let mut y = 0.0;
        let pos = self.pos;
        for ((fir, hist), xi) in self.fir.iter().zip(self.hist.iter_mut()).zip(x) {
            hist[pos] = xi;
            hist[pos + TAPS] = xi;
            let window = &hist[pos + 1..pos + 1 + TAPS];
            y += fir.iter().zip(window).map(|(h, v)| h * v).sum::<f32>();
        }
        self.pos = (self.pos + 1) % TAPS;
        KEY_WEIGHT * y
    }
}

impl OrganLayer {
    /// `true` when the Farfisa processor stage replaces the drawbar sum of this layer.
    pub(super) fn farfisa_keys_active(&self) -> bool {
        self.farfisa_processor
            && self.model == OrganModel::Farfisa
            && self.effective_tone_source() == ToneWheelSource::FarfisaWheels
    }

    /// Output of the key stage for the current wheel bus (level law of the firmware paths, `L`).
    pub(super) fn sum_farfisa_keys(&mut self, wheels: &[f32; NUM_TONEWHEELS]) -> f32 {
        let oct = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        let level = self.weight_level * self.level_law().bus_gain();
        level
            * self.farfisa_keys.process(
                &self.held_notes,
                &self.note_gains,
                oct,
                wheels,
                self.sample_rate,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organ::farfisa_wheels::FarfisaWheels;

    const FS: f32 = 48_000.0;

    /// Gain of FIR `sum` at `hz` by direct evaluation of its frequency response.
    fn response(k: &FarfisaKeys, sum: usize, hz: f32) -> f32 {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (n, h) in k.fir[sum].iter().enumerate() {
            let a = std::f32::consts::TAU * hz / FS * n as f32;
            re += h * a.cos();
            im -= h * a.sin();
        }
        re.hypot(im)
    }

    #[test]
    fn fir_follows_the_measured_gain_table() {
        let mut k = FarfisaKeys::new();
        k.design(FS);
        for sum in 0..2 {
            for hz in [300.0f32, 600.0, 900.0, 1500.0, 3000.0] {
                let want = 20.0 * farfisa_proc_gain(sum, hz).log10();
                let got = 20.0 * response(&k, sum, hz).log10();
                assert!(
                    (got - want).abs() < 1.0,
                    "sum {sum} {hz} Hz: {got} vs {want}"
                );
            }
        }
    }

    #[test]
    fn wheel_map_is_note_minus_24() {
        assert_eq!(wheel_of(24), Some(0));
        assert_eq!(wheel_of(114), Some(90));
        assert_eq!(wheel_of(23), None);
        assert_eq!(wheel_of(115), None);
    }

    /// Goertzel magnitude of `x` at `hz` over a Hann window.
    fn line(x: &[f32], hz: f32) -> f32 {
        let n = x.len() as f32;
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n).cos();
            let a = std::f32::consts::TAU * hz / FS * i as f32;
            re += v * w * a.cos();
            im += v * w * a.sin();
        }
        2.0 * re.hypot(im) / n
    }

    fn chord(keys: &[usize], frames: usize) -> Vec<f32> {
        let mut gen = FarfisaWheels::new();
        gen.retune(FS, 0.0);
        let mut stage = FarfisaKeys::new();
        let mut held = [false; 128];
        for &k in keys {
            held[k] = true;
        }
        let gains = [1.0f32; 128];
        let mut wheels = [0.0f32; NUM_TONEWHEELS];
        let mut out = Vec::with_capacity(frames);
        for _ in 0..frames {
            gen.advance(&mut wheels);
            out.push(stage.process(&held, &gains, 0, &wheels, FS));
        }
        out
    }

    /// Oracle chord `a` (`organ-proc-oracle-lines.csv`, Farfisa 60 64 67): the four strongest
    /// lines are C4, G4, C3 and E3 within 1 dB of each other, peak 0.0867. The Rust stage has no
    /// phase model, so the check is on line levels (3 dB) and the peak (6 dB), not bit-exact.
    #[test]
    fn oracle_chord_a_line_levels() {
        let out = chord(&[60, 64, 67], 96_000);
        let tail = &out[48_000..];
        let hz = |note: f32| 440.0 * ((note - 69.0) / 12.0).exp2();
        let lines: Vec<f32> = [60.0, 67.0, 48.0, 52.0]
            .iter()
            .map(|&n| 20.0 * line(tail, hz(n)).log10())
            .collect();
        let top = lines.iter().cloned().fold(f32::MIN, f32::max);
        for (l, n) in lines.iter().zip([60, 67, 48, 52]) {
            assert!((l - top).abs() < 3.5, "note {n}: {l} dB, top {top}");
        }
        let peak = tail.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let ratio_db = 20.0 * (peak / 0.08666).log10();
        assert!(
            ratio_db.abs() < 6.0,
            "peak {peak} ({ratio_db} dB vs oracle)"
        );
    }

    #[test]
    fn released_keys_ring_down_to_silence() {
        let mut stage = FarfisaKeys::new();
        let held = [false; 128];
        let gains = [1.0f32; 128];
        let wheels = [0.5f32; NUM_TONEWHEELS];
        let mut held_on = held;
        held_on[60] = true;
        for _ in 0..2000 {
            stage.process(&held_on, &gains, 0, &wheels, FS);
        }
        assert!(stage.active());
        let mut last = 1.0;
        for _ in 0..(TAPS + 20_000) {
            last = stage.process(&held, &gains, 0, &wheels, FS);
        }
        assert!(!stage.active());
        assert_eq!(last, 0.0);
    }

    #[test]
    fn octave_shift_moves_the_key_to_other_wheels() {
        let mut stage = FarfisaKeys::new();
        let mut held = [false; 128];
        held[60] = true;
        let gains = [1.0f32; 128];
        let mut wheels = [0.0f32; NUM_TONEWHEELS];
        wheels[60 - 24] = 1.0; // the 8' row of key 60
        let mut base = 0.0f32;
        let mut shifted = 0.0f32;
        for _ in 0..4000 {
            base = base.max(stage.process(&held, &gains, 0, &wheels, FS).abs());
        }
        let mut stage = FarfisaKeys::new();
        for _ in 0..4000 {
            shifted = shifted.max(stage.process(&held, &gains, 24, &wheels, FS).abs());
        }
        assert!(base > 1e-3, "{base}");
        assert!(shifted < 0.05 * base, "{shifted} vs {base}");
    }

    fn render_layer(processor: bool, model: OrganModel) -> f32 {
        let mut o = OrganLayer::new(FS);
        o.model = model;
        o.tonewheel_source = ToneWheelSource::FarfisaWheels;
        o.farfisa_processor = processor;
        o.rotary_enabled = false;
        o.note_on(60);
        let mut peak = 0.0f32;
        for i in 0..24_000 {
            let [l, _] = o.render_frame();
            if i > 12_000 {
                peak = peak.max(l.abs());
            }
        }
        peak
    }

    #[test]
    fn wiring_is_opt_in_and_only_for_the_farfisa_model() {
        let off = render_layer(false, OrganModel::Farfisa);
        let on = render_layer(true, OrganModel::Farfisa);
        assert!(on > 1e-3 && off > 1e-3, "{on} {off}");
        assert!(
            (on - off).abs() > 1e-4,
            "the stage changes the Farfisa render"
        );
        // other models ignore the flag (their source resolves to something else)
        assert_eq!(
            render_layer(true, OrganModel::B3),
            render_layer(false, OrganModel::B3)
        );
        let mut o = OrganLayer::new(FS);
        assert!(!o.farfisa_processor && !o.farfisa_keys_active());
        o.model = OrganModel::Farfisa;
        o.farfisa_processor = true;
        assert!(!o.farfisa_keys_active(), "needs the FarfisaWheels source");
        o.tonewheel_source = ToneWheelSource::FarfisaWheels;
        assert!(o.farfisa_keys_active());
    }
}
