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

/// Stereo FIR with a circular history; allocation free and `Copy`.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwCabinet {
    taps: &'static [f32; LEN],
    hist: [[f32; LEN]; 2],
    pos: usize,
}

impl FwCabinet {
    /// Cabinet of amp type `kind`, or `None` for a type without one.
    pub(super) fn new(kind: u8) -> Option<Self> {
        Some(Self {
            taps: taps_for(kind)?,
            hist: [[0.0; LEN]; 2],
            pos: 0,
        })
    }

    /// One stereo frame through the cabinet.
    ///
    /// Each output is `a + b` with `a` the sequential sum over the older part of the circular
    /// history and `b` over the newer part. The four chains (two channels, `a` and `b`) are
    /// independent, so they run in lockstep: the float additions of every chain stay in the same
    /// order (bit-identical result) while the CPU overlaps their 4-cycle add latencies.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        // `hist[c][pos]` is the newest sample; older ones follow at increasing (wrapping) index.
        self.pos = if self.pos == 0 { LEN - 1 } else { self.pos - 1 };
        let pos = self.pos;
        self.hist[0][pos] = frame[0];
        self.hist[1][pos] = frame[1];
        let [h0, h1] = &self.hist;
        let (n0, o0) = h0.split_at(pos);
        let (n1, o1) = h1.split_at(pos);
        let (t_head, t_tail) = self.taps.split_at(o0.len());
        // `-0.0` is the start value of `Iterator::sum` for floats.
        let mut acc = [-0.0f32; 4];
        let common = o0.len().min(n0.len());
        for i in 0..common {
            acc[0] += o0[i] * t_head[i];
            acc[1] += o1[i] * t_head[i];
            acc[2] += n0[i] * t_tail[i];
            acc[3] += n1[i] * t_tail[i];
        }
        for i in common..o0.len() {
            acc[0] += o0[i] * t_head[i];
            acc[1] += o1[i] * t_head[i];
        }
        for i in common..n0.len() {
            acc[2] += n0[i] * t_tail[i];
            acc[3] += n1[i] * t_tail[i];
        }
        [acc[0] + acc[2], acc[1] + acc[3]]
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

    #[test]
    fn lockstep_sums_equal_the_sequential_iterator_sums_bit_for_bit() {
        // The straightforward formulation: one `Iterator::sum` per half and channel.
        fn reference(c: &mut FwCabinet, frame: [f32; 2]) -> [f32; 2] {
            c.pos = if c.pos == 0 { LEN - 1 } else { c.pos - 1 };
            let mut out = [0.0; 2];
            for (ch, y) in out.iter_mut().enumerate() {
                let h = &mut c.hist[ch];
                h[c.pos] = frame[ch];
                let (newer, older) = h.split_at(c.pos);
                let (t_head, t_tail) = c.taps.split_at(older.len());
                let a: f32 = older.iter().zip(t_head).map(|(x, t)| x * t).sum();
                let b: f32 = newer.iter().zip(t_tail).map(|(x, t)| x * t).sum();
                *y = a + b;
            }
            out
        }
        let mut fast = FwCabinet::new(1).unwrap();
        let mut slow = FwCabinet::new(1).unwrap();
        let mut state = 0x1234_5678u32;
        for n in 0..1500 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            // silence first (signed zeros), then noise
            let x = if n < 450 {
                0.0
            } else {
                (state >> 8) as f32 / 16_777_216.0 - 0.5
            };
            let f = [x, -x * 0.5];
            let (a, b) = (fast.process_frame(f), reference(&mut slow, f));
            assert_eq!(
                [a[0].to_bits(), a[1].to_bits()],
                [b[0].to_bits(), b[1].to_bits()],
                "frame {n}"
            );
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
