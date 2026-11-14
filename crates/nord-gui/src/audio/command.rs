//! `AudioCommand`: messages from the UI and MIDI threads to the real-time DSP thread.

use super::edits::{FxEdit, OrganEdit, PianoEdit, ProgramEdit, SynthEdit};
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::midi::MidiEvent;
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};
use std::sync::Arc;

/// Commands sent from the UI/MIDI to the real-time DSP thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Organ-area panel edit (see `audio/edits/organ.rs`).
    Organ(OrganEdit),
    /// Piano-area panel edit.
    Piano(PianoEdit),
    /// Synth-area panel edit.
    Synth(SynthEdit),
    /// Layer-effects panel edit.
    Fx(FxEdit),
    /// Program-area panel edit.
    Program(ProgramEdit),
    /// MIDI from outside (a MIDI input port): plays the engines whatever Local Control says.
    Midi(MidiEvent),
    /// Event from the instrument's own keyboard / panel (on-screen keybed, wheels): goes through
    /// `Stage4Engine::handle_keyboard_midi`, so Local Control Off only transmits it.
    Keyboard(MidiEvent),
    SetMasterVolume(f32),
    SetDrawbars([u8; 9]),
    SetOrganModel(OrganModel),
    SetScannerMode(ScannerMode),
    SetScannerOn(bool),
    SetPercussion {
        enabled: bool,
        soft: bool,
        fast: bool,
        third: bool,
    },
    SetRotarySpeed(RotarySpeed),
    SetRotaryDrive(f32),
    SetRotaryEnabled(bool),
    SetOrganLevel(f32),
    SetOrganBLevel(f32),
    SetPianoLevel(f32),
    SetPianoBLevel(f32),
    SetPianoCategory(PianoCategory),
    SetPianoTimbre(PianoTimbre),
    SetPianoTouch(VelocityTouch),
    SetPianoAcoustics {
        string_res: bool,
        pedal_noise: bool,
    },
    SetSynthLevel(f32),
    SetSynthBLevel(f32),
    SetSynthCLevel(f32),
    SetSynthOscMode(SynthOscMode),
    SetSynthWaveform(VaWaveform),
    SetSynthDetune(f32),
    SetSynthFilterModel(FilterModel),
    SetSynthCutoff(f32),
    SetSynthResonance(f32),
    SetSynthDrive(f32),
    SetSynthAttack(f32),
    SetSynthDecay(f32),
    SetSynthRelease(f32),
    SetFxMod1 {
        kind: Mod1Type,
        rate: f32,
        amount: f32,
    },
    SetFxMod2 {
        kind: Mod2Type,
        rate: f32,
        amount: f32,
    },
    SetFxEq {
        drive: f32,
        bass: f32,
        mid: f32,
        mid_freq: f32,
        treble: f32,
    },
    SetFxDelay {
        enabled: bool,
        feedback: f32,
        mix: f32,
    },
    SetFxReverb {
        model: ReverbModel,
        mix: f32,
    },
    SetMasterComp(f32),
    /// Activates/deactivates one of the 7 layers (index 0..=6) in both scenes.
    SetLayerActive(usize, bool),
    /// Loads a complete ProgramDecoded into the DSP engine.
    LoadProgram(Box<nord_core::cbin::ProgramDecoded>),
    /// Swaps the pre-decoded `.npno` or SFZ piano library on Piano A (`slot_b = false`) or B (`slot_b = true`).
    LoadPianoSound {
        slot_b: bool,
        sound: Option<Arc<nord_core::piano::NpnoSound>>,
    },
    /// Swaps the pre-decoded `.nsmp4` multisample bank on Synth A (`0`), B (`1`), or C (`2`).
    LoadSynthSample {
        layer_idx: usize,
        sound: Option<Arc<nord_core::synth::SynthSampleSound>>,
    },
    /// Swaps prepared program state into the engine without allocating (see `audio/control`).
    SwapState(super::control::SwapState),
    /// Sets every opt-in firmware path at once (A/B switching, see `firmware_panel`).
    SetFirmwareOptions(nord_core::render_opts::FirmwareOptions),
}

impl AudioCommand {
    /// `true` when the command changes the sound of the program being played, so the display
    /// marks the program as edited (boxed `E`). Loads, recalls, snapshots, copies, MIDI, global
    /// settings and the master volume are not edits.
    ///
    /// ASSUMED: every panel setter of the sound areas counts, including no-op writes of the same
    /// value; the hardware may filter unchanged values. Confirm with footage of a re-tapped button.
    pub fn is_program_edit(&self) -> bool {
        use ProgramEdit as P;
        match self {
            Self::Organ(_) | Self::Piano(_) | Self::Synth(_) | Self::Fx(_) => true,
            Self::Program(p) => matches!(
                p,
                P::Transpose(_)
                    | P::LayerInit(_)
                    | P::Undo
                    | P::Redo
                    | P::Paste(_)
                    | P::Swap(..)
                    | P::ClearMorph(_)
                    | P::MorphDepth { .. }
                    | P::Split(_)
                    | P::MasterBpm(_)
                    | P::LayerScene2(_)
            ),
            Self::Midi(_)
            | Self::SetMasterVolume(_)
            | Self::LoadProgram(_)
            | Self::LoadPianoSound { .. }
            | Self::LoadSynthSample { .. }
            | Self::SwapState(_)
            | Self::SetFirmwareOptions(_) => false,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_are_sound_changes_and_loads_are_not() {
        assert!(AudioCommand::SetSynthCutoff(0.5).is_program_edit());
        assert!(AudioCommand::SetLayerActive(2, true).is_program_edit());
        assert!(AudioCommand::Program(ProgramEdit::Transpose(2)).is_program_edit());
        assert!(!AudioCommand::SetMasterVolume(0.5).is_program_edit());
        assert!(!AudioCommand::Program(ProgramEdit::Panic).is_program_edit());
        assert!(
            !AudioCommand::Program(ProgramEdit::Copy(crate::audio::CopyItem::Layer(
                nord_core::morph::LayerId::PianoA
            )))
            .is_program_edit()
        );
        assert!(!AudioCommand::LoadPianoSound {
            slot_b: false,
            sound: None
        }
        .is_program_edit());
    }

    #[test]
    fn test_command_is_clone_and_debug() {
        let cmd = AudioCommand::SetMasterVolume(0.25);
        let copy = cmd.clone();
        assert!(format!("{copy:?}").contains("SetMasterVolume"));
    }
}
