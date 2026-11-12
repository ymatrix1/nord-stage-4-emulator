//! `Mod 1` and `Mod 2` stages of the per-layer FX chain.

use super::*;

impl LayerFxChain {
    pub(super) fn process_mod1(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod1_type == Mod1Type::Off {
            return [l, r];
        }
        let lfo = self.mod1_phase.sin();
        self.mod1_phase = (self.mod1_phase + TAU * self.mod1_rate_hz * inv_fs) % TAU;
        let amt = self.mod1_amount.clamp(0.0, 1.0);
        match self.mod1_type {
            Mod1Type::Off => [l, r],
            Mod1Type::Tremolo => {
                let g = 1.0 - 0.5 * amt * (1.0 + lfo);
                [l * g, r * g]
            }
            Mod1Type::AutoPan => {
                let pan = 0.5 * amt * lfo;
                [l * (1.0 - pan), r * (1.0 + pan)]
            }
            Mod1Type::RingMod => [
                l * (1.0 - amt) + (l * lfo) * amt,
                r * (1.0 - amt) + (r * lfo) * amt,
            ],
            Mod1Type::Wah => {
                // Resonant 2-pole SVF bandpass swept between 350 Hz and 2.8 kHz
                let norm_sweep = 0.5 + 0.5 * lfo;
                let fc = 350.0 * 8.0f32.powf(norm_sweep);
                let w = (TAU * fc * inv_fs).clamp(0.02, 0.55);
                let q_damp = 0.35;

                let hp_l = l - self.wah_state_l[1] - q_damp * self.wah_state_l[0];
                let bp_l = self.wah_state_l[0] + w * hp_l;
                let lp_l = self.wah_state_l[1] + w * bp_l;
                self.wah_state_l = [bp_l, lp_l];

                let hp_r = r - self.wah_state_r[1] - q_damp * self.wah_state_r[0];
                let bp_r = self.wah_state_r[0] + w * hp_r;
                let lp_r = self.wah_state_r[1] + w * bp_r;
                self.wah_state_r = [bp_r, lp_r];

                [
                    l * (1.0 - amt) + bp_l * 1.4 * amt,
                    r * (1.0 - amt) + bp_r * 1.4 * amt,
                ]
            }
        }
    }

    pub(super) fn process_mod2(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod2_type == Mod2Type::Off {
            return [l, r];
        }
        let lfo_l = self.mod2_phase.sin();
        let lfo_r = self.mod2_phase.cos();
        self.mod2_phase = (self.mod2_phase + TAU * self.mod2_rate_hz * inv_fs) % TAU;
        let amt = self.mod2_amount.clamp(0.0, 1.0);

        match self.mod2_type {
            Mod2Type::Off => [l, r],
            Mod2Type::Chorus | Mod2Type::Vibe | Mod2Type::Flanger => {
                let (base_ms, depth_ms, fb_gain, wet_scale) = match self.mod2_type {
                    Mod2Type::Chorus => (9.0, 4.5 * amt, 0.0, 0.5),
                    Mod2Type::Vibe => (11.0, 6.0 * amt, 0.15 * amt, 0.65),
                    Mod2Type::Flanger => (2.8, 2.2 * amt, 0.45 * amt, 0.55),
                    _ => (0.0, 0.0, 0.0, 0.0),
                };
                let d_l = (base_ms + depth_ms * lfo_l) * 0.001 * self.sample_rate;
                let d_r = (base_ms + depth_ms * lfo_r) * 0.001 * self.sample_rate;
                let wet_l = read_ring_interpolated(&self.mod2_buf_l, self.mod2_write_idx, d_l);
                let wet_r = read_ring_interpolated(&self.mod2_buf_r, self.mod2_write_idx, d_r);
                self.mod2_buf_l[self.mod2_write_idx] = (l + wet_l * fb_gain).clamp(-2.0, 2.0);
                self.mod2_buf_r[self.mod2_write_idx] = (r + wet_r * fb_gain).clamp(-2.0, 2.0);
                self.mod2_write_idx = (self.mod2_write_idx + 1) % MOD2_LINE_LEN;

                let dry_mix = 1.0 - wet_scale * amt;
                let wet_mix = wet_scale * amt;
                [l * dry_mix + wet_l * wet_mix, r * dry_mix + wet_r * wet_mix]
            }
            Mod2Type::Phaser => {
                // 4-stage cascaded 1st-order all-pass phaser with stereo LFO sweep
                let freq_l = 320.0 * 9.0f32.powf(0.5 + 0.5 * lfo_l);
                let freq_r = 320.0 * 9.0f32.powf(0.5 + 0.5 * lfo_r);
                let coeff_l = ((1.0 - TAU * freq_l * inv_fs) / (1.0 + TAU * freq_l * inv_fs))
                    .clamp(-0.95, 0.95);
                let coeff_r = ((1.0 - TAU * freq_r * inv_fs) / (1.0 + TAU * freq_r * inv_fs))
                    .clamp(-0.95, 0.95);

                let mut x_l = l + self.phaser_fb[0] * 0.35 * amt;
                for state in &mut self.phaser_ap_l {
                    let y = -coeff_l * x_l + *state;
                    *state = x_l + coeff_l * y;
                    x_l = y;
                }
                self.phaser_fb[0] = x_l.clamp(-2.0, 2.0);

                let mut x_r = r + self.phaser_fb[1] * 0.35 * amt;
                for state in &mut self.phaser_ap_r {
                    let y = -coeff_r * x_r + *state;
                    *state = x_r + coeff_r * y;
                    x_r = y;
                }
                self.phaser_fb[1] = x_r.clamp(-2.0, 2.0);

                [
                    l * (1.0 - 0.5 * amt) + x_l * 0.5 * amt,
                    r * (1.0 - 0.5 * amt) + x_r * 0.5 * amt,
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_stages_pass_silence_as_silence() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.mod1_type = Mod1Type::AutoPan;
        fx.mod2_type = Mod2Type::Chorus;
        for _ in 0..256 {
            assert_eq!(fx.process_frame([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
