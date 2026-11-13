//! Per-area UI-only panel state, one file per area so bay work does not collide.

mod fx;
mod organ;
mod piano;
mod program;
mod synth;

pub use fx::FxUi;
pub use organ::OrganUi;
pub use piano::PianoUi;
pub use program::ProgramUi;
pub use synth::SynthUi;

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
