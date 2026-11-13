//! Record types of the control inventory (`docs/specs/control-inventory.md` §2-3).

/// Front-panel area a control belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    /// Master level, wheels, pitch stick and the Rotary Speaker box.
    Master,
    Organ,
    Piano,
    /// Central Program area: OLED, Program buttons, Morph, Split, Preset Library.
    Program,
    Synth,
    /// Layer Effects area (Mod 1/2, Delay, Amp/EQ, Comp, Reverb, FX Focus).
    Effects,
    /// 88-key keybed and the split-position LEDs above it.
    Keybed,
}

/// Physical kind of a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ControlKind {
    /// Potentiometer knob.
    Knob,
    /// Endless dial (Program dial, Piano Select dial, three Synth OLED dials).
    Encoder,
    /// Layer level fader.
    Fader,
    /// Organ drawbar.
    Drawbar,
    /// Push button, including selectors that cycle LEDs.
    Button,
    /// Standalone LED or group of LEDs that is not part of a button.
    Led,
    /// Column of LEDs next to a fader or drawbar.
    LedLadder,
    /// OLED screen.
    Oled,
    /// Modulation wheel.
    Wheel,
    /// Pitch stick.
    Stick,
    /// Keybed.
    Key,
}

/// Layer a per-layer control belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    A,
    B,
    C,
}

/// How the user reaches the control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    /// Plain press, turn or slide.
    Direct,
    /// Held SHIFT plus the control (function printed below the control).
    Shift,
    /// Press and hold the control for about half a second.
    Hold,
}

/// Evidence level, per `.agents/rules/provenance-in-code.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// Named and described in the manual text on `manual_page`.
    Verified,
    /// Derived from a panel photo or recording.
    Empirical,
    /// Exists per the manual but its panel position or access is inferred.
    Assumed,
}

/// What the control drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// Host parameter id (`nord-plugin`); `{n}` stands for the per-layer index.
    Bound(&'static str),
    /// No audio effect; the reason is documented.
    UiOnly(&'static str),
    /// Missing behaviour tracked by a bean id.
    Unsupported(&'static str),
}

/// One front-panel control.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlEntry {
    /// Stable id, `section.group.name[.layer]`, lowercase ASCII, never renamed.
    pub id: String,
    pub section: Section,
    pub kind: ControlKind,
    pub layer: Option<Layer>,
    pub access: Access,
    /// The manual says this control can be a Morph destination (green Morph LED).
    pub morphable: bool,
    /// Printed page of the OS v1.6x manual (equals the PDF page number).
    pub manual_page: u16,
    pub provenance: Provenance,
    /// Source text for the provenance tag.
    pub source: &'static str,
    pub binding: Binding,
    /// Label as printed on the panel or used in the manual.
    pub label: &'static str,
}

impl ControlEntry {
    /// New entry with defaults: direct access, not morphable, `Verified`,
    /// and an empty binding that tests reject until one is set.
    pub fn new(
        id: &str,
        section: Section,
        kind: ControlKind,
        label: &'static str,
        page: u16,
    ) -> Self {
        Self {
            id: id.to_string(),
            section,
            kind,
            layer: None,
            access: Access::Direct,
            morphable: false,
            manual_page: page,
            provenance: Provenance::Verified,
            source: "manual text",
            binding: Binding::Unsupported(""),
            label,
        }
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = Some(layer);
        self
    }

    pub fn shift(mut self) -> Self {
        self.access = Access::Shift;
        self
    }

    pub fn hold(mut self) -> Self {
        self.access = Access::Hold;
        self
    }

    pub fn morph(mut self) -> Self {
        self.morphable = true;
        self
    }

    pub fn assumed(mut self, why: &'static str) -> Self {
        self.provenance = Provenance::Assumed;
        self.source = why;
        self
    }

    pub fn bound(mut self, host_id: &'static str) -> Self {
        self.binding = Binding::Bound(host_id);
        self
    }

    pub fn ui(mut self, reason: &'static str) -> Self {
        self.binding = Binding::UiOnly(reason);
        self
    }

    pub fn todo(mut self, bean: &'static str) -> Self {
        self.binding = Binding::Unsupported(bean);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_fields() {
        let e = ControlEntry::new("organ.on", Section::Organ, ControlKind::Button, "ON", 18)
            .layer(Layer::B)
            .shift()
            .morph()
            .bound("organ_a_enabled");
        assert_eq!(e.id, "organ.on");
        assert_eq!(e.layer, Some(Layer::B));
        assert_eq!(e.access, Access::Shift);
        assert!(e.morphable);
        assert_eq!(e.binding, Binding::Bound("organ_a_enabled"));
        assert_eq!(e.provenance, Provenance::Verified);
    }

    #[test]
    fn assumed_replaces_source() {
        let e = ControlEntry::new("x", Section::Synth, ControlKind::Knob, "X", 1)
            .assumed("photo only")
            .hold()
            .ui("no audio");
        assert_eq!(e.provenance, Provenance::Assumed);
        assert_eq!(e.source, "photo only");
        assert_eq!(e.access, Access::Hold);
        assert_eq!(e.binding, Binding::UiOnly("no audio"));
    }

    #[test]
    fn default_binding_is_empty_unsupported() {
        let e = ControlEntry::new("x", Section::Piano, ControlKind::Led, "X", 1).todo("ns4-4w9l");
        assert_eq!(e.binding, Binding::Unsupported("ns4-4w9l"));
        let d = ControlEntry::new("y", Section::Piano, ControlKind::Led, "Y", 1);
        assert_eq!(d.binding, Binding::Unsupported(""));
    }
}
