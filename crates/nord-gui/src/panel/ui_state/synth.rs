//! UI-only state of the Synth area (widgets that have no engine field of their own).
//!
//! Add fields here; `Stage4PanelState::synth_ui` holds the struct.

/// UI state for the Synth area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SynthUi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert_eq!(SynthUi::default(), SynthUi {});
    }
}
