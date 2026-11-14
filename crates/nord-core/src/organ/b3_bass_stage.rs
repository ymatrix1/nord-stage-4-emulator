//! B3 Bass stage: the processor `0x122296` on the B3 wheel bus (opt-in).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.9. Research: `docs/research/organ-farfisa-bass.md`
//! s11 / s12. Opt-in (`OrganLayer::set_b3_bass_processor`, `--b3-bass-processor on`), only on the
//! `B3` model in Bass mode with `ToneWheelSource::B3Wheels` and no processor tail (the stage is the
//! tail of this model).
//!
//! `ring[s] = wheel(note s + 12) * gen_weight(note)` (`b3_bass_gen.rs`), the processor sums
//! `x1 = sum W1[k] ring[k]` and `x2 = sum W2[k] ring[12 + k]` (`b3_bass_state.rs`), filters both
//! with the measured line gain of their window (two linear-phase FIRs) and scales by
//! `1 - 0.84 G`. All three state words are functions of the held keys, so the stage folds them
//! into one coefficient per wheel and window, ramped over 2 ms.
//!
//! Tags: key-down state VERIFIED (research s11 / s12); gain tables EMPIRICAL (magnitude only);
//! the linear-phase FIR realisation, the key ramp and the 888888888 key tables (the real default
//! program writes smaller window steps) ASSUMED. Not modelled: drawbar and percussion settings
//! (the tables are the `888888888` key-down), the scanner inside the processor.

use super::b3_bass_gen::GenWeights;
use super::b3_bass_state::{BassState, W2_RING_OFFSET};
use super::gain_fir;
use super::types::NUM_TONEWHEELS;
use super::{OrganLayer, OrganModel, ToneWheelSource};

/// Taps of each gain FIR (odd). ASSUMED: 511 resolves the 1/6 octave table down to about 94 Hz at
/// 48 kHz; the Bass table starts at 74 Hz and is smooth below.
pub(super) const TAPS: usize = 511;
/// Wheel 0 of the bus is note 24 (`b3_wheels.rs`).
const WHEEL0_NOTE: usize = 24;
/// Ring slot of note `n` is `n - 12` (VERIFIED: slot 48 = 259.6 Hz = C4, research s11).
const RING_NOTE0: usize = 12;
/// ASSUMED: coefficient ramp time constant in seconds (as the Farfisa stage).
const RAMP_TAU: f32 = 0.002;
/// Keys `24..=108` carry key-down tables.
const MIN_KEY: i32 = 24;
const MAX_KEY: i32 = 108;

/// State of the stage: key tables, folded coefficients, the two gain filters with histories.
#[derive(Debug, Clone)]
pub(super) struct B3BassStage {
    state: BassState,
    gen: GenWeights,
    fir: [Vec<f32>; 2],
    hist: [Vec<f32>; 2],
    pos: usize,
    designed_for: f32,
    coef_ramp: f32,
    target: [[f32; NUM_TONEWHEELS]; 2],
    coef: [[f32; NUM_TONEWHEELS]; 2],
    /// Transposed key registered by `note_on` of each note (`-1` = none).
    keys: [i16; 128],
    ring_down: usize,
}

impl B3BassStage {
    /// Parses the derived tables and allocates the filters (setup time only).
    pub(super) fn new() -> Self {
        Self {
            state: BassState::new(),
            gen: GenWeights::new(),
            fir: [vec![0.0; TAPS], vec![0.0; TAPS]],
            hist: [vec![0.0; 2 * TAPS], vec![0.0; 2 * TAPS]],
            pos: 0,
            designed_for: 0.0,
            coef_ramp: 1.0,
            target: [[0.0; NUM_TONEWHEELS]; 2],
            coef: [[0.0; NUM_TONEWHEELS]; 2],
            keys: [-1; 128],
            ring_down: 0,
        }
    }

    fn design(&mut self, sample_rate: f32) {
        for (i, fir) in self.fir.iter_mut().enumerate() {
            let state = &self.state;
            gain_fir::design(fir, sample_rate, |hz| state.window_gain(i + 1, hz));
        }
        self.coef_ramp = 1.0 - (-1.0 / (RAMP_TAU * sample_rate)).exp();
        self.designed_for = sample_rate;
    }

    /// Re-folds the window words, the generator weights and `1 - 0.84 G` into one coefficient per
    /// wheel and window.
    fn refold(&mut self) {
        let scale = self.state.output_scale();
        let (w1, w2) = self.state.windows();
        for i in 0..NUM_TONEWHEELS {
            let note = WHEEL0_NOTE + i;
            let slot = note - RING_NOTE0;
            let g = self.gen.weight(note) * scale;
            self.target[0][i] = w1.get(slot).copied().unwrap_or(0.0) * g;
            self.target[1][i] = slot
                .checked_sub(W2_RING_OFFSET)
                .and_then(|k| w2.get(k))
                .copied()
                .unwrap_or(0.0)
                * g;
        }
    }

    /// Key-down of `note` at the transposed `key`.
    pub(super) fn note_on(&mut self, note: usize, key: i32) {
        if !(MIN_KEY..=MAX_KEY).contains(&key) || self.keys[note] >= 0 {
            return;
        }
        self.keys[note] = key as i16;
        self.state.key_down(key as u8);
        self.gen.key_down(key as usize);
        self.refold();
        self.ring_down = TAPS + 16 * (1.0 / self.coef_ramp.max(1e-4)) as usize;
    }

    /// Key-up of `note` (the key registered at its key-down).
    pub(super) fn note_off(&mut self, note: usize) {
        let key = self.keys[note];
        if key < 0 {
            return;
        }
        self.keys[note] = -1;
        self.state.key_up(key as u8);
        self.gen.key_up(key as usize);
        self.refold();
    }

    /// `true` while a key is held or the ramps and filters still carry signal.
    pub(super) fn active(&self) -> bool {
        self.ring_down > 0 || self.keys.iter().any(|&k| k >= 0)
    }

    /// One output sample (before the level law) from the 91 bus wheels.
    pub(super) fn process(&mut self, wheels: &[f32; NUM_TONEWHEELS], sample_rate: f32) -> f32 {
        if (sample_rate - self.designed_for).abs() > 0.5 {
            self.design(sample_rate);
        }
        let mut x = [0.0f32; 2];
        for (w, xw) in x.iter_mut().enumerate() {
            let (coef, target) = (&mut self.coef[w], &self.target[w]);
            let mut acc = 0.0;
            for ((c, t), v) in coef.iter_mut().zip(target).zip(wheels) {
                *c += (t - *c) * self.coef_ramp;
                acc += *c * v;
            }
            *xw = acc;
        }
        if self.keys.iter().any(|&k| k >= 0) {
            self.ring_down = TAPS + 16 * (1.0 / self.coef_ramp.max(1e-4)) as usize;
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
        y
    }
}

impl OrganLayer {
    /// Switches the Bass processor stage (setup time: allocates the stage). Turning it on selects
    /// the plain wheel tables (no B3 tail): the stage replaces the tail for this model.
    pub fn set_b3_bass_processor(&mut self, on: bool) {
        self.b3_bass_processor = on;
        if on {
            self.set_b3_tail(false);
            if self.b3_bass.is_none() {
                self.b3_bass = Some(Box::new(B3BassStage::new()));
            }
        }
    }

    /// Whether the Bass processor stage is switched on (`--b3-bass-processor on`).
    pub fn b3_bass_processor_on(&self) -> bool {
        self.b3_bass_processor
    }

    /// `true` when the Bass stage replaces the drawbar sum of this layer.
    pub(super) fn b3_bass_active(&self) -> bool {
        self.b3_bass_processor
            && self.b3_bass.is_some()
            && self.model == OrganModel::B3
            && self.b3_bass_mode
            && !self.b3_tail
            && self.effective_tone_source() == ToneWheelSource::B3Wheels
    }

    /// `true` while the stage still carries signal.
    pub(super) fn b3_bass_sounding(&self) -> bool {
        self.b3_bass_active() && self.b3_bass.as_ref().is_some_and(|b| b.active())
    }

    /// Key event hook of `note_on` / `note_off`.
    pub(super) fn b3_bass_key(&mut self, note: usize, down: bool) {
        let oct = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        if let Some(b) = self.b3_bass.as_mut() {
            if down {
                b.note_on(note, note as i32 + oct);
            } else {
                b.note_off(note);
            }
        }
    }

    /// Output of the Bass stage for the current wheel bus (level law of the B3 weight path).
    pub(super) fn sum_b3_bass(&mut self, wheels: &[f32; NUM_TONEWHEELS]) -> f32 {
        let level = super::b3_weights::DRY_GAIN * self.level_law().bus_gain();
        let fs = self.sample_rate;
        level * self.b3_bass.as_mut().map_or(0.0, |b| b.process(wheels, fs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn layer() -> OrganLayer {
        let mut o = OrganLayer::new(FS);
        o.model = OrganModel::B3;
        o.b3_bass_mode = true;
        o.tonewheel_source = ToneWheelSource::B3Wheels;
        o.set_b3_bass_processor(true);
        o.rotary_enabled = false;
        o.key_click_enabled = false;
        o.percussion_enabled = false;
        o
    }

    fn render(o: &mut OrganLayer, n: usize) -> Vec<f32> {
        (0..n).map(|_| o.render_frame()[0]).collect()
    }

    /// Hann Goertzel amplitude of `x` at `hz`.
    fn line(x: &[f32], hz: f32) -> f32 {
        let (mut re, mut im, mut win) = (0.0f64, 0.0f64, 0.0f64);
        for (n, v) in x.iter().enumerate() {
            let h = 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / x.len() as f64).cos();
            let a = std::f64::consts::TAU * f64::from(hz) / f64::from(FS) * n as f64;
            re += f64::from(*v) * h * a.cos();
            im += f64::from(*v) * h * a.sin();
            win += h;
        }
        (2.0 * re.hypot(im) / win) as f32
    }

    #[test]
    fn stage_is_off_by_default_and_needs_bass_mode_and_b3_wheels() {
        let o = OrganLayer::new(FS);
        assert!(!o.b3_bass_active());
        let mut o = layer();
        assert!(o.b3_bass_active());
        o.b3_bass_mode = false;
        assert!(!o.b3_bass_active());
        o.b3_bass_mode = true;
        o.set_b3_tail(true);
        assert!(!o.b3_bass_active());
    }

    #[test]
    fn chord_lines_follow_the_oracle_goldens() {
        // Oracle lines of the chord 60 64 67 (`bass-chord-goldens.csv`, rows `g3`: boot + install +
        // the ARM key-down, `sharc-chain` generator + processor, 8192 point Hann line amplitude of
        // the output stream). The stage runs on the plain B3 wheel tables, at the host rate and
        // with the project tuning (lines read at `b3_note_hz`); the oracle values carry a Hann
        // scalloping of up to 1.4 dB. Measured: +1.1 to +2.5 dB per line (research s12); the test
        // bounds it at 3.5 dB so a regression of the key state (6 to 15 dB per rule) fails.
        let csv = include_str!("../../../../docs/research/data/bass-chord-goldens.csv");
        let mut o = layer();
        for k in [60u8, 64, 67] {
            o.note_on(k);
        }
        let x = render(&mut o, 48_000);
        let seg = &x[8_192..8_192 + 16_384];
        let mut worst = 0.0f32;
        let mut seen = 0;
        for l in csv.lines().filter(|l| l.starts_with("g3,")) {
            let f: Vec<&str> = l.split(',').collect();
            let slot: usize = f[2].parse().unwrap();
            let want: f32 = f[5].parse().unwrap();
            let got = line(seg, super::super::b3_note_hz(slot + 12) as f32);
            let err = 20.0 * (got / want).log10();
            worst = worst.max(err.abs());
            seen += 1;
        }
        assert_eq!(seen, 6);
        assert!(worst < 3.5, "worst line error {worst} dB");
    }

    #[test]
    fn more_keys_pull_the_coefficient_of_a_wheel_down() {
        // generator weights shrink on shared wheels and G grows (ring amplitude of slot 48 0.262
        // after 3 keys against 0.061 after 85, research s11): the folded coefficient of wheel 60
        // (bus index 36, window 2) is far below its single key value with all 85 keys held.
        let mut single = B3BassStage::new();
        single.note_on(60, 60);
        let mut all = B3BassStage::new();
        for k in 24..=108 {
            all.note_on(k as usize, k);
        }
        let (one, many) = (single.target[1][36], all.target[1][36]);
        assert!(one > 0.0 && many > 0.0);
        assert!(many < 0.5 * one, "{many} vs {one}");
    }

    #[test]
    fn release_rings_down_to_silence_and_stays_finite() {
        let mut o = layer();
        o.note_on(48);
        o.note_on(55);
        let held = render(&mut o, 4_000);
        assert!(held.iter().all(|v| v.is_finite()));
        assert!(held.iter().any(|v| v.abs() > 1e-4));
        o.note_off(48);
        o.note_off(55);
        let tail = render(&mut o, 30_000);
        assert!(tail.iter().all(|v| v.is_finite()));
        assert!(tail[29_000..].iter().all(|v| v.abs() < 1e-7));
        assert!(!o.b3_bass_sounding());
    }

    #[test]
    fn octave_shift_registers_the_transposed_key_and_releases_it() {
        let mut o = layer();
        o.octave_shift = 1;
        o.note_on(60);
        o.octave_shift = 0;
        o.note_off(60);
        let x = render(&mut o, 30_000);
        assert!(x[29_000..].iter().all(|v| v.abs() < 1e-7));
        // a key outside the decoded range 24..=108 does nothing
        o.note_on(10);
        assert!(render(&mut o, 2_000).iter().all(|v| v.abs() < 1e-7));
    }
}
