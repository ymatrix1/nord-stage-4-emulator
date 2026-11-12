//! Oracle versus engine: the SY1SY2 sample resampler (Stage B, `0x1C10EE`) against `SincKernelSet`.
//!
//! The oracle output (`sharc-ref` running the firmware routine on the SHARC core model) for a
//! 64-frame stereo case is embedded as float32 bit patterns in `tests/support/stage_b_golden.rs`.
//! This test regenerates the same frames and states how far the engine is from the oracle.
//! Method, results table and the status of every assumption: `docs/research/oracle-vs-engine.md`.
//!
//! Case (`tools/oracle_scenarios/sy_stage_b_golden.scn`): 128-frame ring of the `hash` signal
//! (`((n * mul + add) % 64) / 64 - 0.5`, mul/add 37/11 left and 23/5 right), kernel 2 (`fc = 1.00`),
//! gain 1.0, ratio 0.5 source frames per output frame, start position 0.3 frames.
#[path = "support/ext40.rs"]
mod ext40;
#[path = "support/stage_b_golden.rs"]
mod golden;

use golden::*;
use nord_core::sinc::{SincKernelSet, CENTER_TAP, PHASES, TAPS};

/// Largest `|engine kernel - oracle table|` over all 18,432 table values: 1.19e-7 (2 ulp near 1),
/// measured with `ns4_oracle_compare.py` on the `sy_stage_b_kernel` dump. VERIFIED. The cause is
/// `kernel_value(fc: f32, ..)` widening the f32 cutoff (1.1f32 = 1.10000002384) to f64; with the
/// decimal cutoff the error against the table is 5.95e-8 (model fit in `sy-dsp-map.md`).
const KERNEL_TOL: f32 = 1.2e-7;

/// Largest `|engine render - oracle|` over the 64 golden frames with the engine kernel and the
/// DSP's 40-bit arithmetic: the kernel difference (weights differ by up to 2 ulp) times the signal.
/// Measured: 5.96e-8 (one ulp at 0.5) on both channels; the bound is 1.0e-7.
const RENDER_TOL: f32 = 1.0e-7;

/// Bound for the plain-f32 `SincKernel::interpolate` path (sequential f32 accumulation).
/// Measured with the same engine weights: 5.96e-8 (left channel, the one tested here).
const INTERPOLATE_TOL: f32 = 1.5e-7;

const FRAMES: usize = 64;

fn hash_ring(mul: u64, add: u64) -> Vec<f32> {
    (0..128u64)
        .map(|n| ((n * mul + add) % 64) as f32 / 64.0 - 0.5)
        .collect()
}

/// Position word of output frame `n`: start 0.3 frames (25 fraction bits), `+= 2 * step` per call,
/// `step = ratio * 2^24` (VERIFIED against the oracle, see the document).
fn pos_word(n: usize) -> u32 {
    let start = (0.3_f64 * f64::from(1u32 << 25)).round() as u32;
    let step = (0.5_f64 * f64::from(1u32 << 24)).round() as u32;
    start.wrapping_add((2 * n as u32).wrapping_mul(step))
}

fn frame_index_and_phase(pos: u32) -> (usize, usize) {
    ((pos >> 25) as usize, ((pos >> 17) & 0xFF) as usize)
}

fn oracle(bits: &[u32]) -> Vec<f32> {
    bits.iter().map(|b| f32::from_bits(*b)).collect()
}

fn oracle_weights(phase: usize) -> Vec<f32> {
    let w = match phase {
        76 => &ORACLE_WEIGHTS_PHASE_76,
        204 => &ORACLE_WEIGHTS_PHASE_204,
        other => panic!("golden case uses phases 76 and 204 only, got {other}"),
    };
    oracle(w)
}

fn render(ring: &[f32], weights: impl Fn(usize) -> Vec<f32>) -> Vec<f32> {
    (0..FRAMES)
        .map(|n| {
            let (idx, phase) = frame_index_and_phase(pos_word(n));
            ext40::mac_ext40(
                &weights(phase),
                |t| ring[(idx + t) % ring.len()],
                1.0,
                false,
                true,
            )
        })
        .collect()
}

fn max_abs(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn position_words_alternate_between_phases_76_and_204() {
    for n in 0..FRAMES {
        let (idx, phase) = frame_index_and_phase(pos_word(n));
        assert_eq!(phase, if n % 2 == 0 { 76 } else { 204 }, "frame {n}");
        assert_eq!(idx, n / 2, "frame {n}");
    }
}

#[test]
fn stage_b_is_bit_exact_with_the_oracle_weights_and_40_bit_arithmetic() {
    let l = render(&hash_ring(37, 11), oracle_weights);
    let r = render(&hash_ring(23, 5), oracle_weights);
    for (n, (got, want)) in l.iter().zip(oracle(&ORACLE_L)).enumerate() {
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "L frame {n}: {got} vs {want}"
        );
    }
    for (n, (got, want)) in r.iter().zip(oracle(&ORACLE_R)).enumerate() {
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "R frame {n}: {got} vs {want}"
        );
    }
}

#[test]
fn engine_kernel_stays_within_two_ulp_of_the_oracle_table() {
    let set = SincKernelSet::new();
    for (k, tap, phase, bits) in ORACLE_TABLE_SAMPLES {
        let want = f32::from_bits(bits);
        let got = set.get(k).weights(phase)[tap];
        assert!(
            (got - want).abs() <= KERNEL_TOL,
            "kernel {k} tap {tap} phase {phase}: engine {got} oracle {want}"
        );
    }
}

#[test]
fn engine_kernel_with_dsp_arithmetic_is_within_tolerance_of_the_oracle() {
    let set = SincKernelSet::new();
    let engine = |phase| set.get(2).weights(phase).to_vec();
    let l = render(&hash_ring(37, 11), engine);
    let r = render(&hash_ring(23, 5), engine);
    let (el, er) = (
        max_abs(&l, &oracle(&ORACLE_L)),
        max_abs(&r, &oracle(&ORACLE_R)),
    );
    assert!(el <= RENDER_TOL && er <= RENDER_TOL, "L {el:e} R {er:e}");
    // the kernel table, not the arithmetic, is what is left: the error is not zero
    assert!(el > 0.0 || er > 0.0);
}

#[test]
fn engine_interpolate_matches_the_oracle_when_the_phase_is_exact() {
    // `SincKernel::interpolate` takes the centre-sample position (frame index + 5 + phase / 256),
    // rounds the phase to nearest and multiplies by fc (= 1.0 for kernel 2). Phases 76 and 204 are
    // exact there, so only kernel and f32-accumulation differences remain.
    let kern = SincKernelSet::new().get(2).clone();
    let ring = hash_ring(37, 11);
    let want = oracle(&ORACLE_L);
    let got: Vec<f32> = (0..FRAMES)
        .map(|n| {
            let (idx, phase) = frame_index_and_phase(pos_word(n));
            let pos = (idx + CENTER_TAP) as f64 + phase as f64 / PHASES as f64;
            kern.interpolate(&ring, pos)
        })
        .collect();
    assert_eq!(TAPS, 12);
    let e = max_abs(&got, &want);
    assert!(e <= INTERPOLATE_TOL, "max error {e:e}");
}

#[test]
fn known_difference_engine_rounds_the_phase_where_the_oracle_floors() {
    // Start position 0.3 frames = phase 76.8. The oracle uses the 8 phase bits only (phase 76);
    // `interpolate` rounds to nearest (phase 77). The two answers differ by far more than any
    // arithmetic effect, which is the finding behind `docs/research/oracle-vs-engine.md` section 5.
    let kern = SincKernelSet::new().get(2).clone();
    let ring = hash_ring(37, 11);
    let (_, phase0) = frame_index_and_phase(pos_word(0));
    assert_eq!(phase0, 76);
    let engine = kern.interpolate(&ring, CENTER_TAP as f64 + 0.3);
    let oracle0 = oracle(&ORACLE_L)[0];
    assert!((engine - oracle0).abs() > 1.0e-3, "{engine} vs {oracle0}");
}
