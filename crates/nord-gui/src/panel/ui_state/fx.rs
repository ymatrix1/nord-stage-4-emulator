//! UI-only state of the Fx area (widgets that have no engine field of their own).
//!
//! Add fields here; `Stage4PanelState::fx_ui` holds the struct.

/// UI state for the Fx area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FxUi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert_eq!(FxUi::default(), FxUi {});
    }
}
