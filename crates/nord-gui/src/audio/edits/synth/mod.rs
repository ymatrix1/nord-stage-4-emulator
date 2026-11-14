//! Synth edits applied on the DSP thread (one `SynthParam` variant per engine-affecting control).
//!
//! `AudioCommand::Synth` carries a [`SynthEdit`]. A `Param` edit is written to the focused layer,
//! or to all three layers when a GROUP / Section Edit is on ([`SynthTarget::All`]); a `Group` edit
//! copies one settings group from a source layer to the others. Nothing here allocates.

mod group;
mod param;
mod pattern;

pub use group::SynthGroup;
pub use param::SynthParam;
pub use pattern::PatternParam;

use nord_core::engine::Stage4Engine;
use nord_core::morph::{LayerId, MorphParameter};
use nord_core::synth::SynthLayer;

/// Engine layer index of Synth A (Organ A, B, Piano A, B, Synth A, B, C).
const SYNTH_FIRST_LAYER: usize = 4;

/// Which Synth layers an edit reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthTarget {
    /// One layer: 0 = A, 1 = B, 2 = C.
    Layer(u8),
    /// All three layers (GROUP on, or a section-wide control such as KB Hold).
    All,
}

/// Panel edits for the Synth area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynthEdit {
    /// Write one control value.
    Param(SynthTarget, SynthParam),
    /// GROUP: copy a settings group from layer `src` to the other layers.
    Group { src: u8, group: SynthGroup },
    /// SOUND INIT of layer `0..=2` (Shift+Waveform, manual p.37): `SynthLayer::sound_init` plus the
    /// cutoff morph reset. Allocation-free, unlike `Stage4Engine::sound_init`, which also pushes an
    /// Undo snapshot (so this edit is not undoable).
    SoundInit(u8),
    /// SUSTPED (Shift+Layer A): all three Synth layers follow the sustain pedal.
    Sustped(bool),
    /// PSTICK (Shift+Layer B): all three Synth layers follow the pitch stick.
    Pstick(bool),
    /// KB ZONE of Synth layer `0..=2`: the zone LED set (bit `u` = zone `u + 1`).
    Zone { layer: u8, leds: u8 },
    /// Aux KB LED of Synth layer `0..=2`: the layer is played by the aux keyboard.
    AuxKb { layer: u8, on: bool },
    /// EXTERN (Shift+Mode) of Synth layer `0..=2`: MIDI out instead of audio (manual p.46).
    Extern { layer: u8, on: bool },
    /// CC VAL 1 / CC VAL 2 dial (`slot` 0 or 1) of an Extern layer: stores and transmits the value.
    ExternCc { layer: u8, slot: u8, value: u8 },
    /// PROGRAM dial of an Extern layer: Bank Select then Program Change.
    ExternProgram { layer: u8, program: u8 },
}

/// Engine id of Synth layer `0..=2`; out-of-range values map to Synth C.
fn layer_id(layer: u8) -> LayerId {
    match layer {
        0 => LayerId::SynthA,
        1 => LayerId::SynthB,
        _ => LayerId::SynthC,
    }
}

/// The three synth layers, in A, B, C order.
fn layers(engine: &mut Stage4Engine) -> [&mut SynthLayer; 3] {
    [
        &mut engine.synth_a,
        &mut engine.synth_b,
        &mut engine.synth_c,
    ]
}

impl SynthEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            Self::Param(SynthTarget::Layer(i), p) => {
                if let Some(l) = layers(engine).into_iter().nth(usize::from(i)) {
                    p.apply(l);
                }
            }
            Self::Param(SynthTarget::All, p) => {
                for l in layers(engine) {
                    p.apply(l);
                }
            }
            Self::SoundInit(i) => {
                let i = usize::from(i);
                if i < 3 {
                    layers(engine)[i].sound_init();
                    engine.synth_cutoff_morph[i] = MorphParameter::new(0.5);
                }
            }
            Self::Sustped(on) => {
                engine.pedals.sustped[SYNTH_FIRST_LAYER..SYNTH_FIRST_LAYER + 3].fill(on);
                engine.refresh_pedal_routing();
            }
            Self::Pstick(on) => {
                engine.pedals.pstick[SYNTH_FIRST_LAYER..SYNTH_FIRST_LAYER + 3].fill(on);
                engine.refresh_pedal_routing();
            }
            Self::Zone { layer, leds } if layer < 3 => {
                engine.set_layer_zone_leds(layer_id(layer), leds)
            }
            Self::AuxKb { layer, on } if layer < 3 => engine.set_layer_aux(layer_id(layer), on),
            Self::Extern { layer, on } => engine.set_layer_extern(usize::from(layer), on),
            Self::ExternCc { layer, slot, value } if layer < 3 && slot < 2 => {
                engine.extern_set_cc_value(usize::from(layer), usize::from(slot), value)
            }
            Self::ExternProgram { layer, program } if layer < 3 => {
                engine.extern_set_program(usize::from(layer), program)
            }
            Self::Zone { .. }
            | Self::AuxKb { .. }
            | Self::ExternCc { .. }
            | Self::ExternProgram { .. } => {}
            Self::Group { src, group } => {
                let [a, b, c] = layers(engine);
                match src {
                    0 => {
                        group.copy(a, b);
                        group.copy(a, c);
                    }
                    1 => {
                        group.copy(b, a);
                        group.copy(b, c);
                    }
                    _ => {
                        group.copy(c, a);
                        group.copy(c, b);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth_types::LfoShape;

    #[test]
    fn param_edit_reaches_only_the_target_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Param(SynthTarget::Layer(1), SynthParam::Cutoff(1234.0)).apply(&mut e);
        assert_eq!(e.synth_b.cutoff_hz, 1234.0);
        assert_ne!(e.synth_a.cutoff_hz, 1234.0);
        assert_ne!(e.synth_c.cutoff_hz, 1234.0);
    }

    #[test]
    fn all_target_reaches_every_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Param(SynthTarget::All, SynthParam::Resonance(0.77)).apply(&mut e);
        assert_eq!(e.synth_a.resonance, 0.77);
        assert_eq!(e.synth_b.resonance, 0.77);
        assert_eq!(e.synth_c.resonance, 0.77);
    }

    #[test]
    fn out_of_range_layer_is_ignored() {
        let mut e = Stage4Engine::new(48_000.0);
        let before = e.synth_a.cutoff_hz;
        SynthEdit::Param(SynthTarget::Layer(9), SynthParam::Cutoff(50.0)).apply(&mut e);
        assert_eq!(e.synth_a.cutoff_hz, before);
    }

    #[test]
    fn group_edit_copies_from_the_source_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_c.lfo_shape = LfoShape::Square;
        SynthEdit::Group {
            src: 2,
            group: SynthGroup::Lfo,
        }
        .apply(&mut e);
        assert_eq!(e.synth_a.lfo_shape, LfoShape::Square);
        assert_eq!(e.synth_b.lfo_shape, LfoShape::Square);
    }

    #[test]
    fn edits_arrive_through_the_audio_command_path() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut master = 0.8;
        crate::audio::apply_audio_command(
            crate::audio::AudioCommand::Synth(SynthEdit::Param(
                SynthTarget::Layer(2),
                SynthParam::Glide(0.5),
            )),
            &mut e,
            &mut master,
        );
        assert_eq!(e.synth_c.glide_sec, 0.5);
    }

    #[test]
    fn sound_init_resets_one_layer_in_place_and_keeps_waveform() {
        use nord_core::synth::VaWaveform;
        let mut e = Stage4Engine::new(48_000.0);
        let fresh_cutoff = e.synth_b.cutoff_hz;
        e.synth_b.cutoff_hz = 321.0;
        e.synth_b.waveform = VaWaveform::Triangle;
        e.synth_b.perf.zigzag = true;
        e.synth_c.cutoff_hz = 654.0;
        e.synth_cutoff_morph[1].delta_wheel = 0.4;
        SynthEdit::SoundInit(1).apply(&mut e);
        assert_eq!(e.synth_b.cutoff_hz, fresh_cutoff);
        assert_eq!(e.synth_b.waveform, VaWaveform::Triangle);
        assert!(!e.synth_b.perf.zigzag);
        assert_eq!(e.synth_c.cutoff_hz, 654.0);
        assert_eq!(e.synth_cutoff_morph[1].delta_wheel, 0.0);
        assert_eq!(e.undo_available(), None, "the light edit takes no snapshot");
        SynthEdit::SoundInit(7).apply(&mut e);
    }

    #[test]
    fn sustped_and_pstick_cover_the_three_synth_layers_only() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Sustped(true).apply(&mut e);
        SynthEdit::Pstick(true).apply(&mut e);
        assert_eq!(e.pedals.sustped[4..7], [true; 3]);
        assert_eq!(e.pedals.pstick[4..7], [true; 3]);
        assert!(!e.pedals.pstick[3] && !e.pedals.sustped[1]);
        SynthEdit::Pstick(false).apply(&mut e);
        assert_eq!(e.pedals.pstick[4..7], [false; 3]);
    }

    #[test]
    fn zone_and_aux_edits_target_one_synth_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Zone {
            layer: 1,
            leds: 0b0100,
        }
        .apply(&mut e);
        assert_eq!(e.layer_zone_leds(LayerId::SynthB), 0b0100);
        assert_eq!(e.layer_zone_leds(LayerId::SynthA), 0b1111);
        assert_eq!(e.layer_zone_leds(LayerId::PianoB), 0b1111);
        SynthEdit::AuxKb { layer: 2, on: true }.apply(&mut e);
        assert!(e.aux_kb.is_aux_layer(LayerId::SynthC));
        assert!(!e.aux_kb.is_aux_layer(LayerId::SynthA));
        SynthEdit::Zone { layer: 9, leds: 1 }.apply(&mut e);
        SynthEdit::AuxKb { layer: 9, on: true }.apply(&mut e);
        assert!(!e.aux_kb.is_aux_layer(LayerId::OrganA));
    }

    #[test]
    fn extern_edits_enable_the_layer_and_transmit() {
        use nord_core::engine::MidiOut;
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Extern { layer: 1, on: true }.apply(&mut e);
        assert!(e.extern_out.layers[1].enabled && !e.extern_out.layers[0].enabled);
        SynthEdit::ExternCc {
            layer: 1,
            slot: 1,
            value: 99,
        }
        .apply(&mut e);
        SynthEdit::ExternProgram {
            layer: 1,
            program: 5,
        }
        .apply(&mut e);
        let out: Vec<_> = std::iter::from_fn(|| e.pop_midi_out()).collect();
        assert_eq!(out[0], MidiOut::control_change(13, 2, 99));
        assert_eq!(out.last(), Some(&MidiOut::program_change(13, 5)));
        SynthEdit::Extern {
            layer: 1,
            on: false,
        }
        .apply(&mut e);
        assert!(!e.extern_out.layers[1].enabled);
        SynthEdit::ExternCc {
            layer: 5,
            slot: 0,
            value: 1,
        }
        .apply(&mut e);
        SynthEdit::ExternProgram {
            layer: 5,
            program: 1,
        }
        .apply(&mut e);
    }
}
