//! `Stage4Engine` MIDI dispatch: note routing across layers, splits, channels and controllers.

use super::*;

impl Stage4Engine {
    /// Handles a real-time MIDI event across the 4-zone split router, Morph matrix, and 7 layers.
    pub fn handle_midi(&mut self, event: MidiEvent) {
        let channel = match event {
            MidiEvent::NoteOn { channel, .. }
            | MidiEvent::NoteOff { channel, .. }
            | MidiEvent::ControlChange { channel, .. }
            | MidiEvent::ChannelAftertouch { channel, .. }
            | MidiEvent::ProgramChange { channel, .. }
            | MidiEvent::PitchBend { channel, .. } => channel,
        };
        if channel >= 16 {
            return;
        }
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                let channel_idx = channel as usize;
                let Some(sounding) = self.transposed_note(note) else {
                    return;
                };
                let mut recipients = 0u8;
                for layer in LayerId::ALL {
                    if !self.scene.is_layer_active(layer) {
                        continue;
                    }
                    let Some(route) = self.route_for(layer, channel) else {
                        continue;
                    };
                    let split_gain = if route == NoteRoute::Global {
                        self.split_router.note_gain(layer, note)
                    } else {
                        1.0
                    };
                    if split_gain <= 0.0 {
                        continue;
                    }
                    let velocity = if route == NoteRoute::Aux {
                        self.aux_kb.curve.map(velocity)
                    } else {
                        velocity
                    };
                    recipients |= 1 << (layer as u8);
                    self.clear_held(layer, sounding);
                    self.layer_note_on(layer, sounding, velocity, split_gain);
                }
                if recipients != 0 {
                    self.held_transpose[channel_idx][note as usize] = self.effective_transpose();
                }
                for layer in LayerId::ALL {
                    if recipients & (1 << (layer as u8)) != 0 {
                        let count =
                            &mut self.note_hold_counts[channel_idx][note as usize][layer as usize];
                        *count = count.saturating_add(1);
                    }
                }
            }
            MidiEvent::NoteOff { note, .. } => {
                let channel_idx = channel as usize;
                for layer in LayerId::ALL {
                    let count =
                        &mut self.note_hold_counts[channel_idx][note as usize][layer as usize];
                    if *count == 0 {
                        continue;
                    }
                    *count -= 1;
                    let held_on_any_channel = self
                        .note_hold_counts
                        .iter()
                        .any(|counts| counts[note as usize][layer as usize] > 0);
                    if !held_on_any_channel {
                        let sounding = self.sounding_note(channel_idx, note);
                        self.release_or_hold(layer, sounding);
                    }
                }
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                if self.aux_kb.listens_on(channel) && controller == cc::SUSTAIN_PEDAL {
                    self.set_sustain_pedal(PedalSource::Aux, value);
                }
                if self.global_midi_channel != Some(channel) {
                    return;
                }
                let norm = f32::from(value.min(127)) / 127.0;
                match controller {
                    cc::MOD_WHEEL => self.morph_controllers.mod_wheel = norm,
                    cc::CONTROL_PEDAL => self.set_control_pedal(norm),
                    cc::ORGAN_SWELL => self.set_swell_pedal(norm),
                    cc::DRAWBAR_1_16..=cc::DRAWBAR_9_1 => {
                        let idx = (controller - cc::DRAWBAR_1_16) as usize;
                        let step = ((norm * 8.0).round() as u8).min(8);
                        self.organ_a.drawbars[idx] = step;
                    }
                    cc::SUSTAIN_PEDAL => self.set_sustain_pedal(PedalSource::Global, value),
                    cc::SOSTENUTO_PEDAL => {
                        let down = value >= 64;
                        self.piano_a.set_sostenuto_pedal(down, value);
                        self.piano_b.set_sostenuto_pedal(down, value);
                    }
                    cc::SOFT_PEDAL => {
                        let down = value >= 64;
                        self.piano_a.set_soft_pedal(down, value);
                        self.piano_b.set_soft_pedal(down, value);
                    }
                    cc::SYNTH_FILTER_CUTOFF => {
                        for m in &mut self.synth_cutoff_morph {
                            m.base = norm;
                        }
                    }
                    cc::ROTARY_SPEED => {
                        let spd = if value >= 64 {
                            RotarySpeed::Fast
                        } else {
                            RotarySpeed::Slow
                        };
                        self.organ_a.rotary_speed = spd;
                        self.organ_b.rotary_speed = spd;
                    }
                    cc::LAYER_SCENE_II => {
                        self.scene.scene_2_active = value >= 64;
                    }
                    cc::MASTER_COMPRESSOR => {
                        self.master_bus.compressor_amount = norm;
                    }
                    _ => {}
                }
            }
            MidiEvent::ChannelAftertouch { pressure, .. } => {
                if self.global_midi_channel == Some(channel) {
                    self.morph_controllers.aftertouch = f32::from(pressure.min(127)) / 127.0;
                }
            }
            MidiEvent::ProgramChange { .. } => {}
            MidiEvent::PitchBend { value, .. } => {
                if self.aux_kb.listens_on(channel) {
                    self.set_pitch_bend(PedalSource::Aux, value);
                }
                if self.global_midi_channel == Some(channel) {
                    self.set_pitch_bend(PedalSource::Global, value);
                }
            }
        }
    }

    pub(super) fn layer_note_on(&mut self, layer: LayerId, note: u8, velocity: u8, gain: f32) {
        if let Some(i) = self.extern_index(layer) {
            // Crossfade gain scales the transmitted velocity; exactly 1.0 outside crossfades.
            let v = (f32::from(velocity) * gain).round().clamp(1.0, 127.0) as u8;
            self.extern_note_on(i, note, v);
            return;
        }
        match layer {
            LayerId::OrganA => self.organ_a.note_on_with_gain(note, gain),
            LayerId::OrganB => self.organ_b.note_on_with_gain(note, gain),
            LayerId::PianoA => self.piano_a.note_on_with_gain(note, velocity, gain),
            LayerId::PianoB => self.piano_b.note_on_with_gain(note, velocity, gain),
            LayerId::SynthA => self.synth_a.note_on_with_gain(note, velocity, gain),
            LayerId::SynthB => self.synth_b.note_on_with_gain(note, velocity, gain),
            LayerId::SynthC => self.synth_c.note_on_with_gain(note, velocity, gain),
        }
    }

    pub(super) fn layer_note_off(&mut self, layer: LayerId, note: u8) {
        match layer {
            LayerId::OrganA => self.organ_a.note_off(note),
            LayerId::OrganB => self.organ_b.note_off(note),
            LayerId::PianoA => self.piano_a.note_off(note),
            LayerId::PianoB => self.piano_b.note_off(note),
            LayerId::SynthA => self.synth_a.note_off(note),
            LayerId::SynthB => self.synth_b.note_off(note),
            LayerId::SynthC => self.synth_c.note_off(note),
        }
    }

    #[cfg(test)]
    pub(super) fn note_recipients_for_test(&self, channel: u8, note: u8) -> u8 {
        self.note_hold_counts[channel as usize][note as usize]
            .iter()
            .enumerate()
            .fold(0, |mask, (layer, count)| {
                if *count > 0 {
                    mask | (1 << layer)
                } else {
                    mask
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_channel_is_ignored() {
        let mut e = Stage4Engine::new(48_000.0);
        e.handle_midi(MidiEvent::NoteOn {
            channel: 16,
            note: 60,
            velocity: 100,
        });
        assert_eq!(e.note_recipients_for_test(0, 60), 0);
    }
}
