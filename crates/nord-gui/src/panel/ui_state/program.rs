//! UI-only state of the Program area (widgets that have no engine field of their own).
//!
//! Add fields here; `Stage4PanelState::program_ui` holds the struct.

/// UI state for the Program area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgramUi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert_eq!(ProgramUi::default(), ProgramUi {});
    }
}
