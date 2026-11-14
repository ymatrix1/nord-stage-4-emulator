//! `Mod 1` and `Mod 2` stages of the per-layer FX chain.

use super::*;

/// Soft-clipped sine `tanh(k x) / tanh(k)`: approaches a square wave as `k` grows (unity peak).
#[inline]
fn soft_square(x: f32, k: f32) -> f32 {
    (k * x).tanh() / k.tanh()
}

impl LayerFxChain {
    /// Mod 1 LFO rate: Master Clock (`mod1_sync_beats`) for A-Pan / Trem / Wah / Pump, else the Rate knob.
    #[inline]
    pub(super) fn mod1_effective_rate_hz(&self) -> f32 {
        match (self.mod1_sync_beats, self.mod1_type) {
            (Some(beats), t) if t != Mod1Type::RingMod && beats > 0.0 => {
                self.clock_bpm / 60.0 / beats
            }
            _ => self.mod1_rate_hz,
        }
    }

    /// The firmware A-PAN / TREM stage (`FirmwareFx::mod1`): `None` when this program is not one
    /// of `kFx1Type` 0..=3 (types 4..=11 go to `process_fw_mod1_rest`), runs in Master Clock sync, or has no raw values (engine path).
    fn process_fw_mod1(&mut self, l: f32, r: f32) -> Option<[f32; 2]> {
        let (ty, rate) = (self.mod1_type_raw?, self.mod1_rate_raw?);
        if ty > 11 || self.mod1_sync_beats.is_some() {
            self.fw_mod1 = None;
            self.fw_mod1_rest = None;
            return None;
        }
        if ty > 3 {
            return self.process_fw_mod1_rest(ty, u32::from(rate), l, r);
        }
        self.fw_mod1_rest = None;
        let shape = if ty < 2 {
            fw_mod1::Mod1Shape::AutoPan
        } else {
            fw_mod1::Mod1Shape::Tremolo
        };
        let hard = ty & 1 == 1;
        let amount = (self.mod1_amount.clamp(0.0, 1.0) * 127.0).round() as u32;
        let rate = u32::from(rate);
        match &mut self.fw_mod1 {
            Some(st) if st.shape() == shape && st.hard() == hard => st.set_params(rate, amount),
            slot => {
                *slot = Some(fw_mod1::FwMod1Stage::new(
                    shape,
                    hard,
                    rate,
                    amount,
                    self.sample_rate,
                ));
            }
        }
        self.fw_mod1.as_mut().map(|st| st.process_frame([l, r]))
    }

    /// The firmware RM / A-WAH / WAH / PUMP stages (`kFx1Type` 4 to 11). The PED variation of the
    /// wah (type 9) takes its position from the pedal in place of the rate (ASSUMED: the ARM
    /// writes the pedal through the rate id, the type 9 law of `c[-16]` is `v / 128`).
    fn process_fw_mod1_rest(&mut self, ty: u8, rate: u32, l: f32, r: f32) -> Option<[f32; 2]> {
        self.fw_mod1 = None;
        let amount = (self.mod1_amount.clamp(0.0, 1.0) * 127.0).round() as u32;
        let rate = if ty == 9 && self.mod1_ped {
            (self.mod1_ped_pos.clamp(0.0, 1.0) * 127.0).round() as u32
        } else {
            rate
        };
        match &mut self.fw_mod1_rest {
            Some(st) if st.ty() == ty => st.set_params(rate, amount),
            slot => {
                *slot =
                    fw_mod1_rest::FwMod1Rest::new(ty, rate, amount, self.sample_rate).map(Box::new);
            }
        }
        self.fw_mod1_rest
            .as_mut()
            .map(|st| st.process_frame([l, r]))
    }

    pub(super) fn process_mod1(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod1_type == Mod1Type::Off {
            return [l, r];
        }
        if self.firmware_fx.mod1 {
            if let Some(out) = self.process_fw_mod1(l, r) {
                return out;
            }
        }
        // Appended types with their own state (dynamics driven, no shared LFO phase).
        match self.mod1_type {
            Mod1Type::AutoWah => return self.process_awah(l, r, inv_fs),
            Mod1Type::Pump => return self.process_pump(l, r, inv_fs),
            _ => {}
        }
        let phase = self.mod1_phase;
        let lfo = phase.sin();
        self.mod1_phase = (self.mod1_phase + TAU * self.mod1_effective_rate_hz() * inv_fs) % TAU;
        let amt = self.mod1_amount.clamp(0.0, 1.0);
        let var = self.mod1_variation;
        match self.mod1_type {
            Mod1Type::Off | Mod1Type::AutoWah | Mod1Type::Pump => [l, r],
            Mod1Type::Tremolo => {
                // VARIATION: square wave shape (manual p.49), soft-clipped sine (ASSUMED k = 6).
                let w = if var { soft_square(lfo, 6.0) } else { lfo };
                let g = 1.0 - 0.5 * amt * (1.0 + w);
                [l * g, r * g]
            }
            Mod1Type::AutoPan => {
                // VARIATION: harder pan waveform (manual p.49), ASSUMED k = 3.
                let w = if var { soft_square(lfo, 3.0) } else { lfo };
                let pan = 0.5 * amt * w;
                [l * (1.0 - pan), r * (1.0 + pan)]
            }
            Mod1Type::RingMod => {
                // VARIATION: stereo ring mod (manual p.49): the right carrier is 90 degrees ahead.
                let lfo_r = if var { phase.cos() } else { lfo };
                [
                    l * (1.0 - amt) + (l * lfo) * amt,
                    r * (1.0 - amt) + (r * lfo_r) * amt,
                ]
            }
            Mod1Type::Wah => {
                // Resonant 2-pole SVF bandpass swept between 350 Hz and 2.8 kHz
                let norm_sweep = if self.mod1_ped {
                    self.mod1_ped_pos.clamp(0.0, 1.0)
                } else {
                    0.5 + 0.5 * lfo
                };
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

    /// The firmware Mod 2 stages (`FirmwareFx::mod2`): `None` unless the program is `kFx2Type` 0
    /// to 11 with its raw values (engine path otherwise). Types 0 to 3 and 8 to 11 run through
    /// the law stage (`PHAS`, `FLANG`, `ENS`, `SPIN`), 4 and 5 through the chorus, 6 and 7 through
    /// `VIBE`; only one stage is kept alive.
    fn process_fw_mod2(&mut self, l: f32, r: f32) -> Option<[f32; 2]> {
        let (ty, rate) = (self.mod2_type_raw?, self.mod2_rate_raw?);
        let amount = (self.mod2_amount.clamp(0.0, 1.0) * 127.0).round() as u32;
        let rate = u32::from(rate);
        if (6..=7).contains(&ty) {
            self.fw_mod2 = None;
            self.fw_law = None;
            match &mut self.fw_vibe {
                Some(st) => st.set_params(rate, amount),
                slot => {
                    *slot = Some(Box::new(fw_vibe_stage::FwVibeStage::new(
                        rate,
                        amount,
                        self.sample_rate,
                    )));
                }
            }
            return self.fw_vibe.as_mut().map(|st| st.process_frame([l, r]));
        }
        self.fw_vibe = None;
        if !(4..=5).contains(&ty) {
            self.fw_mod2 = None;
            match &mut self.fw_law {
                Some(st) if st.ty() == ty => st.set_params(rate, amount),
                slot => {
                    *slot = fw_mod2_law_stage::FwLawStage::new(ty, rate, amount, self.sample_rate)
                        .map(Box::new);
                }
            }
            return self.fw_law.as_mut().map(|st| st.process_frame([l, r]));
        }
        self.fw_law = None;
        let variation = ty == 5;
        match &mut self.fw_mod2 {
            Some(st) if st.variation() == variation => st.set_params(rate, amount),
            slot => {
                *slot = Some(Box::new(fw_mod2::FwMod2Stage::new(
                    variation,
                    rate,
                    amount,
                    self.sample_rate,
                )));
            }
        }
        self.fw_mod2.as_mut().map(|st| st.process_frame([l, r]))
    }

    pub(super) fn process_mod2(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod2_type == Mod2Type::Off {
            return [l, r];
        }
        if self.firmware_fx.mod2 {
            if let Some(out) = self.process_fw_mod2(l, r) {
                return out;
            }
        }
        match self.mod2_type {
            Mod2Type::Spin => return self.process_spin(l, r, inv_fs),
            Mod2Type::Ensemble => return self.process_ensemble(l, r, inv_fs),
            _ => {}
        }
        let lfo_l = self.mod2_phase.sin();
        let lfo_r = self.mod2_phase.cos();
        self.mod2_phase = (self.mod2_phase + TAU * self.mod2_rate_hz * inv_fs) % TAU;
        let amt = self.mod2_amount.clamp(0.0, 1.0);
        let var = self.mod2_variation;
        // VARIATION of Flanger / Vibe is the mono version (manual p.50): both channels share one LFO.
        let lfo_r = if var && matches!(self.mod2_type, Mod2Type::Flanger | Mod2Type::Vibe) {
            lfo_l
        } else {
            lfo_r
        };

        match self.mod2_type {
            Mod2Type::Off | Mod2Type::Spin | Mod2Type::Ensemble => [l, r],
            Mod2Type::Chorus | Mod2Type::Vibe | Mod2Type::Flanger => {
                let (base_ms, depth_ms, fb_gain, wet_scale) = match self.mod2_type {
                    // VARIATION: vintage pedal chorus, wider and deeper (manual p.50), ASSUMED values.
                    Mod2Type::Chorus if var => (7.0, 6.5 * amt, 0.0, 0.6),
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

                // VARIATION: Bi-Phase character (manual p.50): ASSUMED stronger feedback, zero at minimum.
                let fb = if var { 0.6 } else { 0.35 };
                let mut x_l = l + self.phaser_fb[0] * fb * amt;
                for state in &mut self.phaser_ap_l {
                    let y = -coeff_l * x_l + *state;
                    *state = x_l + coeff_l * y;
                    x_l = y;
                }
                self.phaser_fb[0] = x_l.clamp(-2.0, 2.0);

                let mut x_r = r + self.phaser_fb[1] * fb * amt;
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

    fn run(configure: impl Fn(&mut LayerFxChain)) -> Vec<[f32; 2]> {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = ReverbModel::Off;
        fx.mod1_amount = 0.8;
        fx.mod2_amount = 0.8;
        fx.mod1_rate_hz = 5.0;
        fx.mod2_rate_hz = 2.0;
        configure(&mut fx);
        (0..9600)
            .map(|n| {
                let x = (n as f32 * 0.05).sin() * 0.5;
                fx.process_frame([x, x * 0.8])
            })
            .collect()
    }

    #[test]
    fn mod1_variation_changes_every_type_and_stays_finite() {
        for t in [Mod1Type::Tremolo, Mod1Type::AutoPan, Mod1Type::RingMod] {
            let base = run(|fx| fx.mod1_type = t);
            let var = run(|fx| {
                fx.mod1_type = t;
                fx.mod1_variation = true;
            });
            assert!(var.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
            assert!(base != var, "{t:?} variation has no effect");
        }
    }

    #[test]
    fn mod2_variation_changes_chorus_flanger_vibe_and_phaser() {
        for t in [
            Mod2Type::Chorus,
            Mod2Type::Flanger,
            Mod2Type::Vibe,
            Mod2Type::Phaser,
        ] {
            let base = run(|fx| fx.mod2_type = t);
            let var = run(|fx| {
                fx.mod2_type = t;
                fx.mod2_variation = true;
            });
            assert!(var.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
            assert!(base != var, "{t:?} variation has no effect");
        }
    }

    #[test]
    fn mono_flanger_variation_gives_identical_lfo_in_both_channels() {
        let out = run(|fx| {
            fx.mod2_type = Mod2Type::Flanger;
            fx.mod2_variation = true;
        });
        // Left and right inputs differ only by gain 0.8; with a shared LFO the ratio stays 0.8.
        let ok = out
            .iter()
            .skip(100)
            .all(|f| (f[1] - 0.8 * f[0]).abs() < 1e-4);
        assert!(ok, "variation flanger is not mono");
    }

    #[test]
    fn wah_ped_position_moves_the_filter_and_ignores_the_lfo() {
        let at = |pos: f32| {
            run(|fx| {
                fx.mod1_type = Mod1Type::Wah;
                fx.mod1_ped = true;
                fx.mod1_ped_pos = pos;
            })
        };
        assert!(at(0.1) != at(0.9), "pedal position has no effect");
        let free = run(|fx| fx.mod1_type = Mod1Type::Wah);
        assert!(free != at(0.5));
    }
}
