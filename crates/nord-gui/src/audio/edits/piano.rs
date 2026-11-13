//! Piano edits applied on the DSP thread (one variant per panel control).
//!
//! Add variants here and handle them in [`PianoEdit::apply`]; `AudioCommand::Piano` carries them.

use nord_core::engine::Stage4Engine;

/// Panel edits for the Piano area.
#[derive(Debug, Clone, PartialEq)]
pub enum PianoEdit {}

impl PianoEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, _engine: &mut Stage4Engine) {
        match *self {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn edit_enum_is_declared_and_empty_enum_is_zero_sized() {
        assert_eq!(std::mem::size_of::<super::PianoEdit>(), 0);
    }
}
