//! Firmware parity of the multi-tap reverb (types 2..11): the ported wrapper and core against the
//! oracle fingerprints (`tests_fw_rvbm_golden_data.rs`, `tools/fx_rvbmulti.py golden`), 3600 calls
//! per case: an impulse at `kRvbAmt` 127 (wet only) and noise at 63.
//!
//! The [`E40`] path must reproduce every output word (hash, non-zero count, first call with
//! output); the `f32` path is the fast variant and is measured against it.

use super::fw_ext40::{Fp, E40};
use super::fw_rvbm::{Frame, ReverbMulti};
use super::tests_fw_rvbm_golden_data::{Golden, GOLDEN};

/// Seed of the xorshift32 noise of `tools/fx_rvbcore_golden.py`.
const SEED: u32 = 0x2545_F491;
/// Captured `kRvbAmt` words `(send, dry)` of 127 and 63 (`docs/research/data/fx-param-fits.csv`).
const AMT_127: (u32, u32) = (0x3F80_0000, 0);
const AMT_63: (u32, u32) = (0x3EBB_0410, 0x3F6E_4FB0);

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

/// The 8 input words of call `k` (an impulse on the first frame, or xorshift32 noise).
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

fn run<F: Fp>(g: &Golden, calls: usize) -> Vec<[u32; 8]> {
    let mut r = ReverbMulti::<F>::new(g.rvb_type).expect("a captured type");
    let (send, dry) = if g.amount == 127 { AMT_127 } else { AMT_63 };
    r.set_amount_words(f32::from_bits(send), f32::from_bits(dry), true);
    let mut input = Input {
        noise: g.noise,
        s: SEED,
    };
    (0..calls)
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
fn e40_port_reproduces_the_oracle_fingerprints() {
    for g in &GOLDEN {
        let out = run::<E40>(g, g.calls);
        let flat = || out.iter().flatten().copied();
        let tag = format!("type {} noise {}", g.rvb_type, g.noise);
        assert_eq!(
            flat().filter(|w| w & 0x7FFF_FFFF != 0).count(),
            g.nonzero,
            "{tag}"
        );
        let first = out
            .iter()
            .position(|r| r.iter().any(|w| w & 0x7FFF_FFFF != 0))
            .map_or(-1, |k| k as i64);
        assert_eq!(first, g.first, "{tag}: first call with output");
        assert_eq!(fnv(flat()), g.hash, "{tag}: hash of every output word");
    }
}

#[test]
fn f32_path_follows_the_e40_path_closely() {
    for g in GOLDEN.iter().filter(|g| g.noise && g.rvb_type % 3 == 2) {
        let calls = 600;
        let (a, b) = (run::<f32>(g, calls), run::<E40>(g, calls));
        let (mut peak, mut err) = (0.0f32, 0.0f32);
        for (x, y) in a.iter().flatten().zip(b.iter().flatten()) {
            let (x, y) = (f32::from_bits(*x), f32::from_bits(*y));
            peak = peak.max(y.abs());
            err = err.max((x - y).abs());
        }
        assert!(
            peak > 1e-3 && err < 0.02 * peak,
            "type {} peak {peak} err {err}",
            g.rvb_type
        );
    }
}
