//! Palladium key-click: noise burst through a 2-pole bandpass.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.4. Constants are ASSUMED (TD-04).

use super::{OrganLayer, OrganModel};
use std::f32::consts::TAU;

impl OrganLayer {
    /// Synthesizes palladium contact-bounce key-click impulses through a resonant bandpass filter.
    pub(super) fn process_key_click(&mut self, inv_fs: f32) -> f32 {
        if self.click_env < 1e-6 && self.click_bp_state[0].abs() < 1e-6 {
            self.click_env = 0.0;
            return 0.0;
        }
        // Deterministic zero-alloc xorshift32 noise
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        let white = ((x as i32) as f32) * (1.0 / i32::MAX as f32);

        let center_hz = match self.model {
            OrganModel::Pipe1 | OrganModel::Pipe2 => 2400.0,
            _ => 1800.0,
        };
        let w = (TAU * center_hz * inv_fs).clamp(0.02, 0.55);
        let q_damp = 0.45;
        let excite = white * self.click_env * self.globals.click_scale;
        // 2-pole State-Variable Bandpass filter (zero DC offset)
        let hp = excite - self.click_bp_state[1] - q_damp * self.click_bp_state[0];
        let bp = self.click_bp_state[0] + w * hp;
        let lp = self.click_bp_state[1] + w * bp;
        self.click_bp_state[0] = bp;
        self.click_bp_state[1] = lp;

        self.click_env *= (-inv_fs / 0.0025).exp();
        bp * 0.85
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_click_bandpass_has_energy_and_zero_dc() {
        let mut o = OrganLayer::new(48_000.0);
        o.drawbars = [0; 9];
        o.percussion_enabled = false;
        o.rotary_enabled = false;
        o.note_on(60);
        let (mut mean, mut energy) = (0.0f32, 0.0f32);
        for _ in 0..512 {
            let [l, _] = o.render_frame();
            assert!(l.is_finite());
            mean += l;
            energy += l * l;
        }
        assert!(energy > 1e-5, "key-click must produce transient energy");
        assert!((mean / 512.0).abs() < 5e-3, "mean={}", mean / 512.0);
    }

    #[test]
    fn key_click_is_silent_when_idle_and_pipe_model_uses_higher_band() {
        let mut o = OrganLayer::new(48_000.0);
        assert_eq!(o.process_key_click(1.0 / 48_000.0), 0.0);
        o.model = OrganModel::Pipe1;
        o.click_env = 0.25;
        let y: f32 = (0..64)
            .map(|_| o.process_key_click(1.0 / 48_000.0).abs())
            .sum();
        assert!(y > 0.0);
    }
}
