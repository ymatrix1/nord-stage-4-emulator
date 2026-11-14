//! Arpeggiator / gate engine for `SynthLayer`.

use super::arp_order::pool_position;
use super::*;

impl SynthLayer {
    /// Advances the arpeggiator engine and generates notes when active.
    pub(super) fn tick_arpeggiator(&mut self, inv_fs: f32) {
        if self.arp_mode != ArpMode::Gate {
            self.perf.gate_level = 1.0;
        }
        if self.arp_mode == ArpMode::Off {
            return;
        }
        if self.arp_mode == ArpMode::Gate {
            self.tick_gate(inv_fs);
            return;
        }

        let mut held_list = [0u8; 128];
        let mut count = 0;
        for (n, &held) in self.held_keys.iter().enumerate() {
            if held {
                held_list[count] = n as u8;
                count += 1;
            }
        }

        if count == 0 {
            self.release_arp_notes();
            self.perf.tie_out = false;
            self.perf.tied_in = false;
            self.arp_step = 0;
            return;
        }

        // Arp speed: 16th note at arp_rate_bpm, or a Master Clock subdivision when synced.
        let sixteenth_sec = self.step_seconds();
        self.arp_sample_counter += inv_fs;

        if self.arp_sample_counter < sixteenth_sec {
            if self.perf.pattern_on {
                self.pattern_release(sixteenth_sec);
            }
        } else {
            self.arp_sample_counter -= sixteenth_sec;

            // Release the previous arp note, unless a `Q` step ties it into this one.
            let keep = self.pattern_keeps_notes();
            if !keep {
                self.release_arp_notes();
            }
            if self.pattern_rest() {
                if keep && self.perf.cur_len == 0 {
                    self.release_arp_notes();
                }
                return;
            }

            if self.arp_mode == ArpMode::Poly {
                for (slot, &note) in held_list
                    .iter()
                    .take(count.min(MAX_SYNTH_VOICES))
                    .enumerate()
                {
                    let velocity = self.accent_velocity(self.held_velocities[note as usize]);
                    self.perf.voice_pan = self.step_pan();
                    self.trigger_voice(note, velocity, self.held_gains[note as usize]);
                    self.perf.voice_pan = 0.0;
                    self.arp_active_notes[slot] = Some(note);
                }
                self.arp_step = (self.arp_step + 1) % count.max(1);
                return;
            }

            // Pick next note
            let oct_range = self.arp_octave_range.clamp(1, 4) as usize;
            let extra = extra_notes(&held_list[..count], self.perf.range_extra_thirds);
            let pool = count * oct_range + extra;
            // ZIG ZAG walks the pool two forward, one back; the step counter then runs over
            // two periods.
            let zig = self.perf.zigzag;
            let step = if zig {
                zigzag_index(self.arp_step, pool)
            } else {
                self.arp_step % pool
            };

            let (note_in_chord, oct_offset) = match self.arp_mode {
                ArpMode::Up => pool_position(step, count),
                ArpMode::Down if extra > 0 => pool_position(pool - 1 - step, count),
                ArpMode::Down => (
                    (count - 1) - (step % count),
                    (oct_range - 1 - (step / count)) as u8,
                ),
                ArpMode::UpDown if extra > 0 => {
                    let cycle_len = (pool * 2).saturating_sub(2).max(1);
                    let sub_step = step % cycle_len;
                    let pos = if sub_step < pool {
                        sub_step
                    } else {
                        cycle_len - sub_step
                    };
                    pool_position(pos, count)
                }
                ArpMode::UpDown => {
                    let cycle_len = (count * 2).saturating_sub(2).max(1);
                    let sub_step = step % cycle_len;
                    let n_idx = if sub_step < count {
                        sub_step
                    } else {
                        cycle_len - sub_step
                    };
                    (n_idx, (step / count) as u8)
                }
                ArpMode::Random => {
                    self.arp_rng = self
                        .arp_rng
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223);
                    if extra > 0 {
                        pool_position((self.arp_rng as usize) % pool, count)
                    } else {
                        (
                            (self.arp_rng as usize) % count,
                            ((self.arp_rng >> 16) as usize % oct_range) as u8,
                        )
                    }
                }
                ArpMode::Off | ArpMode::Poly | ArpMode::Gate => unreachable!(),
            };

            let next_note = (held_list[note_in_chord] + oct_offset * 12).min(127);
            self.arp_active_notes[0] = Some(next_note);
            self.arp_step = (self.arp_step + 1) % if zig { pool * 2 } else { pool };

            let source_note = held_list[note_in_chord] as usize;
            let velocity = self.accent_velocity(self.held_velocities[source_note]);
            self.perf.voice_pan = self.step_pan();
            self.trigger_voice(next_note, velocity, self.held_gains[source_note]);
            self.perf.voice_pan = 0.0;
        }
    }

    pub(super) fn release_arp_notes(&mut self) {
        for active in &mut self.arp_active_notes {
            if let Some(note) = active.take() {
                for voice in &mut self.voices {
                    if voice.active && voice.note == note {
                        voice.gate = false;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arpeggiator_generation() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.arp_mode = ArpMode::Up;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 100);
        synth.note_on(64, 100);
        synth.note_on(67, 100);

        let mut energy = 0.0f32;
        for _ in 0..24000 {
            let [l, r] = synth.render_frame();
            energy += l * l + r * r;
        }
        assert!(energy > 0.05, "Arpeggiator must output active sound energy");
        assert!(synth.arp_step > 0, "Arpeggiator step must advance");
    }

    #[test]
    fn poly_arpeggiator_triggers_held_chord_simultaneously() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.arp_mode = ArpMode::Poly;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 70);
        synth.note_on(64, 90);
        synth.note_on(67, 110);
        for _ in 0..3_200 {
            synth.render_frame();
        }
        for note in [60, 64, 67] {
            assert!(synth
                .voices
                .iter()
                .any(|voice| voice.active && voice.gate && voice.note == note));
        }
    }

    #[test]
    fn arpeggiator_uses_allocator_without_overwriting_an_active_voice() {
        let mut synth = SynthLayer::legacy(48_000.0);
        synth.note_on(40, 100);
        synth.note_off(40);
        synth.arp_mode = ArpMode::Up;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 100);
        synth.note_on(64, 100);
        for _ in 0..3_200 {
            synth.render_frame();
        }
        assert_eq!(synth.voices[0].note, 40);
        assert!(!synth.voices[0].gate);
        assert!(synth
            .voices
            .iter()
            .any(|voice| voice.active && voice.gate && voice.note == 60));
    }
}
