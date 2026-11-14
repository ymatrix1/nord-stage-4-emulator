//! Per-area UI-only panel state, one file per area so bay work does not collide.

pub mod fx;
mod menu;
mod organ;
mod piano;
mod program;
mod synth;

pub use fx::FxUi;
pub use menu::{step_value, MenuUi, SoftResult};
pub use organ::OrganUi;
pub use piano::{inventory_unsupported, slot, PianoOverlay, PianoUi};
pub use program::ProgramUi;
pub use synth::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_area_ui_types_default() {
        let _ = (
            OrganUi::default(),
            PianoUi::default(),
            SynthUi::default(),
            FxUi::default(),
            ProgramUi::default(),
        );
    }
}
