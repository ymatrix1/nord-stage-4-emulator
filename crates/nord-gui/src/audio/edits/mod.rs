//! Per-area engine edits sent from the panel (`AudioCommand::{Organ,Piano,Synth,Fx,Program}`).

mod fx;
mod organ;
mod piano;
mod program;
mod settings;
mod synth;

pub use fx::{chain_of_slot, FxEdit, FxFlag, FxFloat, FX_SLOTS};
pub use organ::{OrganEdit, ANGLE_NARROW_DEG, ANGLE_WIDE_DEG};
pub use piano::PianoEdit;
pub use program::{CopyItem, MorphTarget, ProgramEdit, SplitCfg};
pub use settings::SettingsEdit;
pub use synth::{PatternParam, SynthEdit, SynthGroup, SynthParam, SynthTarget};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_area_edit_types_are_exported() {
        let sizes = [
            std::mem::size_of::<OrganEdit>(),
            std::mem::size_of::<PianoEdit>(),
            std::mem::size_of::<SynthEdit>(),
            std::mem::size_of::<FxEdit>(),
            std::mem::size_of::<ProgramEdit>(),
        ];
        assert_eq!(sizes.len(), 5);
    }
}
