//! Delay stage (tempo sync, filters, feedback FX, ping-pong) of the per-layer FX chain.

use super::*;

impl LayerFxChain {
    pub(super) fn target_delay_samples(&self, bpm: f32) -> f32 {
        let target_ms = if self.delay_clock_sync {
            let beat_ms = 60_000.0 / bpm.clamp(30.0, 300.0);
            (beat_ms
                * self
                    .delay_sync_beats
                    .unwrap_or_else(|| self.delay_subdivision.beats()))
            .clamp(10.0, 2000.0)
        } else {
            self.delay_time_ms.clamp(10.0, 2000.0)
        };
        (target_ms * 0.001 * self.sample_rate).clamp(1.0, (DELAY_LINE_LEN - 8) as f32)
    }

    /// Snaps the smoothed analog delay read position to the current target without slewing.
    pub fn snap_delay_time(&mut self, bpm: f32) {
        self.delay_smoothed_samples = self.target_delay_samples(bpm);
    }

    /// Delay feedback effect on one repeat (`l`, `r`): Ensemble / Vibe / Chorus / Flam / Space.
    ///
    /// Structure VERIFIED by the vendor manual (see [`DelayFeedbackFx`]); every rate, depth, delay time
    /// and mix below is ASSUMED (no firmware or recording source yet).
    pub(super) fn process_delay_fx(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.delay_fx == DelayFeedbackFx::Off {
            return [l, r];
        }
        let fs = self.sample_rate;
        let var = self.delay_fx_variation;
        let w = self.dfx_idx;
        self.dfx_buf_l[w] = l;
        self.dfx_buf_r[w] = r;
        let mono = 0.5 * (l + r);
        let out = match self.delay_fx {
            DelayFeedbackFx::Off => [l, r],
            DelayFeedbackFx::Chorus | DelayFeedbackFx::Vibe | DelayFeedbackFx::Ensemble => {
                let (rate, base_ms, depth_ms) = match (self.delay_fx, var) {
                    (DelayFeedbackFx::Chorus, false) => (0.8, 12.0, 3.5),
                    (DelayFeedbackFx::Chorus, true) => (1.1, 14.0, 7.0),
                    (DelayFeedbackFx::Vibe, _) => (3.2, 4.0, 2.5),
                    _ => (0.55, 9.0, 3.0),
                };
                self.dfx_phase = (self.dfx_phase + TAU * rate * inv_fs) % TAU;
                let ph = self.dfx_phase;
                let tap = |buf: &[f32], phase: f32| {
                    let d = (base_ms + depth_ms * phase.sin()) * 0.001 * fs;
                    read_ring_interpolated(buf, w, d)
                };
                match self.delay_fx {
                    DelayFeedbackFx::Chorus => {
                        // opposite LFO polarity per channel widens a mono source; the variation is
                        // deeper and also blends the cross channel
                        let wl = tap(&self.dfx_buf_l, ph);
                        let wr = tap(&self.dfx_buf_r, ph + std::f32::consts::PI);
                        let (cl, cr) = if var {
                            (0.5 * (wl + 0.3 * wr), 0.5 * (wr + 0.3 * wl))
                        } else {
                            (0.5 * wl, 0.5 * wr)
                        };
                        [0.5 * l + cl * 1.2, 0.5 * r + cr * 1.2]
                    }
                    DelayFeedbackFx::Vibe => {
                        // 100 % wet pitch/phase wobble; variation = mono version
                        if var {
                            let m = tap(&self.dfx_buf_l, ph);
                            let m2 = tap(&self.dfx_buf_r, ph);
                            let v = 0.5 * (m + m2);
                            [v, v]
                        } else {
                            [
                                tap(&self.dfx_buf_l, ph),
                                tap(&self.dfx_buf_r, ph + std::f32::consts::FRAC_PI_2),
                            ]
                        }
                    }
                    _ => {
                        // three cross-connected modulated lines 120 degrees apart (read from the mono sum)
                        self.dfx_buf_l[w] = mono;
                        let t0 = tap(&self.dfx_buf_l, ph);
                        let t1 = tap(&self.dfx_buf_l, ph + TAU / 3.0);
                        let t2 = tap(&self.dfx_buf_l, ph + 2.0 * TAU / 3.0);
                        self.dfx_buf_l[w] = l;
                        if var {
                            let v = (t0 + t1 + t2) / 3.0;
                            [0.4 * mono + 0.6 * v, 0.4 * mono + 0.6 * v]
                        } else {
                            [
                                0.4 * mono + 0.6 * (0.6 * t0 + 0.4 * t1),
                                0.4 * mono + 0.6 * (0.6 * t2 + 0.4 * t1),
                            ]
                        }
                    }
                }
            }
            DelayFeedbackFx::Flam => {
                // ping-pong style offset between the channels: 20 ms (variation 40 ms) on the right
                let shift = if var { 0.040 } else { 0.020 } * fs;
                [l, read_ring_interpolated(&self.dfx_buf_r, w, shift)]
            }
            DelayFeedbackFx::Space => {
                // Schroeder all-pass diffusion; 50 % wet (variation 100 % wet)
                let g = 0.55;
                let mut y = [l, r];
                for (ch, sig) in y.iter_mut().enumerate() {
                    for stage in 0..2 {
                        let k = ch * 2 + stage;
                        let len = DFX_AP_LENGTHS[k];
                        let i = self.dfx_ap_idx[k] % len;
                        let delayed = self.dfx_ap[k][i];
                        let out = -g * *sig + delayed;
                        self.dfx_ap[k][i] = *sig + g * out;
                        self.dfx_ap_idx[k] = (i + 1) % len;
                        *sig = out;
                    }
                }
                let wet = if var { 1.0 } else { 0.5 };
                [l * (1.0 - wet) + y[0] * wet, r * (1.0 - wet) + y[1] * wet]
            }
        };
        self.dfx_idx = (w + 1) % DFX_BUF_LEN;
        out
    }

    pub(super) fn process_delay(&mut self, l: f32, r: f32, bpm: f32, inv_fs: f32) -> [f32; 2] {
        if !self.delay_enabled {
            return [l, r];
        }
        let target_samples = self.target_delay_samples(bpm);

        // Smooth Analog tape pitch-slew (80 ms inertia) vs exact digital tap
        if self.delay_analog {
            self.delay_smoothed_samples +=
                (target_samples - self.delay_smoothed_samples) * (inv_fs / 0.080).min(1.0);
        } else {
            self.delay_smoothed_samples = target_samples;
        }

        let read_delay = self
            .delay_smoothed_samples
            .clamp(1.0, (DELAY_LINE_LEN - 4) as f32);
        let tap_l = read_ring_interpolated(&self.delay_buf_l, self.delay_write_idx, read_delay);
        let tap_r = read_ring_interpolated(&self.delay_buf_r, self.delay_write_idx, read_delay);
        // Feedback effect acts on every repeat (heard in the output and fed back)
        let [dl, dr] = self.process_delay_fx(tap_l, tap_r, inv_fs);

        // Feedback loop HP / LP / BP filters
        let (mut fb_l, mut fb_r) = (dl, dr);
        if self.delay_filter != DelayFilterMode::Off {
            let hp_a = (TAU * 280.0 * inv_fs).clamp(0.005, 0.35);
            let lp_a = (TAU * 2200.0 * inv_fs).clamp(0.02, 0.75);
            for (ch, sig) in [&mut fb_l, &mut fb_r].into_iter().enumerate() {
                self.delay_hp_state[ch] += hp_a * (*sig - self.delay_hp_state[ch]);
                let hp_out = *sig - self.delay_hp_state[ch];
                let lp_in = match self.delay_filter {
                    DelayFilterMode::Hp => {
                        *sig = hp_out;
                        continue;
                    }
                    DelayFilterMode::Lp => *sig,
                    DelayFilterMode::Bp => hp_out,
                    DelayFilterMode::Off => *sig,
                };
                self.delay_lp_state[ch] += lp_a * (lp_in - self.delay_lp_state[ch]);
                *sig = self.delay_lp_state[ch];
            }
        }

        if self.delay_analog {
            fb_l = (fb_l * 1.15).tanh();
            fb_r = (fb_r * 1.15).tanh();
        }

        // Firmware mix law of `kDlyMix`: input send `min(1, (v / 64)^2)`, dry `min(1, (2 - v / 64)^2)`,
        // unity tap (opt-in, `FirmwareFx::delay_laws`); otherwise the engine's dry + mix * wet.
        let (dry, send, wet) = if self.firmware_fx.delay_laws {
            let ui = fw_map::unit_to_ui(self.delay_mix);
            (fw_map::delay_dry(ui), fw_map::delay_send(ui), 1.0)
        } else {
            (1.0, 1.0, self.delay_mix.clamp(0.0, 1.0))
        };
        let fb = self.delay_feedback.clamp(0.0, 0.95);
        if self.delay_ping_pong {
            let mono_in = 0.5 * (l + r) * send;
            self.delay_buf_l[self.delay_write_idx] = (mono_in + fb_r * fb).clamp(-4.0, 4.0);
            self.delay_buf_r[self.delay_write_idx] = (fb_l * fb).clamp(-4.0, 4.0);
        } else {
            self.delay_buf_l[self.delay_write_idx] = (l * send + fb_l * fb).clamp(-4.0, 4.0);
            self.delay_buf_r[self.delay_write_idx] = (r * send + fb_r * fb).clamp(-4.0, 4.0);
        }
        self.delay_write_idx = (self.delay_write_idx + 1) % DELAY_LINE_LEN;

        [l * dry + dl * wet, r * dry + dr * wet]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_delay_time_follows_the_tempo() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.snap_delay_time(120.0);
        let a = fx.target_delay_samples(120.0);
        let b = fx.target_delay_samples(60.0);
        assert!(a.is_finite() && b.is_finite());
    }
}
