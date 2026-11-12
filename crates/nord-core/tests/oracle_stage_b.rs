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
#[path = "support/stage_b_golden.rs"]
mod golden;

use golden::*;
use nord_core::sinc::ext40;
use nord_core::sinc::{SincKernel, SincKernelSet, CENTER_TAP, PHASES, TAPS};

/// One float32 ulp in [0.5, 1): 2^-24.
const ULP_HALF: f32 = 5.960_464_5e-8;

/// Largest `|engine kernel - oracle table|` over the sampled table values: one ulp (5.96e-8 near
/// 0.8). Over all 18,432 values the engine (decimal cutoffs kept in f64) is within 5.95e-8 of the
/// firmware table (`ns4_oracle_compare.py` on the `sy_stage_b_kernel` dump, and `sy-dsp-map.md`);
/// widening `1.1f32` to f64 before the fix gave 1.19e-7. VERIFIED.
const KERNEL_TOL: f32 = ULP_HALF;

/// Largest `|engine render - oracle|` over the 64 golden frames (both channels) with the engine
/// kernel and the DSP's 40-bit arithmetic. Measured: 5.96e-8 (one ulp at 0.5), previously bound 1e-7.
const RENDER_TOL: f32 = ULP_HALF;

/// Bound for the plain-f32 `SincKernel::interpolate` path at the true fractional positions
/// (floored phase, sequential f32 accumulation). Measured: 5.96e-8 left, 8.94e-8 right.
const INTERPOLATE_TOL: f32 = 9.0e-8;

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
fn engine_kernel_stays_within_one_ulp_of_the_oracle_table() {
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

/// Centre-tap position (frames, `f64`) of output frame `n`: the position word is exact in f64.
fn centre_pos(n: usize) -> f64 {
    f64::from(pos_word(n)) / f64::from(1u32 << 25) + CENTER_TAP as f64
}

fn oracle_columns() -> [(Vec<f32>, Vec<f32>); 2] {
    [
        (hash_ring(37, 11), oracle(&ORACLE_L)),
        (hash_ring(23, 5), oracle(&ORACLE_R)),
    ]
}

#[test]
fn engine_interpolate_matches_the_oracle_at_the_true_fractional_positions() {
    // `SincKernel::interpolate` takes the centre-sample position (frame index + 5 + phase / 256 +
    // the ignored low bits), floors the phase like the hardware and multiplies by fc (= 1.0 for
    // kernel 2). Positions are the real ones (fraction 76.8 phases at the start): only the kernel
    // table and the f32 accumulation differ from the oracle.
    let kern = SincKernelSet::new().get(2).clone();
    assert_eq!(TAPS, 12);
    for (ch, (ring, want)) in oracle_columns().into_iter().enumerate() {
        let got: Vec<f32> = (0..FRAMES)
            .map(|n| kern.interpolate(&ring, centre_pos(n)))
            .collect();
        let e = max_abs(&got, &want);
        assert!(e <= INTERPOLATE_TOL, "channel {ch}: max error {e:e}");
    }
}

#[test]
fn engine_interpolate_ext40_with_the_engine_kernel_stays_within_one_ulp() {
    let kern = SincKernelSet::new().get(2).clone();
    for (ch, (ring, want)) in oracle_columns().into_iter().enumerate() {
        let got: Vec<f32> = (0..FRAMES)
            .map(|n| kern.interpolate_ext40(&ring, centre_pos(n), kern.cutoff()))
            .collect();
        let e = max_abs(&got, &want);
        assert!(e <= RENDER_TOL, "channel {ch}: max error {e:e}");
    }
}

#[test]
fn engine_floors_the_phase_like_the_oracle() {
    // Start position 0.3 frames = phase 76.8. The oracle uses the 8 phase bits only (phase 76) and
    // the engine now floors too (TD-08 item 1, fixed). This pinned the old rounding to phase 77,
    // which was 3.4e-3 off (SNR 41 to 45 dB), see `docs/research/oracle-vs-engine.md` section 5.
    let kern = SincKernelSet::new().get(2).clone();
    let ring = hash_ring(37, 11);
    let (_, phase0) = frame_index_and_phase(pos_word(0));
    assert_eq!(phase0, 76);
    let oracle0 = oracle(&ORACLE_L)[0];
    let floored = kern.interpolate(&ring, CENTER_TAP as f64 + 0.3);
    assert!(
        (floored - oracle0).abs() <= INTERPOLATE_TOL,
        "{floored} vs {oracle0}"
    );
    // what the old nearest-phase rule returned (phase 77): far more than any arithmetic effect
    let rounded = kern.interpolate(&ring, CENTER_TAP as f64 + 77.0 / PHASES as f64);
    assert!((rounded - oracle0).abs() > 1.0e-3, "{rounded} vs {oracle0}");
}

#[test]
fn engine_cutoffs_are_the_decimal_values_in_f64() {
    // The table matches the decimal cutoffs evaluated in f64 (5.95e-8); the widened f32 cutoff
    // gave 1.19e-7. Compare the engine weights with the f32-widened kernel to see the change.
    let set = SincKernelSet::new();
    let widened = SincKernel::new(f64::from(1.1_f32));
    let decimal = set.get(0);
    let diff = (0..PHASES)
        .flat_map(|p| (0..TAPS).map(move |t| (p, t)))
        .map(|(p, t)| (widened.weights(p)[t] - decimal.weights(p)[t]).abs())
        .fold(0.0_f32, f32::max);
    assert!(diff > 0.0 && diff < 2.0e-7, "{diff:e}");
}
