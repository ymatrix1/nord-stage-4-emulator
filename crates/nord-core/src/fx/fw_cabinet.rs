//! Firmware amp cabinet (routine `0x122A1D`, module `0xC0442A94`) as a measured impulse response.
//!
//! The routine (a 130 tap FIR over the stereo pair stream followed by a bank of biquad sections
//! with state in PM, scaled by the `0.0078125` / `128` pair around it) is **exactly linear, time
//! invariant and channel independent** in the `sharc-fx` oracle with the real context: shifted and
//! scaled impulses give word identical results, the cross channel is zero (`tools/fx_amp_cabinet.py`
//! checks it while measuring). Its whole behaviour is therefore the impulse response in
//! [`super::fw_cabinet_taps`]; this stage convolves with the first 400 samples (99.9999 % of the
//! energy). No latency (the output of frame `n` uses inputs up to `n`).
//!
//! VERIFIED: the taps are oracle words; `tests_cabinet_golden.rs` (drive stage then cabinet on a
//! broadband seeded signal) bounds the truncation error. The structure of the routine (which
//! sections are IIR) is not transliterated; the second routine `0x122B1D` (a variant with an extra
//! mix path) is not modelled. The response was measured with the organ unit's context (PNO image,
//! `I1 = 0xE376A`); the same cabinet in the piano and synth units is ASSUMED.

use super::fw_cabinet_taps::{TAPS_1, TAPS_3, TAPS_5};

/// Taps kept per cabinet.
pub(super) const LEN: usize = TAPS_1.len();

/// A table of float words as floats (compile time).
const fn floats(words: &[u32; LEN]) -> [f32; LEN] {
    let mut out = [0.0; LEN];
    let mut i = 0;
    while i < LEN {
        out[i] = f32::from_bits(words[i]);
        i += 1;
    }
    out
}

static TWIN: [f32; LEN] = floats(&TAPS_1);
static JC: [f32; LEN] = floats(&TAPS_3);
static SMALL: [f32; LEN] = floats(&TAPS_5);

/// The cabinet of amp type `kind` (`kAmpTypeNew` 1..=6; pairs share a cabinet).
pub(super) fn taps_for(kind: u8) -> Option<&'static [f32; LEN]> {
    match kind {
        1 | 2 => Some(&TWIN),
        3 | 4 => Some(&JC),
        5 | 6 => Some(&SMALL),
        _ => None,
    }
}

/// Capacity of the linear history of one channel.
const BUF: usize = 2 * LEN;

/// Samples kept when the buffer is full and its newest part moves to the start: the `LEN - 1`
/// before a sample plus three more, so that a group of four frames never loses its history.
const KEEP: usize = LEN + 3;

/// Start value of `Iterator::sum` for floats (the sums below keep that order and start).
const SUM_START: f32 = -0.0;

/// Stereo FIR; allocation free and `Copy`.
///
/// Each output sample is `a + b`: `a` the sequential sum over the taps `k < split` and `b` the
/// sequential sum over the taps `k >= split`, with `split = LEN - slot` and `slot` the position of
/// the newest sample in a 400-slot circular history that moves down by one slot per frame. The
/// split is kept exactly as that circular buffer defines it (`pos` below), so the results are
/// bit-identical to summing the two slices of the ring one after the other, but the samples
/// themselves live in a linear, time ordered buffer so that a window of taps is contiguous.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwCabinet {
    taps: &'static [f32; LEN],
    /// Linear history per channel, oldest first; `hist[c][w - 1]` is the newest sample.
    hist: [[f32; BUF]; 2],
    /// Next write index; at least `LEN - 1` (the silent history before the first sample).
    w: usize,
    /// Ring slot of the newest sample of the equivalent circular history (`0..LEN`).
    pos: usize,
}

impl FwCabinet {
    /// Cabinet of amp type `kind`, or `None` for a type without one.
    pub(super) fn new(kind: u8) -> Option<Self> {
        Some(Self {
            taps: taps_for(kind)?,
            hist: [[0.0; BUF]; 2],
            w: LEN - 1,
            pos: 0,
        })
    }

    /// Stores `frame` as the newest sample and moves the ring slot down by one.
    #[inline]
    fn push(&mut self, frame: [f32; 2]) {
        if self.w == BUF {
            for h in &mut self.hist {
                h.copy_within(BUF - KEEP.., 0);
            }
            self.w = KEEP;
        }
        for (h, x) in self.hist.iter_mut().zip(frame) {
            h[self.w] = x;
        }
        self.w += 1;
        self.pos = if self.pos == 0 { LEN - 1 } else { self.pos - 1 };
    }

    /// One stereo frame through the cabinet.
    ///
    /// The four chains (two channels, `a` and `b`) are independent, so they run in lockstep: the
    /// float additions of every chain stay in the same order (bit-identical result) while the CPU
    /// overlaps their 4-cycle add latencies.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        self.push(frame);
        let split = LEN - self.pos;
        let newest = self.w - 1;
        let [h0, h1] = &self.hist;
        let t = self.taps;
        let mut acc = [SUM_START; 4];
        let b_len = LEN - split;
        let common = split.min(b_len);
        for k in 0..common {
            let kb = split + k;
            acc[0] += h0[newest - k] * t[k];
            acc[1] += h1[newest - k] * t[k];
            acc[2] += h0[newest - kb] * t[kb];
            acc[3] += h1[newest - kb] * t[kb];
        }
        for k in common..split {
            acc[0] += h0[newest - k] * t[k];
            acc[1] += h1[newest - k] * t[k];
        }
        for k in common..b_len {
            let kb = split + k;
            acc[2] += h0[newest - kb] * t[kb];
            acc[3] += h1[newest - kb] * t[kb];
        }
        [acc[0] + acc[2], acc[1] + acc[3]]
    }

    /// Four consecutive stereo frames through the cabinet (`frames[0]` first), the same result and
    /// the same state as four calls of [`Self::process_frame`].
    ///
    /// The four frames are independent sums over the same taps, so they run as the four lanes of
    /// one pass over the taps (about three times fewer dependent additions than four sequential
    /// frames). Frame `j` splits its sum at tap `s0 + j` (the ring slot moves by one per frame), so
    /// only the four taps from `s0` on need per-lane care. When the ring wraps inside the group (one
    /// group in a hundred) the frames go through [`Self::process_frame`] instead.
    #[inline]
    pub(super) fn process_block4(&mut self, frames: [[f32; 2]; 4]) -> [[f32; 2]; 4] {
        if self.pos < 4 {
            return frames.map(|f| self.process_frame(f));
        }
        let split0 = LEN - self.pos + 1; // split of frame 0 once its slot (`pos - 1`) is written
        for f in frames {
            self.push(f);
        }
        let first = self.w - 4; // index of frame 0; lane `j` reads `hist[first + j - k]` at tap `k`
        let t = self.taps;
        let high = (split0 + 4).min(LEN);
        // sums[chain a / b][channel][lane]
        let mut sums = [[[SUM_START; 4]; 2]; 2];
        let [h0, h1] = &self.hist;
        // The two channels advance together so that their dependent add chains overlap.
        let lanes = |k: usize| -> [&[f32; 4]; 2] {
            [
                h0[first - k..first - k + 4].try_into().unwrap(),
                h1[first - k..first - k + 4].try_into().unwrap(),
            ]
        };
        let [chain_a, chain_b] = &mut sums;
        // taps below every split: all lanes extend their `a` chain
        for (k, &tk) in t.iter().enumerate().take(split0) {
            let x = lanes(k);
            for j in 0..4 {
                chain_a[0][j] += x[0][j] * tk;
                chain_a[1][j] += x[1][j] * tk;
            }
        }
        // the taps around the splits: each lane switches from `a` to `b` at its own tap
        for (k, &tk) in t.iter().enumerate().take(high).skip(split0) {
            let x = lanes(k);
            for j in 0..4 {
                for ch in 0..2 {
                    let p = x[ch][j] * tk;
                    if k < split0 + j {
                        chain_a[ch][j] += p;
                    } else {
                        chain_b[ch][j] += p;
                    }
                }
            }
        }
        // taps above every split: all lanes extend their `b` chain
        for (k, &tk) in t.iter().enumerate().skip(high) {
            let x = lanes(k);
            for j in 0..4 {
                chain_b[0][j] += x[0][j] * tk;
                chain_b[1][j] += x[1][j] * tk;
            }
        }
        std::array::from_fn(|j| std::array::from_fn(|ch| sums[0][ch][j] + sums[1][ch][j]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_impulse_reproduces_the_taps_and_the_tail_ends() {
        for kind in [1u8, 3, 5] {
            let mut c = FwCabinet::new(kind).unwrap();
            let taps = taps_for(kind).unwrap();
            for (n, want) in taps.iter().copied().chain([0.0; 10]).enumerate() {
                let x = if n == 0 { 1.0 } else { 0.0 };
                let y = c.process_frame([x, -x]);
                assert!(
                    (y[0] - want).abs() < 1e-7 && (y[1] + want).abs() < 1e-7,
                    "{kind} {n}"
                );
            }
        }
    }

    /// The straightforward formulation: a circular history and one `Iterator::sum` per half and
    /// channel (the implementation before the lockstep and block versions).
    struct Reference {
        taps: &'static [f32; LEN],
        hist: [[f32; LEN]; 2],
        pos: usize,
    }

    impl Reference {
        fn new(kind: u8) -> Self {
            Self {
                taps: taps_for(kind).unwrap(),
                hist: [[0.0; LEN]; 2],
                pos: 0,
            }
        }

        fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
            self.pos = if self.pos == 0 { LEN - 1 } else { self.pos - 1 };
            let mut out = [0.0; 2];
            for (ch, y) in out.iter_mut().enumerate() {
                let h = &mut self.hist[ch];
                h[self.pos] = frame[ch];
                let (newer, older) = h.split_at(self.pos);
                let (t_head, t_tail) = self.taps.split_at(older.len());
                let a: f32 = older.iter().zip(t_head).map(|(x, t)| x * t).sum();
                let b: f32 = newer.iter().zip(t_tail).map(|(x, t)| x * t).sum();
                *y = a + b;
            }
            out
        }
    }

    fn bits(f: [f32; 2]) -> [u32; 2] {
        [f[0].to_bits(), f[1].to_bits()]
    }

    /// Silence first (signed zeros, an all-zero history), then noise, then silence again.
    fn test_input(n: usize, state: &mut u32) -> [f32; 2] {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let x = if !(450..1900).contains(&n) {
            0.0
        } else {
            (*state >> 8) as f32 / 16_777_216.0 - 0.5
        };
        [x, -x * 0.5]
    }

    #[test]
    fn lockstep_frames_equal_the_sequential_iterator_sums_bit_for_bit() {
        for kind in [1u8, 3, 5] {
            let mut fast = FwCabinet::new(kind).unwrap();
            let mut slow = Reference::new(kind);
            let mut state = 0x1234_5678u32;
            for n in 0..2600 {
                let f = test_input(n, &mut state);
                assert_eq!(
                    bits(fast.process_frame(f)),
                    bits(slow.process_frame(f)),
                    "kind {kind} frame {n}"
                );
            }
        }
    }

    #[test]
    fn four_frame_blocks_equal_four_single_frames_bit_for_bit() {
        for kind in [1u8, 3, 5] {
            let mut block = FwCabinet::new(kind).unwrap();
            let mut slow = Reference::new(kind);
            let mut state = 0x9E37_79B9u32;
            // 700 blocks cover every ring phase (the 400-slot ring wraps every 100 blocks)
            for g in 0..700 {
                let frames: [[f32; 2]; 4] =
                    std::array::from_fn(|j| test_input(g * 4 + j, &mut state));
                let got = block.process_block4(frames);
                for (j, f) in frames.iter().enumerate() {
                    assert_eq!(
                        bits(got[j]),
                        bits(slow.process_frame(*f)),
                        "kind {kind} block {g} frame {j}"
                    );
                }
            }
        }
    }

    #[test]
    fn blocks_and_single_frames_can_be_mixed_on_one_cabinet() {
        let mut mixed = FwCabinet::new(1).unwrap();
        let mut slow = Reference::new(1);
        let mut state = 7u32;
        let mut n = 0;
        for g in 0..300 {
            if g % 3 == 0 {
                let f = test_input(n, &mut state);
                n += 1;
                assert_eq!(bits(mixed.process_frame(f)), bits(slow.process_frame(f)));
            } else {
                let frames: [[f32; 2]; 4] = std::array::from_fn(|j| test_input(n + j, &mut state));
                n += 4;
                let got = mixed.process_block4(frames);
                for (j, f) in frames.iter().enumerate() {
                    assert_eq!(bits(got[j]), bits(slow.process_frame(*f)), "group {g}");
                }
            }
        }
    }

    #[test]
    fn pairs_share_a_cabinet_and_other_types_have_none() {
        assert!(std::ptr::eq(taps_for(1).unwrap(), taps_for(2).unwrap()));
        assert!(std::ptr::eq(taps_for(5).unwrap(), taps_for(6).unwrap()));
        assert!(!std::ptr::eq(taps_for(1).unwrap(), taps_for(3).unwrap()));
        assert!(taps_for(0).is_none() && taps_for(7).is_none() && FwCabinet::new(9).is_none());
    }

    #[test]
    fn the_cabinets_are_lossy_and_have_the_measured_energy() {
        // Impulse response energies from the oracle: 0.0736, 0.0396, 0.1248.
        for (kind, want) in [(1u8, 0.0736f32), (3, 0.0396), (5, 0.1248)] {
            let e: f32 = taps_for(kind).unwrap().iter().map(|t| t * t).sum();
            assert!((e - want).abs() < 2e-4, "{kind}: {e}");
        }
    }

    /// FNV-1a over the words of a tap table.
    fn fingerprint(words: &[u32]) -> u64 {
        words.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, w| {
            (h ^ u64::from(*w)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// Gain in dB of a cabinet at `freq` Hz (single bin DFT of the 400 taps, 48 kHz).
    fn gain_db(taps: &[f32; LEN], freq: f64) -> f64 {
        let w = std::f64::consts::TAU * freq / 48_000.0;
        let (re, im) = taps.iter().enumerate().fold((0.0, 0.0), |(r, i), (n, t)| {
            (
                r + f64::from(*t) * (w * n as f64).cos(),
                i - f64::from(*t) * (w * n as f64).sin(),
            )
        });
        20.0 * (re * re + im * im).sqrt().log10()
    }

    #[test]
    fn tap_tables_match_their_fingerprints_and_responses() {
        // Derived data, regenerated by `tools/fx_amp_cabinet.py taps`: a changed table must change
        // these words deliberately (VERIFIED against the oracle in fx-type-map s10).
        assert_eq!(fingerprint(&TAPS_1), FP_TWIN);
        assert_eq!(fingerprint(&TAPS_3), FP_JC);
        assert_eq!(fingerprint(&TAPS_5), FP_SMALL);
        // Responses of the oracle impulse responses at 1 / 3 / 6 kHz (this port reads the same
        // words, so the test pins the table, not the hardware). The Twin cabinet peaks near 3 kHz
        // (+7 dB over 1 kHz, the presence lift of the amp); JC is flat, Small is bright.
        for (kind, want) in [
            (1u8, [-9.441, -2.191, -11.232]),
            (3, [-10.391, -12.642, -10.791]),
            (5, [-5.059, -1.442, -1.713]),
        ] {
            for (f, w) in [1000.0, 3000.0, 6000.0].into_iter().zip(want) {
                let g = gain_db(taps_for(kind).unwrap(), f);
                assert!((g - w).abs() < 0.01, "type {kind} {f} Hz: {g} dB, want {w}");
            }
        }
    }

    const FP_TWIN: u64 = 0xa8e2_79c4_dcb0_ff2f;
    const FP_JC: u64 = 0x5915_8bd4_f6f6_d27a;
    const FP_SMALL: u64 = 0x30d1_064b_2881_7c25;
}
