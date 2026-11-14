//! Transmit side of the MIDI menu: the instrument's keyboard entry (`handle_keyboard_midi`) and
//! Program Change / Bank Select output on the Global channel (manual OS v1.6x p.56, p.61-62).
//! Spec: `docs/specs/midi-and-control.md` section 2b. The settings live in `midi_menu.rs`.

use super::midi_menu::{is_device_cc, TransposeAt, NO_NOTE};
use super::*;

impl Stage4Engine {
    /// Transmits Bank Select MSB / LSB and Program Change on the Global channel when PC (Program
    /// and Live banks, MSB 0 / 1) or PC Ext (preset banks, MSB 2-4) sends. `true` if sent.
    pub fn send_program_change(&mut self, bank_msb: u8, bank_lsb: u8, program: u8) -> bool {
        let mode = if (2..=4).contains(&bank_msb) {
            self.midi_menu.pc_ext
        } else {
            self.midi_menu.pc
        };
        if !mode.sends() || self.global_midi_channel.is_none() {
            return false;
        }
        self.global_emit(|ch| MidiOut::control_change(ch, cc::BANK_SELECT_MSB, bank_msb));
        self.global_emit(|ch| MidiOut::control_change(ch, cc::BANK_SELECT_LSB, bank_lsb));
        self.global_emit(|ch| MidiOut::program_change(ch, program));
        true
    }

    pub(super) fn global_emit(&mut self, make: impl FnOnce(u8) -> MidiOut) {
        if let Some(ch) = self.global_midi_channel {
            self.extern_out.queue.push(make(ch));
        }
    }

    /// An event from the instrument's own keyboard or panel (the on-screen keys of a host): it is
    /// transmitted on the Global channel (subject to Ctrl / Dev send and Transpose MIDI At) and,
    /// with Local Control On, also dispatched to the sound engines like `handle_midi`.
    pub fn handle_keyboard_midi(&mut self, event: MidiEvent) {
        self.transmit_keyboard(event);
        if self.midi_menu.local_control {
            self.midi_rt.keyboard = true;
            self.handle_midi(event);
            self.midi_rt.keyboard = false;
        }
    }

    fn transmit_keyboard(&mut self, event: MidiEvent) {
        let shift = if self.midi_menu.transpose_at == TransposeAt::Out {
            i16::from(self.effective_transpose())
        } else {
            0
        };
        let m = self.midi_menu;
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                let out = i16::from(note) + shift;
                if (0..=127).contains(&out) {
                    self.midi_rt.sent_note[usize::from(note & 0x7F)] = out as u8;
                    self.global_emit(|ch| MidiOut::note_on(ch, out as u8, velocity));
                }
            }
            MidiEvent::NoteOff { note, .. } => {
                let sent = std::mem::replace(
                    &mut self.midi_rt.sent_note[usize::from(note & 0x7F)],
                    NO_NOTE,
                );
                if sent != NO_NOTE {
                    self.global_emit(|ch| MidiOut::note_off(ch, sent));
                }
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                let mode = if is_device_cc(controller) {
                    m.dev
                } else {
                    m.ctrl
                };
                if mode.sends() {
                    self.global_emit(|ch| MidiOut::control_change(ch, controller, value));
                }
            }
            MidiEvent::ChannelAftertouch { pressure, .. } if m.dev.sends() => {
                self.global_emit(|ch| MidiOut {
                    status: 0xD0 | (ch & 0x0F),
                    data1: pressure & 0x7F,
                    data2: 0,
                    len: 2,
                });
            }
            MidiEvent::PitchBend { value, .. } if m.dev.sends() => {
                self.global_emit(|ch| MidiOut::pitch_bend(ch, value));
            }
            // Program Change goes through `send_program_change` (bank aware).
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(e: &mut Stage4Engine) -> Vec<[u8; 3]> {
        std::iter::from_fn(|| e.pop_midi_out())
            .map(|m| m.bytes())
            .collect()
    }

    #[test]
    fn keyboard_notes_are_transmitted_with_a_matching_note_off() {
        let mut e = Stage4Engine::new(48_000.0);
        e.midi_menu.local_control = false;
        e.midi_menu.transpose_at = TransposeAt::Out;
        e.set_transpose(2);
        e.handle_keyboard_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 90,
        });
        e.set_transpose(0);
        e.handle_keyboard_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(drain(&mut e), vec![[0x90, 62, 90], [0x80, 62, 0]]);
    }

    #[test]
    fn program_change_goes_out_as_bank_select_then_pc_only_when_sending() {
        let mut e = Stage4Engine::new(48_000.0);
        assert!(e.send_program_change(1, 0, 5));
        assert_eq!(
            drain(&mut e),
            vec![[0xB0, 0, 1], [0xB0, 32, 0], [0xC0, 5, 0]]
        );
        e.midi_menu.pc = MidiMode::Receive;
        assert!(!e.send_program_change(1, 0, 5));
        assert!(drain(&mut e).is_empty());
    }
}
