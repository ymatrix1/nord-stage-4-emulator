//! Voice allocation: note on/off, mono/legato, note priority, glide and pitch offsets.

use super::*;

impl SynthLayer {
    /// Triggers a note on across the polyphonic voice allocator or records in arpeggiator list.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on_with_gain(note, velocity, 1.0);
    }

    pub fn note_on_with_gain(&mut self, note: u8, velocity: u8, gain: f32) {
        let note_idx = (note as usize).min(127);
        self.held_keys[note_idx] = true;
        self.held_velocities[note_idx] = velocity.clamp(1, 127);
        self.held_gains[note_idx] = gain.clamp(0.0, 1.0);

        if self.arp_mode != ArpMode::Off {
            return;
        }

        if self.mono {
            let blocked = match self.note_priority {
                NotePriority::Off => false,
                NotePriority::Lo => (0..note_idx).any(|n| self.held_keys[n]),
                NotePriority::Hi => ((note_idx + 1)..128).any(|n| self.held_keys[n]),
            };
            if blocked {
                return;
            }
        }

        self.trigger_voice(note, velocity, gain);
    }

    pub(super) fn trigger_voice(&mut self, note: u8, velocity: u8, gain: f32) {
        let vel_norm = f32::from(velocity.clamp(1, 127)) / 127.0;
        let oct = f32::from(self.octave_offset.clamp(-3, 3));
        let freq =
            440.0 * 2.0f32.powf((f32::from(note) - 69.0) / 12.0 + oct + self.pitch_cents / 1200.0);

        // Mono: one voice; others are released. Legato keeps envelopes running.
        let mut legato_hit = false;
        let mono_idx = if self.mono {
            let keep = self
                .voices
                .iter()
                .position(|v| v.active && v.gate)
                .or_else(|| self.voices.iter().position(|v| v.active));
            for (i, v) in self.voices.iter_mut().enumerate() {
                if Some(i) != keep && v.active {
                    v.gate = false;
                }
            }
            legato_hit = self.legato && keep.is_some_and(|i| self.voices[i].gate);
            keep
        } else {
            None
        };

        // Mono glides on every note; Legato only glides between overlapping notes.
        let glide_allowed = self.mono && (!self.legato || legato_hit);
        let glide_from = if self.glide_sec > 0.0 && self.last_freq_hz > 0.0 && glide_allowed {
            self.last_freq_hz
        } else {
            freq
        };
        self.last_freq_hz = freq;

        if legato_hit {
            if let Some(i) = mono_idx {
                let v = &mut self.voices[i];
                v.note = note;
                v.target_freq_hz = freq;
                if self.glide_sec <= 0.0 {
                    v.freq_hz = freq;
                }
                return;
            }
        }

        // Find existing note voice to re-trigger, or first inactive voice, or steal oldest/quietest
        let idx = mono_idx
            .or_else(|| self.voices.iter().position(|v| v.active && v.note == note))
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or_else(|| {
                // Steal voice with lowest amp envelope
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.amp_env
                            .partial_cmp(&b.amp_env)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let (sample_zone_idx, sample_step) = if let Some(sound) = &self.sample_sound {
            if let Some(z_idx) = sound.zone_for_note(note) {
                let zone = &sound.zones[z_idx];
                let step = (f64::from(zone.sample_rate) / f64::from(self.sample_rate))
                    * 2.0f64.powf((f64::from(note) - f64::from(zone.root_key)) / 12.0);
                (Some(z_idx), step)
            } else {
                (None, 1.0)
            }
        } else {
            (None, 1.0)
        };

        self.voices[idx] = SynthVoice {
            active: true,
            gate: true,
            note,
            velocity: vel_norm,
            gain: gain.clamp(0.0, 1.0),
            gain_target: gain.clamp(0.0, 1.0),
            gain_state: self.voices[idx].gain_state,
            freq_hz: glide_from,
            target_freq_hz: freq,
            fenv: 0.0,
            fenv_attack_done: false,
            oenv: 0.0,
            oenv_attack_done: false,
            age_sec: 0.0,
            uni_phase: UNISON_START_PHASES,
            uni_supersaw: [SUPERSAW_PHASE_INITS; 4],
            phase: 0.0,
            mod_phase: 0.0,
            op3_phase: 0.0,
            mod_feedback_state: 0.0,
            mod_env: 1.0,
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.01,
            filter: self.voices[idx].filter,
            filter_right: self.voices[idx].filter_right,
            sample_zone_idx,
            sample_pos: 0.0,
            sample_step,
            sample_lp_state: 0.0,
            sample_lp_state_right: 0.0,
        };
    }

    /// Releases a held note across the polyphonic voices.
    pub fn note_off(&mut self, note: u8) {
        let note_idx = (note as usize).min(127);
        self.held_keys[note_idx] = false;

        if self.arp_mode != ArpMode::Off {
            return;
        }

        if self.mono {
            // Fall back to a remaining key when the sounding key is released (Lo = lowest, else highest).
            let fallback = if self.note_priority == NotePriority::Lo {
                (0..128usize).find(|&n| self.held_keys[n])
            } else {
                (0..128usize).rev().find(|&n| self.held_keys[n])
            };
            if let Some(n) = fallback {
                if self
                    .voices
                    .iter()
                    .any(|v| v.active && v.gate && v.note == note)
                {
                    self.trigger_voice(n as u8, self.held_velocities[n], self.held_gains[n]);
                }
                return;
            }
        }

        for v in &mut self.voices {
            if v.active && v.note == note {
                v.gate = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn unison_decorrelates_stereo_and_off_is_mono_identical() {
        let mut off = SynthLayer::new(48_000.0);
        off.note_on(60, 100);
        let frames = render_n(&mut off, 4000);
        assert!(frames.iter().all(|f| (f[0] - f[1]).abs() < 1e-6));

        let mut uni = SynthLayer::new(48_000.0);
        uni.unison = 3;
        uni.note_on(60, 100);
        let frames = render_n(&mut uni, 4000);
        let diff: f32 = frames.iter().map(|f| (f[0] - f[1]).abs()).sum();
        assert!(diff > 1.0, "unison L/R should differ, diff={diff}");
        assert!(frames.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    }

    /// Hann-windowed single-bin DFT magnitude of `x` at `freq` Hz.
    fn bin_magnitude(x: &[f32], rate: f32, freq: f32) -> f32 {
        let n = x.len() as f32;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1.0)).cos();
            let ph = f64::from(std::f32::consts::TAU * freq * i as f32 / rate);
            re += f64::from(v * w) * ph.cos();
            im -= f64::from(v * w) * ph.sin();
        }
        (re * re + im * im).sqrt() as f32
    }

    #[test]
    fn unison_partner_voices_land_on_the_measured_cents_and_channels() {
        use crate::synth::{SynthOscMode, VaWaveform};
        use crate::synth_tables::{UNISON_VOICES, UNISON_VOICE_COUNT, UNISON_WEAK_CHANNEL_GAIN};
        let rate = 48_000.0f32;
        let f0 = 1760.0f32; // MIDI 93
        for level in 1..=3u8 {
            let mut s = SynthLayer::new(rate);
            s.osc_mode = SynthOscMode::Analog;
            s.waveform = VaWaveform::Sine;
            s.filter_on = false;
            s.attack_sec = 0.0005;
            s.decay_sec = 45.0;
            s.sustain_level = 1.0;
            s.unison = level;
            s.note_on(93, 127);
            render_n(&mut s, 4800);
            let frames = render_n(&mut s, 120_000);
            let left: Vec<f32> = frames.iter().map(|f| f[0]).collect();
            let right: Vec<f32> = frames.iter().map(|f| f[1]).collect();
            let n = UNISON_VOICE_COUNT[usize::from(level)];
            let mut strongs = Vec::new();
            for voice in &UNISON_VOICES[usize::from(level)][..n] {
                let f = f0 * voice.ratio;
                let (ml, mr) = (
                    bin_magnitude(&left, rate, f),
                    bin_magnitude(&right, rate, f),
                );
                let (strong, weak) = if voice.strong_left {
                    (ml, mr)
                } else {
                    (mr, ml)
                };
                assert!(strong > weak, "level {level} ratio {}", voice.ratio);
                let leak = weak / strong;
                assert!(
                    (leak - UNISON_WEAK_CHANNEL_GAIN).abs() < 0.06,
                    "level {level} ratio {} leak {leak}",
                    voice.ratio
                );
                strongs.push(strong);
            }
            let max = strongs.iter().cloned().fold(0.0f32, f32::max);
            let min = strongs.iter().cloned().fold(f32::MAX, f32::min);
            assert!(
                min > 0.8 * max,
                "level {level}: voices must have equal level"
            );
            if level != 2 {
                // No centre voice (levels 1 and 3 keep every partner at least 2 cents away).
                let centre = bin_magnitude(&left, rate, f0).max(bin_magnitude(&right, rate, f0));
                assert!(
                    centre < 0.25 * max,
                    "level {level}: centre {centre} vs {max}"
                );
            }
        }
    }

    #[test]
    fn mono_legato_keeps_single_voice_and_glide_slews_pitch() {
        let mut s = SynthLayer::new(48_000.0);
        s.mono = true;
        s.legato = true;
        s.glide_sec = 0.05; // seconds per octave
        s.note_on(60, 100);
        render_n(&mut s, 100);
        s.note_on(72, 100);
        assert_eq!(s.voices.iter().filter(|v| v.active).count(), 1);
        let v = s.voices.iter().find(|v| v.active).unwrap();
        assert_eq!(v.note, 72);
        let start = v.freq_hz;
        render_n(&mut s, 12_000);
        let v = s.voices.iter().find(|v| v.active).unwrap();
        let target = 440.0 * 2.0f32.powf(3.0 / 12.0);
        assert!(start < target * 0.6, "glide must start near old pitch");
        assert!((v.freq_hz - target).abs() < target * 0.01);
    }

    #[test]
    fn mono_note_off_falls_back_to_held_key() {
        let mut s = SynthLayer::new(48_000.0);
        s.mono = true;
        s.note_on(60, 100);
        s.note_on(64, 100);
        s.note_off(64);
        let v = s.voices.iter().find(|v| v.active && v.gate).unwrap();
        assert_eq!(v.note, 60);
    }

    #[test]
    fn octave_offset_shifts_frequency() {
        let mut s = SynthLayer::new(48_000.0);
        s.octave_offset = -1;
        s.note_on(69, 100);
        let v = s.voices.iter().find(|v| v.active).unwrap();
        assert!((v.target_freq_hz - 220.0).abs() < 0.01);
    }

    #[test]
    fn note_priority_lo_and_hi_ignore_keys_on_the_wrong_side() {
        let mut lo = SynthLayer::new(48_000.0);
        lo.mono = true;
        lo.note_priority = NotePriority::Lo;
        lo.note_on(60, 100);
        lo.note_on(72, 100); // higher than held lowest: ignored
        assert_eq!(
            lo.voices.iter().find(|v| v.active && v.gate).unwrap().note,
            60
        );
        lo.note_on(48, 100); // lower: takes over
        assert_eq!(
            lo.voices.iter().find(|v| v.active && v.gate).unwrap().note,
            48
        );

        let mut hi = SynthLayer::new(48_000.0);
        hi.mono = true;
        hi.note_priority = NotePriority::Hi;
        hi.note_on(60, 100);
        hi.note_on(48, 100); // lower than held highest: ignored
        assert_eq!(
            hi.voices.iter().find(|v| v.active && v.gate).unwrap().note,
            60
        );
        hi.note_on(72, 100);
        assert_eq!(
            hi.voices.iter().find(|v| v.active && v.gate).unwrap().note,
            72
        );

        // Lo falls back to the lowest remaining key on release.
        let mut lo2 = SynthLayer::new(48_000.0);
        lo2.mono = true;
        lo2.note_priority = NotePriority::Lo;
        lo2.note_on(48, 100);
        lo2.note_on(60, 100); // ignored but held
        lo2.note_on(72, 100);
        lo2.note_off(48);
        assert_eq!(
            lo2.voices.iter().find(|v| v.active && v.gate).unwrap().note,
            60
        );
    }

    #[test]
    fn legato_glides_only_between_overlapping_notes_and_mono_always() {
        let mut leg = SynthLayer::new(48_000.0);
        leg.mono = true;
        leg.legato = true;
        leg.glide_sec = 0.5;
        leg.note_on(60, 100);
        render_n(&mut leg, 50);
        leg.note_off(60);
        render_n(&mut leg, 50);
        leg.note_on(72, 100); // not overlapping: no glide
        let v = leg.voices.iter().find(|v| v.active && v.gate).unwrap();
        assert!((v.freq_hz - v.target_freq_hz).abs() < 1e-3);

        let mut mono = SynthLayer::new(48_000.0);
        mono.mono = true;
        mono.glide_sec = 0.5;
        mono.note_on(60, 100);
        render_n(&mut mono, 50);
        mono.note_off(60);
        mono.note_on(72, 100);
        let v = mono.voices.iter().find(|v| v.active && v.gate).unwrap();
        assert!(
            v.freq_hz < v.target_freq_hz * 0.6,
            "mono glides from the old pitch"
        );
    }

    #[test]
    fn glide_is_constant_rate_so_two_octaves_take_twice_as_long() {
        let time_for = |hi_note: u8| {
            let mut s = SynthLayer::new(48_000.0);
            s.mono = true;
            s.legato = true;
            s.glide_sec = 0.1;
            s.note_on(48, 100);
            render_n(&mut s, 10);
            s.note_on(hi_note, 100);
            let target = 440.0 * 2.0f32.powf((f32::from(hi_note) - 69.0) / 12.0);
            samples_until_freq(&mut s, target)
        };
        let one = time_for(60) as f32;
        let two = time_for(72) as f32;
        assert!((two / one - 2.0).abs() < 0.1, "one={one} two={two}");
    }

    #[test]
    fn pitch_cents_shift_the_target_frequency() {
        let mut s = SynthLayer::new(48_000.0);
        s.pitch_cents = 1200.0;
        s.note_on(69, 100);
        let v = s.voices.iter().find(|v| v.active).unwrap();
        assert!((v.target_freq_hz - 880.0).abs() < 0.05);
    }
}
