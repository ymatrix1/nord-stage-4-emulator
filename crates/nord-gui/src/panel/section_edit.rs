//! SECTION EDIT (manual p.43): while the button is held or sticky, an edit of a control reaches
//! every Layer of its Section instead of the focused one.
//!
//! One rule for the Organ, Piano and Synth bays: a per-layer edit loops over
//! [`Stage4PanelState::edit_layers`], which is the focused layer alone or the whole section. The
//! Synth bay folds the same flag into `SynthUi::edit` (the GROUP path) and the Effects bay into
//! `FxUi::after_edit`, so every control of those two bays inherits it.

use super::prelude::*;
use std::ops::Range;

impl Stage4PanelState {
    /// SECTION EDIT is held or latched (`ProgramUi::section_edit_active`).
    pub(super) fn section_edit(&self) -> bool {
        self.program_ui.section_edit_active()
    }

    /// Layers an edit of `focus` writes, in a section of `count` layers: all of them while
    /// SECTION EDIT is active, otherwise `focus` alone.
    pub(super) fn edit_layers(&self, focus: usize, count: usize) -> Range<usize> {
        layers_for(focus, count, self.section_edit())
    }
}

/// Layer range of an edit: the whole section of `count` layers or just `focus` (clamped).
pub(super) fn layers_for(focus: usize, count: usize, section_edit: bool) -> Range<usize> {
    if section_edit {
        0..count
    } else {
        let f = focus.min(count.saturating_sub(1));
        f..f + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::behaviour::Trigger;

    #[test]
    fn an_edit_reaches_the_focus_alone_or_the_whole_section() {
        assert_eq!(layers_for(1, 3, false), 1..2);
        assert_eq!(layers_for(1, 3, true), 0..3);
        assert_eq!(layers_for(9, 2, false), 1..2, "focus is clamped");
    }

    #[test]
    fn the_state_reads_the_program_latch() {
        let mut s = Stage4PanelState::default();
        assert_eq!(s.edit_layers(1, 2), 1..2);
        s.program_ui
            .activate("program.section_edit", Trigger::Latch);
        assert!(s.section_edit());
        assert_eq!(s.edit_layers(1, 2), 0..2);
    }
}
