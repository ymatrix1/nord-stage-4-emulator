//! Reverb stage (FDN + chorale) of the per-layer FX chain.

use super::*;

impl LayerFxChain {
    pub(super) fn process_reverb(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.reverb_model == ReverbModel::Off || self.reverb_mix <= 0.0 {
            return [l, r];
        }
        let (decay_gain, room_scale) = match self.reverb_model {
            ReverbModel::Off => (0.0, 1.0),
            ReverbModel::Booth => (0.52, 0.58),
            ReverbModel::Room => (0.72, 1.00),
            ReverbModel::Stage => (0.83, 1.48),
            ReverbModel::Spring => (0.77, 0.72),
            ReverbModel::Hall => (0.89, 2.05),
            ReverbModel::Cathedral => (0.935, 2.65),
        };
        let damp = match self.reverb_color {
            ReverbColor::Normal => 0.42,
            ReverbColor::Bright => 0.68,
            ReverbColor::Dark => 0.20,
            ReverbColor::Chorale => 0.48,
        };
        let (mod_rate_hz, mod_depth) = if self.reverb_color == ReverbColor::Chorale {
            (0.75, 8.0)
        } else {
            (0.42, 1.5)
        };
        self.reverb_chorale_phase = (self.reverb_chorale_phase + TAU * mod_rate_hz * inv_fs) % TAU;
        let mod_offset = (self.reverb_chorale_phase.sin() * mod_depth).round() as isize;
        let fs_scale = (self.sample_rate / 48_000.0).clamp(0.4, 4.0);

        let mut tap_outs = [0.0f32; FDN_SIZE];
        for (i, out) in tap_outs.iter_mut().enumerate() {
            let base_f = if self.reverb_model == ReverbModel::Spring {
                (FDN_DELAYS[i] as f32) * 0.52 + (127.0 + i as f32 * 43.0)
            } else {
                (FDN_DELAYS[i] as f32) * room_scale
            };
            let base_delay = (base_f * fs_scale).round() as isize;
            let sign = if i % 2 == 0 { mod_offset } else { -mod_offset };
            let delay = (base_delay + sign).clamp(64, (FDN_BUF_LEN - 8) as isize) as usize;
            let r_idx = (self.fdn_write_indices[i] + FDN_BUF_LEN - delay) % FDN_BUF_LEN;
            *out = self.fdn_bufs[i][r_idx];
        }

        // Hadamard 4x4 unitary feedback matrix (normalized by 0.5)
        let s0 = tap_outs[0] + tap_outs[1] + tap_outs[2] + tap_outs[3];
        let s1 = tap_outs[0] - tap_outs[1] + tap_outs[2] - tap_outs[3];
        let s2 = tap_outs[0] + tap_outs[1] - tap_outs[2] - tap_outs[3];
        let s3 = tap_outs[0] - tap_outs[1] - tap_outs[2] + tap_outs[3];
        let hadamard = [0.5 * s0, 0.5 * s1, 0.5 * s2, 0.5 * s3];

        // Two-stage stereo input allpass diffusion prevents transient flutter/metallic ringing
        const AP_G: f32 = 0.55;
        let mut dif_l = l;
        for (stage, &ap_delay) in REV_AP_DELAYS.iter().enumerate().take(2) {
            let w = self.rev_ap_indices[stage];
            let r_idx = (w + 512 - ap_delay) % 512;
            let delayed = self.rev_ap_bufs[stage][r_idx];
            let v = dif_l + delayed * AP_G;
            self.rev_ap_bufs[stage][w] = v;
            self.rev_ap_indices[stage] = (w + 1) % 512;
            dif_l = delayed - AP_G * v;
        }
        let mut dif_r = r;
        for (stage, &ap_delay) in REV_AP_DELAYS.iter().enumerate().skip(2) {
            let w = self.rev_ap_indices[stage];
            let r_idx = (w + 512 - ap_delay) % 512;
            let delayed = self.rev_ap_bufs[stage][r_idx];
            let v = dif_r + delayed * AP_G;
            self.rev_ap_bufs[stage][w] = v;
            self.rev_ap_indices[stage] = (w + 1) % 512;
            dif_r = delayed - AP_G * v;
        }

        let stereo_in = [
            0.70 * dif_l + 0.30 * dif_r,
            0.30 * dif_l + 0.70 * dif_r,
            0.85 * dif_l - 0.15 * dif_r,
            -0.15 * dif_l + 0.85 * dif_r,
        ];
        for (i, &had_val) in hadamard.iter().enumerate().take(FDN_SIZE) {
            let fb = had_val * decay_gain;
            self.fdn_lp_states[i] += damp * (fb - self.fdn_lp_states[i]);
            self.fdn_bufs[i][self.fdn_write_indices[i]] =
                stereo_in[i] * 0.55 + self.fdn_lp_states[i];
            self.fdn_write_indices[i] = (self.fdn_write_indices[i] + 1) % FDN_BUF_LEN;
        }

        let rev_l = 0.42 * (tap_outs[0] + 0.75 * tap_outs[2] + 0.25 * tap_outs[3]);
        let rev_r = 0.42 * (tap_outs[1] + 0.75 * tap_outs[3] + 0.25 * tap_outs[2]);
        let mix = self.reverb_mix.clamp(0.0, 1.0);
        if self.firmware_fx.reverb_mix {
            // `kRvbAmt` words: equal-power dry and send (opt-in, `fw_map::reverb_send`).
            let ui = fw_map::unit_to_ui(mix);
            let (dry, send) = (fw_map::reverb_dry(ui), fw_map::reverb_send(ui));
            return [l * dry + rev_l * send, r * dry + rev_r * send];
        }
        [
            l * (1.0 - 0.22 * mix) + rev_l * mix,
            r * (1.0 - 0.22 * mix) + rev_r * mix,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverb_tail_is_finite_after_an_impulse() {
        let mut fx = LayerFxChain::new(48_000.0);
        let mut peak = 0.0f32;
        for n in 0..4096 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let [l, r] = fx.process_reverb(x, x, 1.0 / 48_000.0);
            assert!(l.is_finite() && r.is_finite());
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak <= 4.0);
    }
}
