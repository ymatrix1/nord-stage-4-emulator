//! Arpeggiator / gate engine for `SynthLayer`.

use super::*;

impl SynthLayer {
    /// Advances the arpeggiator engine and generates notes when active.
    pub(super) fn tick_arpeggiator(&mut self, inv_fs: f32) {
        if self.arp_mode == ArpMode::Off {
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
            self.arp_step = 0;
            return;
        }

        // Arp speed: 16th note division at arp_rate_bpm
        let sixteenth_sec = 60.0 / (self.arp_rate_bpm.max(30.0) * 4.0);
        self.arp_sample_counter += inv_fs;

        if self.arp_sample_counter >= sixteenth_sec {
            self.arp_sample_counter -= sixteenth_sec;

            // Release previous arp note
            self.release_arp_notes();

            if self.arp_mode == ArpMode::Poly {
                for (slot, &note) in held_list
                    .iter()
                    .take(count.min(MAX_SYNTH_VOICES))
                    .enumerate()
                {
                    self.trigger_voice(
                        note,
                        self.held_velocities[note as usize],
                        self.held_gains[note as usize],
                    );
                    self.arp_active_notes[slot] = Some(note);
                }
                self.arp_step = (self.arp_step + 1) % count.max(1);
                return;
            }

            // Pick next note
            let oct_range = self.arp_octave_range.clamp(1, 4) as usize;
            let total_steps = count * oct_range;
            let step = self.arp_step % total_steps;

            let (note_in_chord, oct_offset) = match self.arp_mode {
                ArpMode::Up => (step % count, (step / count) as u8),
                ArpMode::Down => (
                    (count - 1) - (step % count),
                    (oct_range - 1 - (step / count)) as u8,
                ),
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
                    (
                        (self.arp_rng as usize) % count,
                        ((self.arp_rng >> 16) as usize % oct_range) as u8,
                    )
                }
                ArpMode::Off | ArpMode::Poly => unreachable!(),
            };

            let next_note = (held_list[note_in_chord] + oct_offset * 12).min(127);
            self.arp_active_notes[0] = Some(next_note);
            self.arp_step = (self.arp_step + 1) % total_steps;

            let source_note = held_list[note_in_chord] as usize;
            self.trigger_voice(
                next_note,
                self.held_velocities[source_note],
                self.held_gains[source_note],
            );
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
        let mut synth = SynthLayer::new(48_000.0);
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
