//! Firmware parity of the reverb of types 0 and 1: the ported core and wrapper against oracle
//! output (`tests_fw_rvb_golden_data.rs`, `tools/fx_rvbcore_golden.py`), 3000 calls per case.
//!
//! The [`E40`] path must reproduce every output word (hash and samples); the `f32` path is the
//! fast variant and is measured against it.

use super::fw_ext40::{Fp, E40};
use super::fw_reverb::{Frame, Reverb};
use super::fw_rvb_wrap::Wrap;
use super::tests_fw_rvb_golden_data::{Golden, CALLS, GOLDEN, SEED};

fn fnv(words: impl IntoIterator<Item = u32>) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for w in words {
        let w = if w & 0x7FFF_FFFF == 0 { 0 } else { w };
        for b in w.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3);
        }
    }
    h
}

/// The 8 input words of call `k` (the tool's impulse, or xorshift32 noise in `[-0.25, 0.25)`).
struct Input {
    noise: bool,
    s: u32,
}

impl Input {
    fn next(&mut self, k: usize) -> [Frame; 4] {
        let mut f = [[0.0f32; 2]; 4];
        for (i, v) in f.iter_mut().flatten().enumerate() {
            *v = if self.noise {
                self.s ^= self.s << 13;
                self.s ^= self.s >> 17;
                self.s ^= self.s << 5;
                ((self.s >> 8) as f32 - 8_388_608.0) / 33_554_432.0
            } else {
                f32::from(k == 0 && i < 2)
            };
        }
        f
    }
}

fn run<F: Fp>(g: &Golden) -> Vec<[u32; 8]> {
    let mut r = Reverb::<F>::new(g.rvb_type);
    r.set_amount_words(f32::from_bits(g.send), f32::from_bits(g.dry), true);
    let mut input = Input {
        noise: g.noise,
        s: SEED,
    };
    (0..CALLS)
        .map(|k| {
            let mut f = input.next(k);
            r.process_block4(&mut f);
            let mut w = [0u32; 8];
            for (o, v) in w.iter_mut().zip(f.iter().flatten()) {
                *o = if v.to_bits() & 0x7FFF_FFFF == 0 {
                    0
                } else {
                    v.to_bits()
                };
            }
            w
        })
        .collect()
}

#[test]
fn e40_port_reproduces_the_oracle_word_for_word() {
    for g in &GOLDEN {
        let out = run::<E40>(g);
        let flat = || out.iter().flatten().copied();
        let tag = format!("type {} noise {}", g.rvb_type, g.noise);
        assert_eq!(
            flat().filter(|w| w & 0x7FFF_FFFF != 0).count(),
            g.nonzero,
            "{tag}"
        );
        let first = out
            .iter()
            .position(|r| r.iter().any(|w| w & 0x7FFF_FFFF != 0));
        assert_eq!(first, Some(g.first), "{tag}: first call with output");
        for (k, want) in g.samples {
            assert_eq!(&out[*k], want, "{tag}: call {k}");
        }
        assert_eq!(
            fnv(flat()),
            g.hash,
            "{tag}: hash of all {} words",
            8 * CALLS
        );
    }
}

#[test]
fn f32_path_tracks_the_bit_exact_path() {
    for g in &GOLDEN {
        let (exact, fast) = (run::<E40>(g), run::<f32>(g));
        let (mut sig, mut err, mut worst) = (0.0f64, 0.0f64, 0.0f64);
        for (a, b) in exact.iter().flatten().zip(fast.iter().flatten()) {
            let (a, b) = (f64::from(f32::from_bits(*a)), f64::from(f32::from_bits(*b)));
            sig += a * a;
            err += (a - b) * (a - b);
            worst = worst.max((a - b).abs());
        }
        let snr = 10.0 * (sig / err.max(1e-300)).log10();
        println!(
            "type {} noise {}: f32 vs ext40 SNR {snr:.1} dB, worst {worst:.2e}",
            g.rvb_type, g.noise
        );
        assert!(snr > 60.0, "f32 path SNR {snr:.1} dB");
    }
}

// --- the generic wrapper on the 40-bit path against the wrapper golden words ---------------
const OUT4: [u32; 64] = super::tests_rvb_wrap_golden::GOLDEN_OUT4;
const STATES: [u32; 24] = super::tests_rvb_wrap_golden::GOLDEN_STATES;

fn wrap_input(ch: usize, n: usize) -> f32 {
    let (mul, add, modulus) = if ch == 0 { (37, 11, 64) } else { (53, 5, 61) };
    ((mul * n + add) % modulus) as f32 / 64.0 - 0.5
}

fn wrap_wet(k: usize, which: usize, ch: usize) -> f32 {
    ((k + 3 * which + 5 * ch) % 7) as f32 / 16.0 - 3.0 / 16.0
}

#[test]
fn e40_wrapper_matches_the_wrapper_golden_words() {
    let mut w = Wrap::<E40>::new();
    let mut out = Vec::new();
    for k in 0..8 {
        let mut f: [Frame; 4] =
            std::array::from_fn(|i| [wrap_input(0, 4 * k + i), wrap_input(1, 4 * k + i)]);
        let wet = |which: usize| [wrap_wet(k, which, 0), wrap_wet(k, which, 1)];
        w.process4(&mut f, 0.25, |_, _| (wet(0), wet(1)));
        out.extend(f.iter().flatten().map(|v| v.to_bits()));
    }
    assert_eq!(out, OUT4, "0x12417E words, 40-bit wrapper");
    let st: Vec<u32> = w.state_words().iter().map(|v| v.to_bits()).collect();
    assert_eq!(st, STATES, "state words");
}
