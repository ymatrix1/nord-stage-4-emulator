//! Firmware ANALOG SHAPE SINE wave 4 (Core2 stage `0x120DAB`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-4.md`;
//! bit-exact Python model `tools/synth_stage_shape_sine2.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden4.py` -> `tests_fw_stage_golden4.rs`.
//!
//! The stage folds the sine into `y = |sine (1 + s5) + (1 - s5)|`, adds a cubic dip where `y` is
//! small (`w^3 k`), subtracts a spread-dependent offset `p`, scales by the saw fade `g` and ends in
//! a one-pole output high pass `y_k = (x_k - x_(k-1)) + c y_(k-1)` over the sample stream.
//! `t` is the phase in cycles, `i_sy` the increment in cycles per SY frame (`a = 2 i_sy`), `shape`
//! the spread word `s5` (idle 0.5; knob scale open, ASSUMED to follow Osc Ctrl).

use super::fw_shape::signed_phase;
use super::fw_sine_warps::sine_poly;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED (listing `0x241BD4`): lower bound `0x358637BD` of the width law.
const EPS: f32 = f32::from_bits(0x3586_37BD);
/// VERIFIED (listing `0x241BB6`): `10 s5` is clipped at 1.
const TEN: f32 = 10.0;
/// VERIFIED (listing `0x241BEC..0x241C26`): coefficients of `p(s5)`, highest order first.
const POLY: [f32; 4] = [
    f32::from_bits(0x3EFC_9646),
    f32::from_bits(0xBFBB_9043),
    f32::from_bits(0x3FEC_7011),
    f32::from_bits(0xBF1A_171A),
];
/// VERIFIED (listing `0x241C2E`): numerator `1 / 2 pi` of the reciprocal `ir`.
const INV_TWO_PI: f32 = f32::from_bits(0x3E22_F981);
/// VERIFIED (listing `0x241C5A`, `0x241CF8`): `pi` and `2/3` (`0x3F2AAA3B`).
const PI: f32 = f32::from_bits(0x4049_0FD0);
const TWO_THIRDS: f32 = f32::from_bits(0x3F2A_AA3B);
/// VERIFIED (dump of `DM(0xC22DA)` after a pass of the oracle, `tools/synth_wave_oracle.py`): the
/// idle high-pass coefficient `0x3F7F9DB2`. ASSUMED: it stays at this value in the Synth (its
/// writer is not located); at 48 kHz this is a 5.6 Hz corner.
pub const HP_COEF: f32 = f32::from_bits(0x3F7F_9DB2);

/// One sample of the stage before the output high pass (own oscillator, level 1).
#[inline]
pub fn shape_sine_hp_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let g = SAW_C * (SAW_B + (SAW_A - a).clamp(-SAW_B, SAW_B));
    let u = 1.0 - (TEN * shape).clamp(-1.0, 1.0);
    let q = (1.0 - u * u).max(EPS);
    // ASSUMED: the ratio word is 1, so the pitch word equals `a`
    let pq = a * q;
    let ir = INV_TWO_PI / pq.max(1e-9);
    let k = PI * TWO_THIRDS * pq;
    let p = (((POLY[0] * shape + POLY[1]) * shape + POLY[2]) * shape + POLY[3]) * shape + 1.0;
    let y = (sine_poly(2.0 * signed_phase(t)) * (shape + 1.0) + (1.0 - shape)).abs();
    let w = 1.0 - (y * ir).min(1.0);
    (w * w * w * k + y - p) * g
}

/// The one-pole output high pass of the stage (state: previous input and output).
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputHighPass {
    x1: f32,
    y1: f32,
}

impl OutputHighPass {
    /// One sample through `y = (x - x_prev) + c y_prev`.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = (x - self.x1) + HP_COEF * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden4 as gold;
    use super::*;

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SSH_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SSH_PHASE[i]),
                    f32::from_bits(gold::SSH_DT[i]),
                );
                let got = shape_sine_hp_core(p, dt, f32::from_bits(gold::SSH_SHAPE[i]));
                (got - f32::from_bits(gold::SSH_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn high_pass_follows_the_stage_recurrence_and_blocks_dc() {
        let mut hp = OutputHighPass::default();
        // first sample: y = x - 0 + c * 0
        assert_eq!(hp.process(0.5), 0.5);
        // second sample: y = (0.5 - 0.5) + c * 0.5
        assert_eq!(hp.process(0.5).to_bits(), (HP_COEF * 0.5).to_bits());
        let mut last = 0.0;
        for _ in 0..20_000 {
            last = hp.process(0.5);
        }
        assert!(last.abs() < 1e-3, "dc leaks: {last}");
    }

    #[test]
    fn core_is_finite_and_bounded_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let y = shape_sine_hp_core(k as f32 / 256.0, dt, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "{shape} {dt} {k}: {y}");
                }
            }
        }
    }
}
