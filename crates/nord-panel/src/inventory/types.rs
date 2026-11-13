//! Record types for the control inventory (spec §2-§3).

/// Stable control identifier, `Section_Layer_Name` (e.g. `OrganA_Drawbar3`). Never renamed once
/// shipped. ASSUMED: a string newtype in the skeleton; B2 may replace it with a generated enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlId(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Master,
    Organ,
    Piano,
    Program,
    Synth,
    Effects,
    Keybed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlKind {
    Knob,
    Encoder,
    Fader,
    Drawbar,
    Button,
    Selector,
    Led,
    LedLadder,
    Oled,
    Wheel,
    Stick,
    Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    A,
    B,
    C,
}

/// Provenance tag per `.agents/rules/provenance-in-code.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    Verified,
    Empirical,
    Assumed,
}

/// What a control drives (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// Host parameter id / `AudioHandle` field.
    Bound(&'static str),
    /// Reason the control has no audio effect.
    UiOnly(&'static str),
    /// Bean id tracking the missing behaviour.
    Unsupported(&'static str),
}

/// One front-panel control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlEntry {
    pub id: ControlId,
    pub section: Section,
    pub kind: ControlKind,
    pub layer: Option<Layer>,
    /// Page in the OS v1.x manual describing the control.
    pub manual_page: u16,
    pub provenance: Provenance,
    /// Source text backing `provenance`.
    pub source: &'static str,
    pub binding: Binding,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binding_variants_are_distinct() {
        assert_ne!(Binding::Bound("x"), Binding::UiOnly("x"));
        assert_ne!(Binding::UiOnly("x"), Binding::Unsupported("x"));
        assert_eq!(ControlId("a"), ControlId("a"));
    }
}
