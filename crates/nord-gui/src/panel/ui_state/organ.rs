//! UI-only state of the Organ area (widgets that have no engine field of their own).
//!
//! Add fields here; `Stage4PanelState::organ_ui` holds the struct.

/// UI state for the Organ area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrganUi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert_eq!(OrganUi::default(), OrganUi {});
    }
}
