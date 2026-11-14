//! Key-down state of the B3 Bass processor `0x122296` (model 5): the weight windows, the output
//! gain accumulator and the measured line gain of the processor.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.9. Research: `docs/research/organ-farfisa-bass.md`
//! s11. Everything here is derived from the real ARM key-down (`tools/organ_bass_state.py`):
//!
//! * window 1 `DM 0x951F1 + k` (56 words, ring slot `k`) and window 2 `DM 0x95241 + k` (68 words,
//!   ring slot `12 + k`) are **additive**: the state is the sum of the per-key increments of the
//!   held keys, independent of the press order;
//! * the output gain word `DM 0xB44C9` (`G`) is a piecewise linear soft clip of the plain sum
//!   `S = sum inc0(key)` of the held keys: `G = phi(S)`, slope `2^-n` on `S` in `[n/2, (n+1)/2)`;
//! * the processor output is scaled by `1 - 0.84 G` and is otherwise linear in the ring, with a
//!   measured gain per line that depends on frequency only (`bass-proc-gain.csv`).
//!
//! The ring itself (`wheel * generator weight`) is **not** modelled here: the generator weights
//! (`DM 0x95122..`) shrink on shared wheels by a rule that is not decoded, so the stage takes the
//! ring line amplitude as an input and is not wired into the render path.

const STATE_CSV: &str = include_str!("../../../../docs/research/data/bass-state.csv");
const GAIN_CSV: &str = include_str!("../../../../docs/research/data/bass-proc-gain.csv");

/// First and last key of the decoded table.
pub(super) const FIRST_KEY: u8 = 24;
/// Last key of the decoded table.
pub(super) const LAST_KEY: u8 = 108;
const KEYS: usize = (LAST_KEY - FIRST_KEY) as usize + 1;
/// Words of weight window 1 and window 2.
pub(super) const W1_WORDS: usize = 56;
/// Words of weight window 2.
pub(super) const W2_WORDS: usize = 68;
/// Ring slot of window 2 word 0.
pub(super) const W2_RING_OFFSET: usize = 12;

/// Slope of the output scale `1 - K G`.
///
/// VERIFIED: sweep of `DM 0xB44C9` from 0 to 0.95 in the oracle (`tools/organ_bass_proc.py`), the
/// gain ratio is `1 - 0.84 G` to 0.01 at G = 0.2, 0.4, 0.6, 0.8, 0.95 and at three ring slots.
pub(super) const G_SLOPE: f32 = 0.84;

/// Per-key increments and the measured gain table, parsed once at construction.
#[derive(Debug, Clone)]
struct Tables {
    /// `delta[key - 24]`: increments of window 1 words then window 2 words.
    delta: Vec<[f32; W1_WORDS + W2_WORDS]>,
    /// `inc0[key - 24]`: unscaled step of `G`.
    inc0: [f32; KEYS],
    /// `(ln hz, dB)` of window 1 and window 2.
    gain: [Vec<(f32, f32)>; 2],
}

fn parse_tables() -> Tables {
    let mut t = Tables {
        delta: vec![[0.0; W1_WORDS + W2_WORDS]; KEYS],
        inc0: [0.0; KEYS],
        gain: [Vec::new(), Vec::new()],
    };
    for line in STATE_CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let key = f[0].parse::<usize>().unwrap_or(0) - usize::from(FIRST_KEY);
        let (win, idx) = (
            f[1].parse::<usize>().unwrap_or(0),
            f[2].parse::<usize>().unwrap_or(0),
        );
        let v = f32::from_bits(u32::from_str_radix(&f[3][2..], 16).unwrap_or(0));
        match win {
            0 => t.inc0[key] = v,
            1 => t.delta[key][idx] = v,
            _ => t.delta[key][W1_WORDS + idx] = v,
        }
    }
    for line in GAIN_CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let (win, hz, db) = (
            f[0].parse::<usize>().unwrap_or(1),
            f[1].parse::<f32>(),
            f[2].parse::<f32>(),
        );
        if let (Ok(hz), Ok(db)) = (hz, db) {
            t.gain[win - 1].push((hz.ln(), db));
        }
    }
    t
}

/// Weight windows, `G` and the held keys of the Bass processor.
#[derive(Debug, Clone)]
pub(super) struct BassState {
    tables: Tables,
    w1: [f32; W1_WORDS],
    w2: [f32; W2_WORDS],
    /// Plain sum `S` of the per-key steps.
    s: f32,
    /// Held keys in press order (`len` valid entries).
    order: [u8; 128],
    len: usize,
}

impl BassState {
    /// Silent state (no key held).
    pub(super) fn new() -> Self {
        Self {
            tables: parse_tables(),
            w1: [0.0; W1_WORDS],
            w2: [0.0; W2_WORDS],
            s: 0.0,
            order: [0; 128],
            len: 0,
        }
    }

    fn holds(&self, key: u8) -> bool {
        self.order[..self.len].contains(&key)
    }

    fn apply(&mut self, key: u8) {
        let Some(row) = usize::from(key)
            .checked_sub(usize::from(FIRST_KEY))
            .filter(|&i| i < KEYS)
        else {
            return;
        };
        let d = &self.tables.delta[row];
        for (w, x) in self.w1.iter_mut().zip(&d[..W1_WORDS]) {
            *w += x;
        }
        for (w, x) in self.w2.iter_mut().zip(&d[W1_WORDS..]) {
            *w += x;
        }
        self.s += self.tables.inc0[row];
    }

    fn rebuild(&mut self) {
        let keys = self.order;
        let len = self.len;
        self.w1 = [0.0; W1_WORDS];
        self.w2 = [0.0; W2_WORDS];
        self.s = 0.0;
        for &k in &keys[..len] {
            self.apply(k);
        }
    }

    /// Key-down in press order. A key that is already held is ignored.
    pub(super) fn key_down(&mut self, key: u8) {
        if key > 127 || self.holds(key) {
            return;
        }
        self.order[self.len] = key;
        self.len += 1;
        self.apply(key);
    }

    /// Key-up: the state is replayed without the key.
    ///
    /// ASSUMED: the firmware key-up undoes the key-down (not captured); the weights are additive,
    /// for `G` the replay of the remaining keys in press order is used.
    pub(super) fn key_up(&mut self, key: u8) {
        let Some(pos) = self.order[..self.len].iter().position(|&k| k == key) else {
            return;
        };
        self.order.copy_within(pos + 1..self.len, pos);
        self.len -= 1;
        self.rebuild();
    }

    /// The accumulator `G` (`DM 0xB44C9`).
    ///
    /// VERIFIED: `G = phi(S)` with `phi(S) = 1 - 2^-n + (S - n/2) 2^-n`, `n = floor(2 S)`: the
    /// ARM steps `G` by `inc0` at full size below 0.5, half size in 0.5..0.75, a quarter in
    /// 0.75..0.875 and so on, and a step that crosses a breakpoint is split (key 43 of the
    /// ascending run, 0.0203 instead of 0.0256). Replays of 6 sequences: max error 5e-7.
    pub(super) fn g(&self) -> f32 {
        let n = (2.0 * self.s).floor().max(0.0);
        let p = 2.0_f32.powf(-n);
        1.0 - p + (self.s - 0.5 * n) * p
    }

    /// Output scale `1 - 0.84 G`.
    pub(super) fn output_scale(&self) -> f32 {
        1.0 - G_SLOPE * self.g()
    }

    /// `(window 1, window 2)` weight on ring slot `slot` (0 outside the windows).
    pub(super) fn weights_on_slot(&self, slot: usize) -> (f32, f32) {
        let a = self.w1.get(slot).copied().unwrap_or(0.0);
        let b = slot
            .checked_sub(W2_RING_OFFSET)
            .and_then(|i| self.w2.get(i))
            .copied()
            .unwrap_or(0.0);
        (a, b)
    }

    /// Processor gain of one window at `hz` in linear units (G = 0), log-frequency interpolation
    /// of the measured table, clamped at both ends.
    ///
    /// EMPIRICAL: `tools/organ_bass_proc.py measure` with `G` pinned to 0, 1/6 octave bins, bin
    /// spread 0.0 to 1.5 dB (the gain does not depend on the note, only on the line frequency).
    pub(super) fn window_gain(&self, window: usize, hz: f32) -> f32 {
        let t = &self.tables.gain[window.clamp(1, 2) - 1];
        let (Some(first), Some(last)) = (t.first(), t.last()) else {
            return 0.0;
        };
        let x = hz.max(1.0).ln();
        let db = if x <= first.0 {
            first.1
        } else if x >= last.0 {
            last.1
        } else {
            let i = t.partition_point(|p| p.0 < x);
            let (a, b) = (t[i - 1], t[i]);
            a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
        };
        10.0_f32.powf(db / 20.0)
    }

    /// Output line amplitude of ring slot `slot` carrying a line of `ring_amp` at `hz`.
    ///
    /// The two windows add coherently (VERIFIED against the oracle chords: coherent sum max error
    /// 1.45 dB, power sum 3.1 dB).
    pub(super) fn line_amplitude(&self, slot: usize, hz: f32, ring_amp: f32) -> f32 {
        let (a, b) = self.weights_on_slot(slot);
        ring_amp * (a * self.window_gain(1, hz) + b * self.window_gain(2, hz)) * self.output_scale()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(keys: &[u8]) -> BassState {
        let mut s = BassState::new();
        for &k in keys {
            s.key_down(k);
        }
        s
    }

    #[test]
    fn g_matches_the_oracle_words_of_the_replayed_sequences() {
        // DM 0xB44C9 after the ARM key-down of the sequence (tools/organ_bass_state.py capture).
        assert!((chord(&[60]).g() - 0.026_199).abs() < 1e-6);
        assert!((chord(&[64]).g() - 0.025_817).abs() < 1e-6);
        assert!((chord(&[67]).g() - 0.026_127).abs() < 1e-6);
        assert!((chord(&[60, 64, 67]).g() - 0.078_143).abs() < 1e-6);
        assert!((chord(&[67, 64, 60]).g() - 0.078_143).abs() < 1e-6);
        let all: Vec<u8> = (24..=108).collect();
        assert!((chord(&all).g() - 0.953_44).abs() < 1e-4);
    }

    #[test]
    fn window_words_are_additive_and_order_free() {
        let a = chord(&[60, 64, 67]);
        let b = chord(&[67, 60, 64]);
        for slot in 0..98 {
            assert_eq!(a.weights_on_slot(slot), b.weights_on_slot(slot));
        }
        let sum: f32 = (0..98).map(|s| a.weights_on_slot(s).1).sum();
        // key 60, 64, 67 window 2 sums 0.01523 + 0.01552 + 0.01456 (ARM words, see bass-state.csv)
        assert!(sum > 0.04, "{sum}");
    }

    #[test]
    fn key_up_returns_to_silence_and_replays_the_rest() {
        let mut s = chord(&[60, 64]);
        s.key_up(60);
        let only = chord(&[64]);
        assert_eq!(s.g(), only.g());
        assert_eq!(s.s, only.s);
        s.key_up(64);
        assert_eq!(s.g(), 0.0);
        assert_eq!(s.weights_on_slot(48), (0.0, 0.0));
    }

    #[test]
    fn output_scale_is_the_swept_oracle_law() {
        let mut s = BassState::new();
        s.s = 0.4;
        assert!((s.output_scale() - 0.664).abs() < 1e-6);
        // oracle sweep: G 0.4 is 3.55 dB below G 0
        assert!((20.0 * s.output_scale().log10() + 3.55).abs() < 0.02);
        // phi is continuous and has slope 1/2 above S = 0.5
        s.s = 0.5;
        assert!((s.g() - 0.5).abs() < 1e-6);
        s.s = 1.0;
        assert!((s.g() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn window_gain_interpolates_the_measured_table() {
        let s = BassState::new();
        // table rows `1,259.61,-5.94` and `2,259.61,-2.08`
        assert!((20.0 * s.window_gain(1, 259.61).log10() + 5.94).abs() < 0.01);
        assert!((20.0 * s.window_gain(2, 259.61).log10() + 2.08).abs() < 0.01);
        assert!(s.window_gain(2, 1.0) > s.window_gain(2, 20_000.0));
    }

    #[test]
    fn chord_goldens_match_the_oracle_lines_within_1p5_db() {
        let csv = include_str!("../../../../docs/research/data/bass-chord-goldens.csv");
        let mut worst = 0.0_f32;
        let mut seen = 0;
        for line in csv.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            let keys: Vec<u8> = f[1].split(' ').filter_map(|k| k.parse().ok()).collect();
            let (slot, hz) = (f[2].parse::<usize>().unwrap(), f[3].parse::<f32>().unwrap());
            let (ring, out, g) = (
                f[4].parse::<f32>().unwrap(),
                f[5].parse::<f32>().unwrap(),
                f[6].parse::<f32>().unwrap(),
            );
            let s = chord(&keys);
            assert!((s.g() - g).abs() < 2e-5, "{} G {} vs {g}", f[0], s.g());
            let err = 20.0 * (s.line_amplitude(slot, hz, ring) / out).log10();
            worst = worst.max(err.abs());
            seen += 1;
        }
        assert!(seen >= 40, "{seen}");
        assert!(worst < 1.5, "worst line error {worst} dB");
    }
}
