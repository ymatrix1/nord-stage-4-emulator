//! MIDI routing and event dispatching for the Nord Stage 4 plugin.
//!
//! Supports both:
//! - OmniSplit mode: Standard Nord Stage 4 hardware 4-zone keybed router & split crossfade.
//! - MultitimbralPerLayer mode: Dedicated MIDI channels for each layer:
//!   - Channel 0: Organ A
//!   - Channel 1: Organ B
//!   - Channel 2: Piano A
//!   - Channel 3: Piano B
//!   - Channel 4: Synth A
//!   - Channel 5: Synth B
//!   - Channel 6: Synth C

use crate::params::{MidiRoutingMode, Stage4Params};
use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent as CoreMidiEvent;
use nord_core::morph::LayerId;

/// Routes a plugin NoteEvent into the `Stage4Engine` based on `MidiRoutingMode`.
pub fn route_plugin_event(
    engine: &mut Stage4Engine,
    _params: &Stage4Params,
    mode: MidiRoutingMode,
    event: NoteEvent<()>,
) {
    // A wildcard key (CLAP `-1` arrives as 255) is not a MIDI note; the per-layer note tables
    // are indexed by `note`, so drop it here before the multitimbral path touches them.
    if let NoteEvent::NoteOn { note, .. }
    | NoteEvent::NoteOff { note, .. }
    | NoteEvent::Choke { note, .. } = event
    {
        if note >= 128 {
            return;
        }
    }
    match mode {
        MidiRoutingMode::OmniSplit => {
            // OmniSplit: translate NoteEvent into CoreMidiEvent and feed engine.handle_midi
            if let Some(core_event) = to_core_midi_event(&event) {
                engine.handle_midi(core_event);
            }
        }
        MidiRoutingMode::MultitimbralPerLayer => {
            // Multitimbral per layer: route notes directly to the specific layer
            match event {
                NoteEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    let vel = ((velocity * 127.0).round() as u8).clamp(1, 127);
                    match channel {
                        0 => engine.organ_a.note_on(note),
                        1 => engine.organ_b.note_on(note),
                        2 => engine.piano_a.note_on(note, vel),
                        3 => engine.piano_b.note_on(note, vel),
                        4 => engine.synth_a.note_on(note, vel),
                        5 => engine.synth_b.note_on(note, vel),
                        6 => engine.synth_c.note_on(note, vel),
                        _ => {
                            // Other channels fall back to omni split
                            if let Some(core_event) = to_core_midi_event(&event) {
                                engine.handle_midi(core_event);
                            }
                        }
                    }
                }
                NoteEvent::NoteOff { channel, note, .. } => match channel {
                    0 => engine.organ_a.note_off(note),
                    1 => engine.organ_b.note_off(note),
                    2 => engine.piano_a.note_off(note),
                    3 => engine.piano_b.note_off(note),
                    4 => engine.synth_a.note_off(note),
                    5 => engine.synth_b.note_off(note),
                    6 => engine.synth_c.note_off(note),
                    _ => {
                        if let Some(core_event) = to_core_midi_event(&event) {
                            engine.handle_midi(core_event);
                        }
                    }
                },
                NoteEvent::Choke { channel, note, .. } => match channel {
                    0 => engine.organ_a.note_off(note),
                    1 => engine.organ_b.note_off(note),
                    2 => engine.piano_a.note_off(note),
                    3 => engine.piano_b.note_off(note),
                    4 => engine.synth_a.note_off(note),
                    5 => engine.synth_b.note_off(note),
                    6 => engine.synth_c.note_off(note),
                    _ => {
                        for layer in LayerId::ALL {
                            match layer {
                                LayerId::OrganA => engine.organ_a.note_off(note),
                                LayerId::OrganB => engine.organ_b.note_off(note),
                                LayerId::PianoA => engine.piano_a.note_off(note),
                                LayerId::PianoB => engine.piano_b.note_off(note),
                                LayerId::SynthA => engine.synth_a.note_off(note),
                                LayerId::SynthB => engine.synth_b.note_off(note),
                                LayerId::SynthC => engine.synth_c.note_off(note),
                            }
                        }
                    }
                },
                _ => {
                    if let Some(core_event) = to_core_midi_event(&event) {
                        engine.handle_midi(core_event);
                    }
                }
            }
        }
    }
}

/// Converts a NIH-plug `NoteEvent<()>` to `nord_core::midi::MidiEvent`.
pub fn to_core_midi_event(event: &NoteEvent<()>) -> Option<CoreMidiEvent> {
    match *event {
        NoteEvent::NoteOn {
            channel,
            note,
            velocity,
            ..
        } => {
            let vel = ((velocity * 127.0).round() as u8).min(127);
            if vel == 0 {
                Some(CoreMidiEvent::NoteOff {
                    channel,
                    note,
                    velocity: 0,
                })
            } else {
                Some(CoreMidiEvent::NoteOn {
                    channel,
                    note,
                    velocity: vel,
                })
            }
        }
        NoteEvent::NoteOff {
            channel,
            note,
            velocity,
            ..
        } => Some(CoreMidiEvent::NoteOff {
            channel,
            note,
            velocity: ((velocity * 127.0).round() as u8).min(127),
        }),
        NoteEvent::Choke { channel, note, .. } => Some(CoreMidiEvent::NoteOff {
            channel,
            note,
            velocity: 0,
        }),
        NoteEvent::MidiCC {
            channel, cc, value, ..
        } => Some(CoreMidiEvent::ControlChange {
            channel,
            controller: cc,
            value: ((value * 127.0).round() as u8).min(127),
        }),
        NoteEvent::MidiChannelPressure {
            channel, pressure, ..
        } => Some(CoreMidiEvent::ChannelAftertouch {
            channel,
            pressure: ((pressure * 127.0).round() as u8).min(127),
        }),
        NoteEvent::MidiProgramChange {
            channel, program, ..
        } => Some(CoreMidiEvent::ProgramChange { channel, program }),
        NoteEvent::MidiPitchBend { channel, value, .. } => Some(CoreMidiEvent::PitchBend {
            channel,
            value: (value.clamp(0.0, 1.0) * 16383.0).round() as u16,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for `clap-validator` `process-note-wildcard` (key -1 arrives as 255).
    #[test]
    fn wildcard_keys_are_dropped_in_every_routing_mode() {
        let params = Stage4Params::default();
        for mode in [
            MidiRoutingMode::OmniSplit,
            MidiRoutingMode::MultitimbralPerLayer,
        ] {
            let mut engine = Stage4Engine::new(48_000.0);
            for channel in 0..8u8 {
                for event in [
                    NoteEvent::NoteOn {
                        timing: 0,
                        voice_id: None,
                        channel,
                        note: 255,
                        velocity: 0.8,
                    },
                    NoteEvent::NoteOff {
                        timing: 0,
                        voice_id: None,
                        channel,
                        note: 255,
                        velocity: 0.0,
                    },
                    NoteEvent::Choke {
                        timing: 0,
                        voice_id: None,
                        channel,
                        note: 255,
                    },
                ] {
                    route_plugin_event(&mut engine, &params, mode, event);
                }
            }
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            engine.process_block(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|s| *s == 0.0), "{mode:?} sounded");
        }
    }

    #[test]
    fn pitch_bend_event_maps_to_14_bit_value() {
        let bend = |value| NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 1,
            value,
        };
        assert_eq!(
            to_core_midi_event(&bend(1.0)),
            Some(CoreMidiEvent::PitchBend {
                channel: 1,
                value: 16383
            })
        );
        assert_eq!(
            to_core_midi_event(&bend(0.0)),
            Some(CoreMidiEvent::PitchBend {
                channel: 1,
                value: 0
            })
        );
    }

    #[test]
    fn test_to_core_midi_event_conversion() {
        let note_on = NoteEvent::NoteOn {
            timing: 10,
            voice_id: None,
            channel: 2,
            note: 64,
            velocity: 0.8,
        };
        assert_eq!(
            to_core_midi_event(&note_on),
            Some(CoreMidiEvent::NoteOn {
                channel: 2,
                note: 64,
                velocity: 102
            })
        );

        let cc_evt = NoteEvent::MidiCC {
            timing: 20,
            channel: 0,
            cc: 1,
            value: 0.5,
        };
        assert_eq!(
            to_core_midi_event(&cc_evt),
            Some(CoreMidiEvent::ControlChange {
                channel: 0,
                controller: 1,
                value: 64
            })
        );

        let prog_evt = NoteEvent::MidiProgramChange {
            timing: 0,
            channel: 1,
            program: 42,
        };
        assert_eq!(
            to_core_midi_event(&prog_evt),
            Some(CoreMidiEvent::ProgramChange {
                channel: 1,
                program: 42
            })
        );
    }

    #[test]
    fn test_multitimbral_channel_routing() {
        let mut engine = Stage4Engine::new(44100.0);
        let params = Stage4Params::default();

        // Send NoteOn on channel 2 -> should target Piano A
        route_plugin_event(
            &mut engine,
            &params,
            MidiRoutingMode::MultitimbralPerLayer,
            NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 2,
                note: 60,
                velocity: 0.9,
            },
        );

        let mut left = [0.0f32; 64];
        let mut right = [0.0f32; 64];
        engine.process_block(&mut left, &mut right);

        let energy: f32 = left.iter().map(|s| s * s).sum();
        assert!(
            energy > 1e-4,
            "Piano A must generate sound when triggered via channel 2"
        );

        // NoteOff on channel 2
        route_plugin_event(
            &mut engine,
            &params,
            MidiRoutingMode::MultitimbralPerLayer,
            NoteEvent::NoteOff {
                timing: 0,
                voice_id: None,
                channel: 2,
                note: 60,
                velocity: 0.0,
            },
        );
    }
}
