//! Integer model of one SMP compressed-voice call (`0x1C07D8..0x1C0857`, ADSP-2148x).
//!
//! Spec: `docs/specs/piano-engine.md`; evidence and the field map:
//! `docs/research/smp-voice.md` (every field was poked on the oracle and observed).
//!
//! One call produces one stereo frame of one voice. Record fields are the signed word offsets from
//! `I3` (VERIFIED, `tools/ns4_smp_voice.py` and `smp_voice_ref`, 0 differences):
//!
//! | Offset | Field | Used as |
//! |---|---|---|
//! | -32 / -31 | step integer / fraction | `phase += frac`, `counter += int + carry` |
//! | -30 | `counter` | residual number `n` (and history phase) |
//! | -29 | `phase` | kernel phase = `phase >> 24` (value before the add) |
//! | -28 | stride | `bit = stride * counter - base` (stride = `channels * width`) |
//! | -19 | width | `FEXT` length (residual bits) |
//! | -27 | `base` | bit position offset (`-width` selects the right channel of an interleaved stream) |
//! | -26 / -25 | stream ring / history ring | `B1` (`L1 = 256`) / `B5` (`L5 = 16`) |
//! | -24 | history phase | `(hist_off + counter) & 7` selects the ring slot |
//! | -18 | bus pointer | `I7`, left at `+0`, right at `+1` (`LW` accumulate) |
//! | -17 | predictor pointer | `I9`, four coefficients, oldest history first |
//! | 0 / 1 | gain L / R | Q31 `SSF` multipliers |
//! | 4 | last interpolated value | store after the shift |

use super::q31_kernel::SmpKernel;

/// Words of the packed-stream ring (`L1 = 0x100`); each word holds 16 payload bits.
pub const RING_WORDS: usize = 256;
/// Words of the history ring (`L5 = 0x10`): 8 samples at stride 2.
pub const HIST_WORDS: usize = 16;
/// `r2 = LSHIFT r2 BY r15`, `R15 = 0x11` is set at `0x1C07C0`, just before the loop.
pub const OUT_SHIFT: u32 = 17;

/// `SSF` multiply without rounding: `(a * b * 2) >> 32`, i.e. the product in Q31, floored.
pub fn mul_ssf(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}

/// State and record fields of one voice.
#[derive(Debug, Clone)]
pub struct SmpVoice {
    pub step_int: u32,
    pub step_frac: u32,
    pub counter: u32,
    pub phase: u32,
    /// Record word `-28`: bits between consecutive residuals of this voice.
    pub stride: u32,
    /// Record word `-19`: residual bits.
    pub width: u32,
    pub base: u32,
    pub hist_off: u32,
    pub coefs: [i32; 4],
    pub gain: [i32; 2],
    pub ring: [u32; RING_WORDS],
    pub hist: [i32; HIST_WORDS],
    /// Record word `+4`.
    pub last: i32,
}

/// Intermediate values of one call (for comparison with the oracle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call {
    /// Counter and kernel phase index used by this call.
    pub counter: u32,
    pub phase_index: usize,
    /// Residual and decoded sample `y_counter`.
    pub residual: i32,
    pub decoded: i32,
    /// `R2` after `MRF (SSFR)` and after the shift (`+4` store).
    pub interp: i32,
    pub shifted: i32,
    /// Amounts added to the bus.
    pub add: [i32; 2],
}

impl SmpVoice {
    pub fn new(width: u32, coefs: [i32; 4], gain: [i32; 2]) -> Self {
        Self {
            step_int: 0,
            step_frac: 0,
            counter: 0,
            phase: 0,
            stride: width,
            width,
            base: 0,
            hist_off: 0,
            coefs,
            gain,
            ring: [0; RING_WORDS],
            hist: [0; HIST_WORDS],
            last: 0,
        }
    }

    /// History slot (word index) of sample `y_j` for a call at counter `j + k`, see [`Self::call`].
    pub fn hist_slot(&self, j: i64) -> usize {
        ((2 * (i64::from(self.hist_off) + j).rem_euclid(8) + 8) % 16) as usize
    }

    /// Bit position of the residual of this call (`stride * counter - base`).
    pub fn bit_position(&self) -> u32 {
        self.stride
            .wrapping_mul(self.counter)
            .wrapping_sub(self.base)
    }

    /// Residual number `counter`: fetch two ring words, align, `FEXT`, `FDEP (SE)`.
    fn fetch_residual(&self) -> i32 {
        let bit = self.bit_position();
        let (off, idx) = (bit & 15, (bit >> 4) as usize & (RING_WORDS - 1));
        let (lo, hi) = (self.ring[idx], self.ring[(idx + 1) & (RING_WORDS - 1)]);
        let window = (lo | (hi << 16)) >> off;
        let shift = 32 - self.width;
        ((window << shift) as i32) >> shift
    }

    /// One call of the loop: decode `y_counter`, interpolate the six samples before it, add to
    /// `bus`, advance the position. Interpolation reads `y_(c-6) .. y_(c-1)`, so the output
    /// position is `c - 4 + phase / 256` in decoded-sample units.
    pub fn call(&mut self, kernel: &SmpKernel, bus: &mut [i32; 2]) -> Call {
        let residual = self.fetch_residual();
        let s = (2 * ((self.hist_off.wrapping_add(self.counter) & 7) as usize)) % HIST_WORDS;
        let pred = (0..4)
            .map(|i| i64::from(self.coefs[i]) * i64::from(self.hist[(s + 2 * i) % HIST_WORDS]))
            .sum::<i64>() as i32;
        let decoded = pred.wrapping_add(residual);
        self.hist[(s + 8) % HIST_WORDS] = decoded;

        let phase_index = (self.phase >> 24) as usize;
        let taps: [i32; 6] = std::array::from_fn(|t| self.hist[(s + 12 + 2 * t) % HIST_WORDS]);
        let interp = kernel.mac_q31(&taps, phase_index);
        let shifted = interp << OUT_SHIFT;
        let add = [
            mul_ssf(shifted, self.gain[0]),
            mul_ssf(shifted, self.gain[1]),
        ];
        bus[0] = bus[0].wrapping_add(add[0]);
        bus[1] = bus[1].wrapping_add(add[1]);
        self.last = shifted;

        let call = Call {
            counter: self.counter,
            phase_index,
            residual,
            decoded,
            interp,
            shifted,
            add,
        };
        let (phase, carry) = self.phase.overflowing_add(self.step_frac);
        self.phase = phase;
        self.counter = self
            .counter
            .wrapping_add(self.step_int)
            .wrapping_add(u32::from(carry));
        call
    }

    /// Loads the history ring as the first page starts: `h[-4..-1]` at the slots of
    /// `y_(counter-4) .. y_(counter-1)`; older slots are cleared.
    pub fn preload_history(&mut self, h: [i32; 4]) {
        self.hist = [0; HIST_WORDS];
        for (k, v) in h.into_iter().enumerate() {
            self.hist[self.hist_slot(i64::from(self.counter) - 4 + k as i64)] = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piano::smp_unpack::{decode_block, pack_units, FIXED_LPC_COEFFS};

    fn residuals(n: usize, width: u32) -> Vec<i32> {
        let span = 1i64 << width;
        let mut s = 12345u32;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (((i64::from(s >> 8) * span) >> 24) - span / 2) as i32
            })
            .collect()
    }

    /// Fills the ring from `units` (first 256 words) and runs `frames` calls at `step`.
    fn run(width: u32, order: usize, step_frac: u32, frames: usize) -> (Vec<Call>, Vec<i32>) {
        let res = residuals(200, width);
        let units = pack_units(&res, width);
        let mut v = SmpVoice::new(width, FIXED_LPC_COEFFS[order], [i32::MAX, i32::MAX]);
        v.ring[..units.len()].copy_from_slice(&units);
        v.step_frac = step_frac;
        let k = SmpKernel::shared();
        let mut bus = [0; 2];
        let calls = (0..frames).map(|_| v.call(k, &mut bus)).collect();
        let mut h = [0; 4];
        (
            calls,
            decode_block(&units, width, order, 200, &mut h).unwrap(),
        )
    }

    #[test]
    fn test_decoded_samples_match_the_unpack_model_for_all_orders() {
        for order in 0..=4 {
            let (calls, want) = run(7, order, 0x8000_0000, 60);
            for c in &calls {
                assert_eq!(c.decoded, want[c.counter as usize], "order {order} {c:?}");
            }
            // The counter advances by one every second call (step 0.5).
            assert_eq!(calls[59].counter, 29);
        }
    }

    #[test]
    fn test_interpolation_reads_samples_c_minus_6_to_c_minus_1() {
        let (calls, want) = run(5, 2, 0xC000_0000, 80);
        let k = SmpKernel::shared();
        for c in &calls {
            let taps: [i32; 6] = std::array::from_fn(|t| {
                let j = i64::from(c.counter) - 6 + t as i64;
                if j < 0 {
                    0
                } else {
                    want[j as usize]
                }
            });
            assert_eq!(c.interp, k.mac_q31(&taps, c.phase_index), "{c:?}");
            assert_eq!(c.shifted, c.interp << 17);
        }
    }

    #[test]
    fn test_phase_carry_advances_the_counter_and_bus_accumulates() {
        let mut v = SmpVoice::new(4, FIXED_LPC_COEFFS[0], [0x4000_0000, 0x2000_0000]);
        v.step_frac = 0xC000_0000;
        let k = SmpKernel::shared();
        let mut bus = [5, -7];
        let counters: Vec<u32> = (0..4).map(|_| v.call(k, &mut bus).counter).collect();
        assert_eq!(counters, [0, 0, 1, 2]);
        assert_eq!(v.phase, 0);
        assert_eq!(bus, [5, -7], "a silent stream adds nothing");
        v.hist = [1000; 16];
        let c = v.call(k, &mut bus);
        assert_eq!(bus[0], 5 + c.add[0]);
        assert_eq!(c.add[0], c.shifted / 2, "gain 0.5");
        assert_eq!(c.add[1], c.shifted / 4, "gain 0.25");
    }

    #[test]
    fn test_ring_wraps_and_base_shifts_the_bit_position() {
        let mut v = SmpVoice::new(16, FIXED_LPC_COEFFS[0], [0; 2]);
        // 16-bit residuals: counter n reads ring word n, wrapping at 256.
        v.ring[3] = 0xFFFE;
        v.ring[0] = 0x0007;
        v.counter = 3;
        assert_eq!(v.fetch_residual(), -2);
        v.counter = 256;
        assert_eq!(v.fetch_residual(), 7);
        v.counter = 4;
        v.base = 16; // bit = 16 * 4 - 16: word 3
        assert_eq!(v.fetch_residual(), -2);
    }

    #[test]
    fn test_interleaved_stereo_streams_use_stride_and_negative_base() {
        // L0 R0 L1 R1 ... at 6 bits: left voice stride 12 base 0, right voice stride 12 base -6.
        let res: Vec<i32> = (0..40)
            .map(|i| if i % 2 == 0 { i / 2 - 9 } else { 20 - i })
            .collect();
        let units = pack_units(&res, 6);
        let mut l = SmpVoice::new(6, FIXED_LPC_COEFFS[0], [0; 2]);
        l.stride = 12;
        let mut r = l.clone();
        r.base = 6u32.wrapping_neg();
        for v in [&mut l, &mut r] {
            v.ring[..units.len()].copy_from_slice(&units);
        }
        for n in 0..20u32 {
            l.counter = n;
            r.counter = n;
            assert_eq!(l.fetch_residual(), res[2 * n as usize], "L {n}");
            assert_eq!(r.fetch_residual(), res[2 * n as usize + 1], "R {n}");
        }
    }

    #[test]
    fn test_preload_history_places_four_samples_before_the_counter() {
        let mut v = SmpVoice::new(8, FIXED_LPC_COEFFS[4], [0; 2]);
        v.preload_history([10, 20, 30, 40]);
        let mut bus = [0; 2];
        let c = v.call(SmpKernel::shared(), &mut bus);
        // order 4 on a straight line continues it: 4*40 - 6*30 + 4*20 - 10 = 50 (residual 0)
        assert_eq!(c.decoded, 50 + c.residual);
        assert_eq!(v.hist_slot(-1), 6);
    }
}
