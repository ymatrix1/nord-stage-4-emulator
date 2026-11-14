//! Firmware ANALOG SYNC wave 2 (Core2 stage `0x122A32`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-5.md`;
//! bit-exact Python model `tools/synth_stage_sync.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden5.py` -> `tests_fw_stage_golden5.rs`.
//!
//! A hard-sync saw pair. The spread word `s5` indexes a table (`fw_sync_table.rs`, linear
//! interpolation) giving the sync ratio `h`: the phase is multiplied by `h` and stops after
//! `trunc(h)` cycles (integer `CLIP`, `ASHIFT 7`, `BTGL 31`). The output is the band-limited
//! difference of a saw at the master phase (`-frac(h)` times a parabola edge at `x = 0`) and the
//! slave saw, and the whole sample is scaled by `-g`. `t` is the phase in cycles, `i_sy` the
//! increment in cycles per SY frame (`f = 2 i_sy`), `shape` the spread word `s5` (idle 0.5,
//! ASSUMED to follow Osc Ctrl).

use super::fw_shape::signed_phase;
use super::fw_sync_table::SYNC_TABLE;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED (listing `0x24549C..0x2454B0`): the index is `FEXT(FIX(s5, 30), 23:8)`, the
/// fraction `FDEP(.., 8:23)`: 128 steps per unit of `s5`, 256 entries.
const STEPS: f32 = 128.0;

/// Sync ratio `h` of the spread word (table interpolation, clamped to the 256 entries).
#[inline]
pub fn sync_ratio(shape: f32) -> f32 {
    let x = (shape * STEPS).clamp(0.0, 255.999_99);
    let i = x as usize;
    let c = x - i as f32;
    let (lo, hi) = (
        f32::from_bits(SYNC_TABLE[i]),
        f32::from_bits(SYNC_TABLE[i + 1]),
    );
    lo + (hi - lo) * c
}

/// `(1 - min(1, |x| r))^2` of a signed phase `x`.
#[inline]
fn parabola(x: f32, r: f32) -> f32 {
    let d = 1.0 - (x.abs() * r).min(1.0);
    d * d
}

/// One sample of the stage (own oscillator, level 1, gain `-g` included).
#[inline]
pub fn sync2_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let f = (2.0 * i_sy).max(1e-9);
    let h = sync_ratio(shape);
    let g = SAW_C * (SAW_B + (SAW_A - f * h).clamp(-SAW_B, SAW_B));
    let ih = h.trunc();
    let frac = h - ih;
    let r7 = 0.5 / f;
    let q = r7 / h;
    // block A: the phase shifted by -1 / 2h cycles, multiplied by h and stopped after `ih` cycles
    let shifted = (t - 0.5 / h).rem_euclid(1.0);
    let a = signed_phase((shifted * h).min(ih) + 0.5);
    // block B: the slave saw
    let u = signed_phase(t * h + 0.5);
    let d = u - (parabola(a, q)).copysign(u);
    let x = signed_phase(t);
    let edge = parabola(x, r7).copysign(x) * -frac;
    (edge - d) * -g
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden5 as gold;
    use super::*;

    #[test]
    fn ratio_follows_the_table_nodes_and_interpolates_between_them() {
        assert_eq!(sync_ratio(0.0), 1.0);
        assert_eq!(sync_ratio(0.5), f32::from_bits(SYNC_TABLE[64]));
        let mid = sync_ratio(0.5 + 0.5 / STEPS);
        let (a, b) = (
            f32::from_bits(SYNC_TABLE[64]),
            f32::from_bits(SYNC_TABLE[65]),
        );
        assert!((mid - 0.5 * (a + b)).abs() < 1e-5);
    }

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SY2_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SY2_PHASE[i]),
                    f32::from_bits(gold::SY2_DT[i]),
                );
                let got = sync2_core(p, dt, f32::from_bits(gold::SY2_SHAPE[i]));
                (got - f32::from_bits(gold::SY2_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-6, "worst deviation {worst}");
    }

    #[test]
    fn core_is_finite_bounded_and_deterministic_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    let y = sync2_core(p, dt, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "{shape} {dt} {k}: {y}");
                    assert_eq!(y.to_bits(), sync2_core(p, dt, shape).to_bits());
                }
            }
        }
    }
}
