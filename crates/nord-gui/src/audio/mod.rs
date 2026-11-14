//! Real-Time Audio Engine Host Thread & CPAL Output Callback & MIDIR Input.
//!
//! Submodules: `command` (message enum), `handle` (UI-side sender), `frame` (sample-format
//! helpers), `apply` (command dispatch on the DSP thread), `io` (CPAL output + MIDIR input).

mod apply;
mod command;
pub mod control;
mod edits;
mod frame;
mod handle;
#[cfg(feature = "app")]
mod io;
mod standalone;

pub use apply::apply_audio_command;
pub use command::AudioCommand;
pub use edits::{
    chain_of_slot, CopyItem, FxEdit, FxFlag, FxFloat, MorphTarget, OrganEdit, PatternParam,
    PianoEdit, ProgramEdit, SettingsEdit, SplitCfg, SynthEdit, SynthGroup, SynthParam, SynthTarget,
    ANGLE_NARROW_DEG, ANGLE_WIDE_DEG, FX_SLOTS,
};
pub use frame::{f32_to_i16, render_frame_to_f32, render_frame_to_i16};
pub use handle::AudioHandle;
#[cfg(feature = "app")]
pub use io::{handle_midi_bytes, start_audio_and_midi};
pub use standalone::standalone_engine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_is_reexported() {
        let (handle, _rx) = AudioHandle::new();
        assert!(handle.send_command(AudioCommand::SetMasterVolume(0.5)));
        assert_eq!(f32_to_i16(1.0), 32767);
    }
}
