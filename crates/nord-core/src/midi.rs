//! Nord Stage 4 MIDI CC Implementation & Event Parser.
//!
//! Spec: `docs/specs/midi-and-control.md`

/// Standard MIDI CC numbers used by the Nord Stage 4 control surface.
pub mod cc {
    pub const BANK_SELECT_MSB: u8 = 0;
    pub const MOD_WHEEL: u8 = 1;
    pub const ORGAN_SWELL: u8 = 4;
    pub const MASTER_LEVEL: u8 = 7;
    pub const CONTROL_PEDAL: u8 = 11;
    pub const DRAWBAR_1_16: u8 = 16;
    pub const DRAWBAR_9_1: u8 = 24;
    pub const ORGAN_MODEL: u8 = 25;
    pub const ORGAN_VIBRATO_MODE: u8 = 26;
    pub const ORGAN_VIBRATO_ENABLE: u8 = 27;
    pub const ORGAN_PERC_ENABLE: u8 = 28;
    pub const ORGAN_PERC_HARMONIC: u8 = 29;
    pub const ORGAN_PERC_DECAY: u8 = 30;
    pub const ORGAN_PERC_VOLUME: u8 = 31;
    pub const BANK_SELECT_LSB: u8 = 32;
    pub const PIANO_MODEL: u8 = 40;
    pub const PIANO_TIMBRE: u8 = 42;
    pub const PIANO_STRING_RES: u8 = 44;
    pub const PIANO_PEDAL_NOISE: u8 = 45;
    pub const SYNTH_OSC_MODE: u8 = 50;
    pub const SYNTH_OSC_SHAPE: u8 = 51;
    pub const SYNTH_FILTER_TYPE: u8 = 59;
    pub const SYNTH_FILTER_CUTOFF: u8 = 60;
    pub const SYNTH_FILTER_RESONANCE: u8 = 61;
    pub const SUSTAIN_PEDAL: u8 = 64;
    pub const SOSTENUTO_PEDAL: u8 = 66;
    pub const SOFT_PEDAL: u8 = 67;
    pub const SYNTH_AMP_ATTACK: u8 = 71;
    pub const SYNTH_AMP_RELEASE: u8 = 72;
    pub const ROTARY_SPEED: u8 = 82;
    pub const LAYER_SCENE_II: u8 = 85;
    pub const REVERB_WET: u8 = 91;
    pub const DELAY_MIX: u8 = 94;
    pub const LAYER_LEVEL_ORG_A: u8 = 100;
    pub const LAYER_LEVEL_SYN_C: u8 = 106;
    pub const MASTER_COMPRESSOR: u8 = 115;
    pub const ALL_SOUND_OFF: u8 = 120;
    pub const ALL_NOTES_OFF: u8 = 123;
}

/// Decoded real-time MIDI event for the `Stage4Engine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ChannelAftertouch {
        channel: u8,
        pressure: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
}

impl MidiEvent {
    /// Parses a 2- or 3-byte standard MIDI channel message.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let status = *bytes.first()?;
        if !(0x80..0xF0).contains(&status) {
            return None;
        }
        let kind = status & 0xF0;
        let channel = status & 0x0F;
        match kind {
            0x80 if bytes.len() >= 3 => Some(Self::NoteOff {
                channel,
                note: bytes[1] & 0x7F,
                velocity: bytes[2] & 0x7F,
            }),
            0x90 if bytes.len() >= 3 => {
                let note = bytes[1] & 0x7F;
                let velocity = bytes[2] & 0x7F;
                if velocity == 0 {
                    Some(Self::NoteOff {
                        channel,
                        note,
                        velocity: 0,
                    })
                } else {
                    Some(Self::NoteOn {
                        channel,
                        note,
                        velocity,
                    })
                }
            }
            0xB0 if bytes.len() >= 3 => Some(Self::ControlChange {
                channel,
                controller: bytes[1] & 0x7F,
                value: bytes[2] & 0x7F,
            }),
            0xC0 if bytes.len() >= 2 => Some(Self::ProgramChange {
                channel,
                program: bytes[1] & 0x7F,
            }),
            0xD0 if bytes.len() >= 2 => Some(Self::ChannelAftertouch {
                channel,
                pressure: bytes[1] & 0x7F,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_midi_messages_and_velocity_zero_note_off() {
        assert_eq!(
            MidiEvent::from_bytes(&[0x90, 60, 100]),
            Some(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            })
        );
        assert_eq!(
            MidiEvent::from_bytes(&[0x90, 60, 0]),
            Some(MidiEvent::NoteOff {
                channel: 0,
                note: 60,
                velocity: 0
            })
        );
        assert_eq!(
            MidiEvent::from_bytes(&[0xB0, cc::MOD_WHEEL, 64]),
            Some(MidiEvent::ControlChange {
                channel: 0,
                controller: 1,
                value: 64
            })
        );
        assert_eq!(
            MidiEvent::from_bytes(&[0xD2, 99]),
            Some(MidiEvent::ChannelAftertouch {
                channel: 2,
                pressure: 99
            })
        );
    }
}
