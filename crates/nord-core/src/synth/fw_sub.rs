//! Firmware ANALOG SUB OSC wave 0 (Core2 stage `0x12179A`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-8.md`;
//! bit-exact Python model `tools/synth_stage_sub.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden8.py` -> `tests_fw_stage_golden8.rs`.
//!
//! The oscillator phase `u` of the firmware advances at **half** the pitch increment: the pulse
//! helper `0x1250C9` (a square, width `0x80000000`, level `s5`) runs on `u`, the saw helper
//! `0x12501F` (level `1 - s5`) on `2 u`, so the saw sounds at the pitch and the square one octave
//! below. The sum is scaled by `1 - 0.3 s5`. `dt` is the pitch increment in cycles per sample,
//! `i_sy` in cycles per SY frame (`a = 2 i_sy`).

use super::fw_stages::{fade, saw_diff};

/// VERIFIED (listing `0x243084`, oracle 320 of 320): level `1 - 0.3 s5`.
const SUB_MIX: f32 = f32::from_bits(0x3E99_999A);

/// One sample from the sub phase `u` (cycles, advancing `dt / 2` per sample).
#[inline]
pub fn sub_core(u: f32, dt: f32, i_sy: f32, s5: f32) -> f32 {
    let saw = (1.0 - s5) * fade(2.0 * i_sy) * saw_diff(2.0 * u, dt);
    let pulse = s5 * fade(i_sy) * (saw_diff(u, 0.5 * dt) - saw_diff(u + 0.5, 0.5 * dt));
    (saw + pulse) * (1.0 - SUB_MIX * s5)
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden8 as gold;
    use super::*;

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SUB_OUT.len())
            .map(|i| {
                let (u, dt) = (
                    f32::from_bits(gold::SUB_PHASE[i]),
                    f32::from_bits(gold::SUB_DT[i]),
                );
                let got = sub_core(u, dt, dt, f32::from_bits(gold::SUB_SHAPE[i]));
                (got - f32::from_bits(gold::SUB_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 3e-5, "worst deviation {worst}");
    }

    #[test]
    fn s5_zero_is_a_pure_saw_and_s5_one_a_pure_square() {
        let (dt, i_sy) = (0.004f32, 0.004f32);
        let saw = sub_core(0.3, dt, i_sy, 0.0);
        assert!((saw - fade(2.0 * i_sy) * saw_diff(0.6, dt)).abs() < 1e-6);
        let sq = sub_core(0.3, dt, i_sy, 1.0);
        let want = 0.7 * fade(i_sy) * (saw_diff(0.3, 0.002) - saw_diff(0.8, 0.002));
        assert!((sq - want).abs() < 1e-6);
    }

    #[test]
    fn core_is_finite_bounded_and_deterministic_over_a_cycle() {
        for s5 in [0.0f32, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..128 {
                    let u = k as f32 / 128.0;
                    let y = sub_core(u, dt, dt, s5);
                    assert!(y.is_finite() && y.abs() < 8.0, "{s5} {dt} {k}: {y}");
                    assert_eq!(y.to_bits(), sub_core(u, dt, dt, s5).to_bits());
                }
            }
        }
    }
}
