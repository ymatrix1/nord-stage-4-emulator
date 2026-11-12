//! Per-sample rendering of `SynthLayer`: LFO, envelopes, oscillators, filter and mix.

use super::*;

impl SynthLayer {
    /// Renders a single stereo frame `[left, right]` across all 16 polyphonic voices.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;

        // 1. Advance LFO
        self.lfo_phase = (self.lfo_phase + TAU * self.lfo_rate_hz * inv_fs) % TAU;
        let lfo_val = match self.lfo_shape {
            LfoShape::Triangle => 1.0 - 4.0 * ((self.lfo_phase / TAU) - 0.5).abs(),
            LfoShape::Saw => 2.0 * (self.lfo_phase / TAU) - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * (self.lfo_phase / TAU),
            LfoShape::Square => {
                if self.lfo_phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => {
                if self.lfo_phase < (TAU * self.lfo_rate_hz * inv_fs) {
                    self.lfo_sh_val = ((self.lfo_phase * 1234.56).sin() * 43_758.547).fract();
                }
                self.lfo_sh_val
            }
        };

        // 2. Advance Arpeggiator
        self.tick_arpeggiator(inv_fs);

        // 3. Render and sum polyphonic voices
        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        let eff_cutoff = match self.lfo_dest {
            LfoDestination::FilterCutoff => (self.cutoff_hz
                * (1.0 + self.lfo_amount * lfo_val * 0.8))
                .clamp(14.0, 0.48 * self.sample_rate),
            _ => self.cutoff_hz,
        };

        let pitch_mod = match self.lfo_dest {
            // VERIFIED depth curve (firmware table, see `synth_tables.rs`): 0..48 semitones.
            LfoDestination::OscPitch => {
                2.0f32.powf(lfo_pitch_semitones(self.lfo_amount) * lfo_val / 12.0)
            }
            _ => 1.0,
        };

        let bank = wavetable_bank();
        let sample_sound = self.sample_sound.as_deref();
        let wt_pos_base = if self.lfo_dest == LfoDestination::OscCtrl {
            (self.wavetable_position + self.lfo_amount * lfo_val * 0.5).clamp(0.0, 1.0)
        } else {
            self.wavetable_position.clamp(0.0, 1.0)
        };
        let fm_ratio = self.fm_ratio.clamp(0.25, 16.0);
        let fm_sub_ratio = self.fm_sub_ratio.clamp(0.25, 16.0);
        let fm_fb_amt = self.fm_feedback.clamp(0.0, 1.0);
        let fm_sub_amt = self.fm_sub_amount.clamp(0.0, 1.0);
        let fm_env_amt = self.fm_env_amount.clamp(0.0, 1.0);
        let mod_decay_tau = self.decay_sec.max(0.015);
        let mod_decay_coeff = (-inv_fs / mod_decay_tau).exp();
        // Constant-rate glide: semitones travelled per sample (see `glide_sec`).
        let glide_step_semi = if self.glide_sec > 0.0 {
            12.0 * inv_fs / self.glide_sec.max(0.001)
        } else {
            0.0
        };
        self.vib_phase =
            (self.vib_phase + TAU * self.vibrato_rate_hz.clamp(0.1, 20.0) * inv_fs) % TAU;
        let vib_sin = self.vib_phase.sin();
        let uni_level = usize::from(self.unison.min(3));
        let uni_count = UNISON_VOICE_COUNT[uni_level];
        // Per voice gains: constant power over the partner voices, measured weak-channel leakage.
        let uni_voice_gain = if uni_count > 0 {
            1.0 / (uni_count as f32).sqrt()
        } else {
            0.0
        };
        let weak = UNISON_WEAK_CHANNEL_GAIN;
        let pan_norm = 1.0 / (1.0 + weak * weak).sqrt();

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            // Envelope advance (OS v1.66 hardware floors: 0.5 ms attack, 3.0 ms decay/release, >=44 s decay = Hold)
            if v.gate {
                if v.amp_env < 0.999 {
                    let atk = self.attack_sec.max(0.0005);
                    v.amp_env += (1.0 - v.amp_env) * (inv_fs / atk).min(1.0);
                } else if self.decay_sec < 44.0 {
                    let dec = self.decay_sec.max(0.003);
                    v.amp_env += (self.sustain_level - v.amp_env) * (inv_fs / dec).min(1.0);
                }
            } else {
                let rel = self.release_sec.max(0.003);
                v.amp_env *= (-inv_fs / rel).exp();
                if v.amp_env < 1e-4 {
                    v.amp_env = 0.0;
                    v.active = false;
                    continue;
                }
            }
            v.mod_env *= mod_decay_coeff;

            if glide_step_semi > 0.0 && v.freq_hz != v.target_freq_hz {
                let d = 12.0 * (v.target_freq_hz / v.freq_hz.max(1.0)).log2();
                if d.abs() <= glide_step_semi {
                    v.freq_hz = v.target_freq_hz;
                } else {
                    v.freq_hz *= 2.0f32.powf(d.signum() * glide_step_semi / 12.0);
                }
            } else {
                v.freq_hz = v.target_freq_hz;
            }
            v.age_sec += inv_fs;

            // Oscillator envelope (ADR); `osc_env_velocity` scales its depth by velocity.
            if v.gate {
                if !v.oenv_attack_done {
                    let atk = self.osc_env_attack_sec.max(0.0005);
                    v.oenv += (1.0 - v.oenv) * (inv_fs / atk).min(1.0);
                    if v.oenv >= 0.999 {
                        v.oenv = 1.0;
                        v.oenv_attack_done = true;
                    }
                } else if self.osc_env_decay_sec < 44.0 {
                    let dec = (self.osc_env_decay_sec * ENV_DECAY_TAU_FRACTION).max(0.003);
                    v.oenv -= v.oenv * (inv_fs / dec).min(1.0);
                }
            } else {
                let rel = self.osc_env_release_sec.max(0.003);
                v.oenv *= (-inv_fs / rel).exp();
            }
            let oenv_depth = if self.osc_env_velocity {
                self.osc_env_amount * v.velocity
            } else {
                self.osc_env_amount
            };
            let osc_env_mod = oenv_depth * v.oenv;
            let osc_ctrl_mod = if self.osc_env_to_pitch {
                0.0
            } else {
                osc_env_mod
            };
            // EMPIRICAL: +/-12 semitones of pitch at full Env Amt (Env To Pitch); Audible Genius "pitch
            // blip" clip, Env Amt +5.5 shows ~6-7 semitone onset peaks (+6.6 predicted), accuracy
            // limited to ~1 semitone by 5 ms onset detection. ASSUMED: +/-0.5 of Osc Ctrl range.
            let env_pitch = if self.osc_env_to_pitch {
                2.0f32.powf(osc_env_mod)
            } else {
                1.0
            };
            let wt_pos = (wt_pos_base + osc_ctrl_mod * 0.5).clamp(0.0, 1.0);
            let vib_amount = match self.vibrato_mode {
                VibratoMode::Off => 0.0,
                VibratoMode::On => 1.0,
                // EMPIRICAL: onset at the displayed delay, full depth ~0.45 s later (hardware clip,
                // Delay 0.7 s: onset 0.74 s, full depth 1.15 s).
                VibratoMode::Delay => {
                    ((v.age_sec - self.vibrato_delay_sec) / VIBRATO_DELAY_RAMP_SEC).clamp(0.0, 1.0)
                }
                VibratoMode::Wheel | VibratoMode::Aftertouch | VibratoMode::Pedal => {
                    self.vibrato_ctrl.clamp(0.0, 1.0)
                }
            };
            let vib_pitch = 2.0f32.powf(vib_amount * self.vibrato_depth_cents * vib_sin / 1200.0);

            // Filter envelope: dedicated ADR, or mirror the amp decay (legacy).
            let fenv_peak = 1.0 - self.filter_env_vel_amount.clamp(0.0, 1.0) * (1.0 - v.velocity);
            let filter_env_level = if self.filter_env_dedicated {
                if v.gate {
                    if !v.fenv_attack_done {
                        let atk = self.filter_env_attack_sec.max(0.0005);
                        v.fenv += (1.0 - v.fenv) * (inv_fs / atk).min(1.0);
                        if v.fenv >= 0.999 {
                            v.fenv = 1.0;
                            v.fenv_attack_done = true;
                        }
                    } else if self.filter_env_decay_sec < 44.0 {
                        let dec = (self.filter_env_decay_sec * ENV_DECAY_TAU_FRACTION).max(0.003);
                        v.fenv -= v.fenv * (inv_fs / dec).min(1.0);
                    }
                } else {
                    let rel = self.filter_env_release_sec.max(0.003);
                    v.fenv *= (-inv_fs / rel).exp();
                }
                v.fenv * fenv_peak
            } else {
                v.mod_env * fenv_peak
            };

            let eff_freq = v.freq_hz * pitch_mod * env_pitch * vib_pitch;
            let dt = (eff_freq * inv_fs).clamp(1e-5, 0.49);
            let t = v.phase;

            let (raw_osc_l, raw_osc_r) = match self.osc_mode {
                SynthOscMode::Analog => {
                    if uni_count > 0 {
                        let mut acc = (0.0f32, 0.0f32);
                        for (k, voice) in UNISON_VOICES[uni_level][..uni_count]
                            .iter()
                            .copied()
                            .enumerate()
                        {
                            let f_k = eff_freq * voice.ratio;
                            let dt_k = (f_k * inv_fs).clamp(1e-5, 0.49);
                            let t_k = v.uni_phase[k];
                            let s_k = Self::render_va(
                                self.waveform,
                                self.supersaw_detune,
                                self.pulse_width,
                                self.lfo_dest,
                                self.lfo_amount,
                                lfo_val,
                                t_k,
                                dt_k,
                                f_k,
                                inv_fs,
                                &mut v.uni_supersaw[k],
                            );
                            v.uni_phase[k] = (t_k + dt_k) % 1.0;
                            let (gl, gr) = unison_pan(voice.strong_left, weak, pan_norm);
                            acc.0 += s_k * uni_voice_gain * gl;
                            acc.1 += s_k * uni_voice_gain * gr;
                        }
                        (acc.0, Some(acc.1))
                    } else {
                        (
                            Self::render_va(
                                self.waveform,
                                self.supersaw_detune,
                                self.pulse_width,
                                self.lfo_dest,
                                self.lfo_amount,
                                lfo_val,
                                t,
                                dt,
                                eff_freq,
                                inv_fs,
                                &mut v.supersaw_phases,
                            ),
                            None,
                        )
                    }
                }
                SynthOscMode::Wavetable => {
                    if uni_count > 0 {
                        let mut acc = (0.0f32, 0.0f32);
                        for (k, voice) in UNISON_VOICES[uni_level][..uni_count]
                            .iter()
                            .copied()
                            .enumerate()
                        {
                            let dt_k = (eff_freq * voice.ratio * inv_fs).clamp(1e-5, 0.49);
                            let t_k = v.uni_phase[k];
                            let s_k =
                                sample_wavetable(bank, self.wavetable_index, wt_pos, t_k, dt_k);
                            v.uni_phase[k] = (t_k + dt_k) % 1.0;
                            let (gl, gr) = unison_pan(voice.strong_left, weak, pan_norm);
                            acc.0 += s_k * uni_voice_gain * gl;
                            acc.1 += s_k * uni_voice_gain * gr;
                        }
                        (acc.0, Some(acc.1))
                    } else {
                        (
                            sample_wavetable(bank, self.wavetable_index, wt_pos, t, dt),
                            None,
                        )
                    }
                }
                SynthOscMode::Fm => {
                    let op3 = (TAU * v.op3_phase).sin();
                    let fb = fm_fb_amt * 1.8 * v.mod_feedback_state;
                    let mod_sig = (TAU * v.mod_phase + fb + fm_sub_amt * 3.5 * op3).sin();
                    v.mod_feedback_state = 0.5 * (v.mod_feedback_state + mod_sig);
                    let dyn_env = (1.0 - fm_env_amt) + fm_env_amt * v.mod_env;
                    let fm_amt = if self.lfo_dest == LfoDestination::OscCtrl {
                        (self.fm_amount + self.lfo_amount * lfo_val * 0.4 + osc_ctrl_mod * 0.4)
                            .clamp(0.0, 1.0)
                    } else {
                        (self.fm_amount + osc_ctrl_mod * 0.4).clamp(0.0, 1.0)
                    };
                    ((TAU * t + 4.0 * fm_amt * dyn_env * mod_sig).sin(), None)
                }
                SynthOscMode::Sample => {
                    if let Some((sound, z_idx)) = sample_sound.zip(v.sample_zone_idx) {
                        if let Some(zone) = sound.zones.get(z_idx) {
                            let (sl, sr) = Self::render_sample_zone_stereo(
                                &self.sinc_kernels,
                                zone,
                                &mut v.sample_pos,
                                v.sample_step * f64::from(pitch_mod),
                                &mut v.active,
                            );
                            let fl = Self::apply_sample_timbre(
                                self.sample_timbre,
                                sl,
                                &mut v.sample_lp_state,
                            );
                            let fr = sr.map(|r| {
                                Self::apply_sample_timbre(
                                    self.sample_timbre,
                                    r,
                                    &mut v.sample_lp_state_right,
                                )
                            });
                            (fl, fr)
                        } else {
                            (0.0, None)
                        }
                    } else {
                        let va = Self::render_va(
                            self.waveform,
                            self.supersaw_detune,
                            self.pulse_width,
                            self.lfo_dest,
                            self.lfo_amount,
                            lfo_val,
                            t,
                            dt,
                            eff_freq,
                            inv_fs,
                            &mut v.supersaw_phases,
                        );
                        (va, None)
                    }
                }
            };

            v.phase = (t + dt) % 1.0;
            v.mod_phase = (v.mod_phase + fm_ratio * dt) % 1.0;
            v.op3_phase = (v.op3_phase + fm_sub_ratio * dt) % 1.0;

            let kb_track_scale = if self.kb_track > 0 {
                let frac = f32::from(self.kb_track.min(3)) / 3.0;
                2.0f32.powf(((f32::from(v.note) - 60.0) / 12.0) * frac)
            } else {
                1.0
            };
            let env_scale = if self.filter_env_amount.abs() > 1e-4 {
                2.0f32.powf(
                    self.filter_env_amount.clamp(-1.0, 1.0) * FILTER_ENV_OCTAVES * filter_env_level,
                )
            } else {
                1.0
            };
            let voice_cutoff =
                (eff_cutoff * kb_track_scale * env_scale).clamp(14.0, 0.48 * self.sample_rate);

            let filtered_l = if self.filter_on {
                v.filter.process(
                    raw_osc_l,
                    self.filter_model,
                    voice_cutoff,
                    self.resonance,
                    self.filter_drive,
                    self.sample_rate,
                )
            } else {
                raw_osc_l
            };
            let amp_vel = (1.0 - self.amp_velocity.clamp(0.0, 1.0))
                + self.amp_velocity.clamp(0.0, 1.0) * v.velocity;
            let env_gain = v.amp_env * amp_vel * v.gain * 0.25;
            mix_l += filtered_l * env_gain;
            if let Some(osc_r) = raw_osc_r {
                let filtered_r = if self.filter_on {
                    v.filter_right.process(
                        osc_r,
                        self.filter_model,
                        voice_cutoff,
                        self.resonance,
                        self.filter_drive,
                        self.sample_rate,
                    )
                } else {
                    osc_r
                };
                mix_r += filtered_r * env_gain;
            } else {
                mix_r += filtered_l * env_gain;
            }
        }

        [mix_l, mix_r]
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn render_va(
        waveform: VaWaveform,
        supersaw_detune: f32,
        pulse_width: f32,
        lfo_dest: LfoDestination,
        lfo_amount: f32,
        lfo_val: f32,
        t: f32,
        dt: f32,
        eff_freq: f32,
        inv_fs: f32,
        supersaw_phases: &mut [f32; 7],
    ) -> f32 {
        match waveform {
            VaWaveform::Saw => (2.0 * t - 1.0) - poly_blep(t, dt),
            VaWaveform::SuperSaw => {
                let mut sum = 0.0f32;
                let detune_scale = supersaw_detune.clamp(0.0, 1.0);
                for (i, &det) in SUPERSAW_DETUNES.iter().enumerate() {
                    let f_osc = eff_freq * (1.0 + det * detune_scale * 0.08);
                    let dt_i = (f_osc * inv_fs).clamp(1e-5, 0.49);
                    let p_i = supersaw_phases[i];
                    let saw = (2.0 * p_i - 1.0) - poly_blep(p_i, dt_i);
                    sum += saw;
                    supersaw_phases[i] = (p_i + dt_i) % 1.0;
                }
                sum * (1.0 / 7.0)
            }
            VaWaveform::Pulse => {
                let mut pw = pulse_width.clamp(0.05, 0.95);
                if lfo_dest == LfoDestination::OscCtrl {
                    pw = (pw + lfo_amount * lfo_val * 0.4).clamp(0.05, 0.95);
                }
                let base = if t < pw { 1.0 } else { -1.0 };
                base + poly_blep(t, dt) - poly_blep((t + 1.0 - pw) % 1.0, dt)
            }
            VaWaveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            VaWaveform::Sine => (TAU * t).sin(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_layer_renders_silence() {
        let mut s = SynthLayer::new(48_000.0);
        for _ in 0..64 {
            assert_eq!(s.render_frame(), [0.0, 0.0]);
        }
    }
}
