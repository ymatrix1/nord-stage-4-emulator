//! UI-only state of the Piano area (widgets that have no engine field of their own).
//!
//! Add fields here; `Stage4PanelState::piano_ui` holds the struct.

/// UI state for the Piano area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PianoUi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert_eq!(PianoUi::default(), PianoUi {});
    }
}
