//! Firmware ANALOG SYNC waves 3..5 (Core2 stage `0x122C5C`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-6.md`;
//! bit-exact Python model `tools/synth_stage_sync3.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden6.py` -> `tests_fw_stage_golden6.rs`.
//!
//! Not the code of `fw_sync.rs`. The table ratio `h` (same table, `sync_ratio`) multiplies the
//! phase (`ASHIFT 7` of the unsigned product wraps it) and the pulse word offsets the result; each
//! half is a band-limited square edge `sg (1 - u^2)` around a width `c`, and the two are blended
//! `(0.5 tB + 0.5)(tA - 1) + 1 + w1`. The gain is `g(a) g(a h) (1 - |r|) / (1 - |w1|)` and the
//! stage ends in the one-pole output high pass of `fw_sine_hp.rs`. `t` is the phase in cycles,
//! `i_sy` the increment in cycles per SY frame (`a = 2 i_sy`), `shape` the spread word `s5`.

use super::fw_shape::signed_phase;
use super::fw_sync::sync_ratio;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED (dump of `DM(0xC15AE)` after a pass of the oracle, `tools/synth_wave_oracle.py`): the
/// idle pulse word `DM(-8, I1)`. ASSUMED: it stays at this value in the Synth (its writer is open).
pub const PULSE_WORD: u32 = 0x4000_0000;

/// `C (B + clip(A - x, B))`, the saw fade of a pitch-like quantity.
#[inline]
fn fade(x: f32) -> f32 {
    SAW_C * (SAW_B + (SAW_A - x).clamp(-SAW_B, SAW_B))
}

/// `sg - copysign(u^2, sg)` with `sg = sgn(d)`, `u = 1 - min(1, |d q|)`, `d = |x| - c`
/// (VERIFIED, listing `0x245B5E..0x245BD4`: the sign register, not `d`, enters the difference).
#[inline]
fn edge(abs_x: f32, c: f32, q: f32) -> f32 {
    let d = abs_x - c;
    let u = 1.0 - (d * q).abs().min(1.0);
    let sg = if d < 0.0 { -1.0 } else { 1.0 };
    sg - (u * u).copysign(sg)
}

/// One sample of the stage before the output high pass (own oscillator, level 1).
#[inline]
pub fn sync3_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = (2.0 * i_sy).max(1e-9);
    let h = sync_ratio(shape);
    let ah = a * h;
    let e = 1.0 - (4.0 * ah).min(1.0);
    // the pulse word as a signed fraction `p` (a phase offset of `p / 2` cycles)
    let p = PULSE_WORD as i32 as f32 / 2_147_483_648.0;
    let r = 2.0 * (p - 0.5);
    let w1 = r.max(-e).min(e);
    let c1 = 0.5 + 0.5 * w1;
    let q = (0.5 / ah).max(2.0);
    let c2 = 0.5 - 0.5 * (1.0 - (4.0 * a).min(1.0));
    let q2 = (0.5 / a).max(2.0);
    let ratio = (1.0 - r.abs()) / (1.0 - w1.abs()).max(1e-6);
    let ta = edge(signed_phase(t * h + 0.5 * p).abs(), c1, q);
    let tb = edge(signed_phase(t).abs(), c2, q2);
    let v = 0.5 * tb + 0.5;
    ((v * (ta - 1.0) + 1.0) + w1) * fade(a) * fade(ah) * ratio
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden6 as gold;
    use super::*;

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SY3_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SY3_PHASE[i]),
                    f32::from_bits(gold::SY3_DT[i]),
                );
                let got = sync3_core(p, dt, f32::from_bits(gold::SY3_SHAPE[i]));
                (got - f32::from_bits(gold::SY3_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn edge_is_plus_minus_one_away_from_the_width_and_dips_at_it() {
        assert_eq!(edge(0.9, 0.5, 4.0), 1.0);
        assert_eq!(edge(0.1, 0.5, 4.0), -1.0);
        // at the width: d = 0, u = 1, sg = +1: 1 - 1 = 0
        assert_eq!(edge(0.5, 0.5, 4.0), 0.0);
    }

    #[test]
    fn core_is_finite_bounded_and_deterministic_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    let y = sync3_core(p, dt, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "{shape} {dt} {k}: {y}");
                    assert_eq!(y.to_bits(), sync3_core(p, dt, shape).to_bits());
                }
            }
        }
    }
}
