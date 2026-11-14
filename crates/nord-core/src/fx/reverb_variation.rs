//! Reverb VARIATION of the left-hand types (manual OS v1.6x p.52-53): SPRING gets a longer tail,
//! BOOTH and ROOM a longer pre-delay. (STAGE, HALL and CATHEDRAL have CHORALE instead, see
//! `ReverbColor::Chorale`.)
//!
//! No firmware DSP is decoded for either, so every number is `ASSUMED`; with
//! `LayerFxChain::reverb_variation == false` (the default) none of this touches the signal, so
//! renders stay bit-identical to the engine without the feature.

use super::*;

/// Pre-delay line length per channel: 8192 samples (about 170 ms at 48 kHz, 35 ms up to 234 kHz).
pub(super) const PREDELAY_LEN: usize = 8192;

/// ASSUMED: SPRING VARIATION raises the feedback gain from 0.77 to 0.86, about 1.7 times the decay
/// time (`ln(1/0.77) / ln(1/0.86)`); the manual only says "longer". Confirm with a decay measurement
/// of Spring with and without VARIATION on a Stage 4.
pub(super) const SPRING_VARIATION_DECAY: f32 = 0.86;

/// ASSUMED: BOOTH / ROOM VARIATION pre-delay of 35 ms ("longer pre-delay", the normal types have
/// none); confirm by recording the onset of the reverb tail after an impulse.
pub(super) const PREDELAY_VARIATION_SEC: f32 = 0.035;

/// Pre-delay ring buffer of the reverb input (preallocated, no allocation while rendering).
#[derive(Debug, Clone)]
pub(super) struct RevPredelay {
    buf: Box<[[f32; PREDELAY_LEN]; 2]>,
    idx: usize,
}

impl RevPredelay {
    pub(super) fn new() -> Self {
        Self {
            buf: Box::new([[0.0; PREDELAY_LEN]; 2]),
            idx: 0,
        }
    }

    /// Writes `(l, r)` and returns the pair written `delay` samples ago (`delay` is clamped).
    pub(super) fn process(&mut self, l: f32, r: f32, delay: usize) -> [f32; 2] {
        let delay = delay.clamp(1, PREDELAY_LEN - 1);
        self.buf[0][self.idx] = l;
        self.buf[1][self.idx] = r;
        let read = (self.idx + PREDELAY_LEN - delay) % PREDELAY_LEN;
        self.idx = (self.idx + 1) % PREDELAY_LEN;
        [self.buf[0][read], self.buf[1][read]]
    }
}

impl LayerFxChain {
    /// Feedback gain of the reverb type, with the SPRING VARIATION applied.
    pub(super) fn reverb_decay(&self, base: f32) -> f32 {
        if self.reverb_variation && self.reverb_model == ReverbModel::Spring {
            SPRING_VARIATION_DECAY
        } else {
            base
        }
    }

    /// Reverb input after the pre-delay of BOOTH / ROOM VARIATION (the dry path stays undelayed).
    pub(super) fn reverb_input(&mut self, l: f32, r: f32) -> [f32; 2] {
        let delayed = self.rev_predelay.process(
            l,
            r,
            (PREDELAY_VARIATION_SEC * self.sample_rate).round() as usize,
        );
        let wants = self.reverb_variation
            && matches!(self.reverb_model, ReverbModel::Booth | ReverbModel::Room);
        if wants {
            delayed
        } else {
            [l, r]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let x = if i == 0 { 1.0 } else { 0.0 };
                fx.process_reverb(x, x, 1.0 / 48_000.0)
            })
            .collect()
    }

    fn reverb(model: ReverbModel, variation: bool) -> LayerFxChain {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = model;
        fx.reverb_mix = 0.5;
        fx.reverb_variation = variation;
        fx
    }

    /// First sample index where the wet tail (dry part removed) exceeds `thr`.
    fn onset(ir: &[[f32; 2]], thr: f32) -> usize {
        ir.iter()
            .enumerate()
            .skip(1)
            .find(|(_, s)| s[0].abs() > thr)
            .map_or(usize::MAX, |(i, _)| i)
    }

    #[test]
    fn variation_off_is_repeatable_and_stage_ignores_the_flag() {
        for model in [
            ReverbModel::Booth,
            ReverbModel::Room,
            ReverbModel::Spring,
            ReverbModel::Stage,
        ] {
            let a = impulse_response(&mut reverb(model, false), 6000);
            let b = impulse_response(&mut reverb(model, false), 6000);
            assert_eq!(a, b);
            // Stage has no VARIATION: the flag changes nothing there at all.
            if model == ReverbModel::Stage {
                assert_eq!(a, impulse_response(&mut reverb(model, true), 6000));
            }
        }
    }

    #[test]
    fn booth_and_room_variation_delay_the_tail_by_the_predelay() {
        for model in [ReverbModel::Booth, ReverbModel::Room] {
            let plain = impulse_response(&mut reverb(model, false), 8000);
            let var = impulse_response(&mut reverb(model, true), 8000);
            let (p, v) = (onset(&plain, 1e-3), onset(&var, 1e-3));
            let pre = (PREDELAY_VARIATION_SEC * 48_000.0).round() as usize;
            assert!(
                v >= p + pre - 4 && v <= p + pre + 4,
                "{model:?}: onset {p} -> {v}, pre-delay {pre}"
            );
        }
    }

    #[test]
    fn spring_variation_lengthens_the_tail_and_adds_no_predelay() {
        let plain = impulse_response(&mut reverb(ReverbModel::Spring, false), 48_000);
        let var = impulse_response(&mut reverb(ReverbModel::Spring, true), 48_000);
        let tail = |ir: &[[f32; 2]]| ir[30_000..].iter().map(|s| s[0].abs()).fold(0.0, f32::max);
        assert!(tail(&var) > 2.0 * tail(&plain), "longer decay");
        assert_eq!(onset(&plain, 1e-3), onset(&var, 1e-3), "no pre-delay");
        assert!(var.iter().all(|s| s[0].is_finite() && s[1].is_finite()));
    }

    #[test]
    fn predelay_line_returns_the_input_after_the_delay_and_clamps() {
        let mut p = RevPredelay::new();
        let out: Vec<f32> = (0..8)
            .map(|i| p.process(if i == 0 { 1.0 } else { 0.0 }, 0.0, 3)[0])
            .collect();
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        // A zero or oversized delay never reads outside the line.
        p.process(0.5, 0.5, 0);
        p.process(0.5, 0.5, usize::MAX);
    }
}
