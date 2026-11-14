//! Firmware ANALOG SYNC waves 6, 7 and 8 (Core2 stages `0x1239BE`, `0x123C7C`, `0x123FE9`) as
//! opt-in models.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-7.md`;
//! bit-exact Python model `tools/synth_stage_sync6.py` (320 of 320 oracle words per stage), golden
//! `tools/synth_stage_golden7.py` -> `tests_fw_stage_golden7.rs`.
//!
//! The family of `fw_sync3.rs` without the pulse word, with a fixed first edge width 0.5 and no
//! output high pass. Block A is the band-limited square edge of the table-ratio phase, block B the
//! edge of the plain phase (width `c2`), blended `A (0.5 tB + 0.5)`. SYNC 6 multiplies that with a
//! saw-like ramp `g2 z - 1` (`z` = signed phase one half cycle later) and adds `1 - k/2`; SYNC 8
//! reads the table at `0.75 s5`, doubles the ratio, moves the plain phase by `0.25 - 2 inc` cycles
//! and subtracts `1 - k/2` (`k = c2 / (1 - c2)`). SYNC 7 doubles the ratio like SYNC 8 (but reads
//! `s5` directly), stacks a second edge on a quarter ratio phase shifted by 0.25 cycles and a third
//! on the plain phase (`0.25 tC + 0.25`), then multiplies the SYNC 6 ramp. `t` is the phase in
//! cycles, `i_sy` the increment in cycles per SY frame (`a = 2 i_sy`), `shape` the spread word.

use super::fw_shape::signed_phase;
use super::fw_sync::sync_ratio;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// `C (B + clip(A - x, B))`, the saw fade of a pitch-like quantity.
#[inline]
fn fade(x: f32) -> f32 {
    SAW_C * (SAW_B + (SAW_A - x).clamp(-SAW_B, SAW_B))
}

/// `sg - copysign(u^2, sg)` with `sg = sgn(d)`, `u = 1 - min(1, |d q|)`, `d = |x| - c`
/// (VERIFIED, `fw_sync3.rs`).
#[inline]
fn edge(abs_x: f32, c: f32, q: f32) -> f32 {
    let d = abs_x - c;
    let u = 1.0 - (d * q).abs().min(1.0);
    let sg = if d < 0.0 { -1.0 } else { 1.0 };
    sg - (u * u).copysign(sg)
}

/// Lane constants shared by both stages: `(h, g1, g2, q, q2, c2, k)`.
fn constants(i_sy: f32, shape: f32, wave8: bool) -> (f32, f32, f32, f32, f32, f32, f32) {
    let a = (2.0 * i_sy).max(1e-9);
    let h = if wave8 {
        2.0 * sync_ratio(0.75 * shape)
    } else {
        sync_ratio(shape)
    };
    let ah = a * h;
    // VERIFIED (listing `0x2474A0`): SYNC 8 loads `R1 = 0`, so `c2` is 0.5 and `k` is 1.
    let c2 = if wave8 {
        0.5
    } else {
        0.5 - 0.5 * (1.0 - (4.0 * a).min(1.0))
    };
    let q = (0.5 / ah).max(2.0);
    let q2 = (0.5 / a).max(2.0);
    (h, fade(ah), fade(a), q, q2, c2, c2 / (1.0 - c2))
}

/// One SYNC 6 sample (own oscillator, level 1).
#[inline]
pub fn sync6_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let (h, g1, g2, q, q2, c2, k) = constants(i_sy, shape, false);
    let ta = edge(signed_phase(t * h).abs(), 0.5, q);
    let tb = edge(signed_phase(t).abs(), c2, q2);
    let p = (ta * g1 + 1.0) * (0.5 * tb + 0.5);
    let z = signed_phase(t + 0.5);
    -0.5 * g2 * (p * (z * g2 - 1.0) + (1.0 - 0.5 * k))
}

/// One SYNC 8 sample (own oscillator, level 1).
#[inline]
pub fn sync8_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let (h, g1, g2, q, q2, c2, k) = constants(i_sy, shape, true);
    let ta = edge(signed_phase(t * h).abs(), 0.5, q);
    let tb = edge(signed_phase(t + 0.25 - 2.0 * i_sy).abs(), c2, q2);
    let p = (ta * g1 + 1.0) * (0.5 * tb + 0.5);
    g2 * (p - (1.0 - 0.5 * k))
}

/// One SYNC 7 sample (own oscillator, level 1).
#[inline]
pub fn sync7_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = (2.0 * i_sy).max(1e-9);
    let h = 2.0 * sync_ratio(shape);
    let ah = a * h;
    let ah4 = 0.25 * ah;
    let c2 = 0.5 - 0.5 * (1.0 - (4.0 * a).min(1.0));
    let (q, q3, q2) = (
        (0.5 / ah).max(2.0),
        (0.5 / ah4).max(2.0),
        (0.5 / a).max(2.0),
    );
    let (g1, g2, g3) = (fade(ah), fade(a), fade(ah4));
    let a_k = edge(signed_phase(t * h).abs(), 0.5, q) * g1 + 1.0;
    let b_k = edge(signed_phase(t * h * 0.25 + 0.25).abs(), 0.5, q3) * g3 + 1.0;
    let tc = edge(signed_phase(t).abs(), c2, q2);
    let p = a_k * b_k * (0.25 * tc + 0.25);
    let cc = 1.0 - c2 / (2.0 * (1.0 - c2));
    -0.5 * g2 * (p * (signed_phase(t + 0.5) * g2 - 1.0) + cc)
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden7 as gold;
    use super::*;

    fn worst(
        core: fn(f32, f32, f32) -> f32,
        ph: &[u32],
        dt: &[u32],
        sh: &[u32],
        out: &[u32],
    ) -> f32 {
        (0..out.len())
            .map(|i| {
                let got = core(
                    f32::from_bits(ph[i]),
                    f32::from_bits(dt[i]),
                    f32::from_bits(sh[i]),
                );
                (got - f32::from_bits(out[i])).abs()
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn sync6_matches_the_python_model_golden_grid() {
        let w = worst(
            sync6_core,
            &gold::SY6_PHASE,
            &gold::SY6_DT,
            &gold::SY6_SHAPE,
            &gold::SY6_OUT,
        );
        assert!(w < 2e-5, "worst deviation {w}");
    }

    #[test]
    fn sync8_matches_the_python_model_golden_grid() {
        let w = worst(
            sync8_core,
            &gold::SY8_PHASE,
            &gold::SY8_DT,
            &gold::SY8_SHAPE,
            &gold::SY8_OUT,
        );
        assert!(w < 2e-5, "worst deviation {w}");
    }

    #[test]
    fn sync7_matches_the_python_model_golden_grid() {
        let w = worst(
            sync7_core,
            &gold::SY7_PHASE,
            &gold::SY7_DT,
            &gold::SY7_SHAPE,
            &gold::SY7_OUT,
        );
        assert!(w < 2e-5, "worst deviation {w}");
    }

    #[test]
    fn sync8_constants_use_the_fixed_width_and_the_doubled_ratio() {
        let (h, _, _, _, _, c2, k) = constants(0.003, 0.5, true);
        assert_eq!((c2, k), (0.5, 1.0));
        assert_eq!(h, 2.0 * sync_ratio(0.375));
        let (h6, _, _, _, _, c6, _) = constants(0.003, 0.5, false);
        assert_eq!(h6, sync_ratio(0.5));
        assert!(c6 < 0.5);
    }

    #[test]
    fn cores_are_finite_bounded_and_deterministic_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    for core in [sync6_core, sync7_core, sync8_core] {
                        let y = core(p, dt, shape);
                        assert!(y.is_finite() && y.abs() < 80.0, "{shape} {dt} {k}: {y}");
                        assert_eq!(y.to_bits(), core(p, dt, shape).to_bits());
                    }
                }
            }
        }
    }
}
