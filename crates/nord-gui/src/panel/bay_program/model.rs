//! Plain enums of the Program area: display views, OLED screens, menus, support levels.
//!
//! No egui here, so the state machines in the sibling files are testable without a frame.

use nord_core::morph::MorphSource;

/// PROG VIEW: the four display modes (manual p.42).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProgView {
    /// Program name and number large, parameter hints below.
    #[default]
    Name,
    /// Small name plus the selection of each focused Layer.
    Layers,
    /// Complete Program configuration.
    Full,
    /// All eight Programs of the current Page.
    Page,
}

impl ProgView {
    /// Next view (PROG VIEW press).
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Layers,
            Self::Layers => Self::Full,
            Self::Full => Self::Page,
            Self::Page => Self::Name,
        }
    }
}

/// LIST view sort mode (manual p.41): Num, Abc, Cat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListMode {
    #[default]
    Num,
    Abc,
    Cat,
}

/// Keyboard Sync of the Master Clock (manual p.40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KbsMode {
    #[default]
    Off,
    On,
    Soft,
}

impl KbsMode {
    /// Next mode (KBS soft button).
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Soft,
            Self::Soft => Self::Off,
        }
    }

    /// Label as the manual names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
            Self::Soft => "Soft",
        }
    }
}

/// Preset Library section (manual p.41-42).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    Organ,
    Piano,
    Synth,
}

impl PresetKind {
    /// Title for the display.
    pub fn title(self) -> &'static str {
        match self {
            Self::Organ => "ORGAN PRESET",
            Self::Piano => "PIANO PRESET",
            Self::Synth => "SYNTH PRESET",
        }
    }
}

/// The eight menus behind Shift+Program 1..8 (manual p.44, p.57-63).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    System,
    Sound,
    Organize,
    AuxKb,
    Output,
    Pedal,
    Midi,
    Extern,
}

impl MenuId {
    /// All menus in Program-button order.
    pub const ALL: [MenuId; 8] = [
        Self::System,
        Self::Sound,
        Self::Organize,
        Self::AuxKb,
        Self::Output,
        Self::Pedal,
        Self::Midi,
        Self::Extern,
    ];

    /// Menu for Program button `n` (1..=8).
    pub fn from_button(n: u8) -> Option<Self> {
        Self::ALL.get(usize::from(n).checked_sub(1)?).copied()
    }

    /// The matching menu of the data model (`nord_panel::menus`).
    pub fn model(self) -> nord_panel::menus::Menu {
        use nord_panel::menus::Menu;
        match self {
            Self::System => Menu::System,
            Self::Sound => Menu::Sound,
            Self::Organize => Menu::Organize,
            Self::AuxKb => Menu::AuxKb,
            Self::Output => Menu::Output,
            Self::Pedal => Menu::Pedal,
            Self::Midi => Menu::Midi,
            Self::Extern => Menu::Extern,
        }
    }

    /// Page title as the OLED shows it.
    ///
    /// VERIFIED (frames of the ARM emulator, `docs/research/arm-oled-render.md` section 8): the
    /// settings menus draw `AUX KB SETTINGS`, `MIDI SETTINGS` and `EXTERN SETTINGS` in the
    /// inverted title bar (the firmware strings are `Aux Kb Settings` ...; the bar's upper-case
    /// flag capitalises them), and Program Organize draws `Program` left and `Organize` right in
    /// mixed case. ASSUMED: `SYSTEM`, `SOUND`, `OUTPUT` and `PEDAL SETTINGS` follow the same table
    /// (`0xC02D3584..0xC02D35E4`), not captured because their Shift+Program key is not identified.
    pub fn title(self) -> &'static str {
        match self {
            Self::System => "SYSTEM SETTINGS",
            Self::Sound => "SOUND SETTINGS",
            Self::Organize => "Program Organize",
            Self::AuxKb => "AUX KB SETTINGS",
            Self::Output => "OUTPUT SETTINGS",
            Self::Pedal => "PEDAL SETTINGS",
            Self::Midi => "MIDI SETTINGS",
            Self::Extern => "EXTERN SETTINGS",
        }
    }

    /// Manual page that documents the menu (OS v1.6x).
    pub fn manual_page(self) -> u16 {
        match self {
            Self::System => 57,
            Self::Sound => 58,
            Self::Organize => 45,
            Self::AuxKb => 44,
            Self::Output => 59,
            Self::Pedal => 60,
            Self::Midi => 62,
            Self::Extern => 46,
        }
    }
}

/// What the OLED shows. Screens with soft buttons turn Program buttons 1-4 into soft buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Program name and number in the current PROG VIEW mode.
    #[default]
    Home,
    /// LIST view of the program catalog (Shift+dial).
    List,
    /// A menu page (`ProgramUi::menu` holds the page, focus and values).
    Menu(MenuId),
    /// STORE PROGRAM TO: choose the location.
    StoreTo,
    /// STORE PROGRAM AS: naming with ABC / Cat / Del / Ins.
    StoreAs,
    /// The program list `Import into A:11` (Program dial, `import.rs`).
    Import,
    /// Keyboard Split screen (hold SPLIT ON/SET).
    Split,
    /// Master Clock tempo and KBS page.
    Clock,
    /// Transposition value (hold TRANSP).
    Transpose,
    /// Preset Library browser; `single` loads one Layer (Shift+Piano/Synth).
    Preset { kind: PresetKind, single: bool },
    /// UNDO confirmation.
    Undo,
    /// LAYER INIT choice.
    LayerInit,
    /// MON/COPY choice.
    Copy,
    /// PASTE / SWAP choice.
    Paste,
    /// Morph source being viewed or assigned.
    Morph(MorphSource),
    /// NUM PAD digit entry.
    NumPad,
}

impl Screen {
    /// Soft button labels (Program buttons 1-4) of this screen; empty when it has none.
    pub fn soft_labels(self) -> [&'static str; 4] {
        match self {
            Self::List => ["Num", "Abc", "Cat", ""],
            Self::StoreAs => ["Abc", "Cat", "Del", "Ins"],
            Self::Import => ["Ok", "", "", "Cancel"],
            Self::Split => ["Note", "Low", "Mid", "High"],
            Self::Clock => ["KBS", "", "", ""],
            Self::Preset { .. } => ["Num", "Cat", "Cancel", ""],
            Self::Undo => ["", "", "Redo", "Ok"],
            Self::LayerInit => ["All", "Organ", "Piano", "Synth"],
            Self::Copy => ["Layer", "Fx", "Morph", "Prog"],
            Self::Paste => ["Paste", "Swap", "", ""],
            // Item rows 1-4; `menu_screen` blanks the rows a page does not have.
            Self::Menu(_) => ["1", "2", "3", "4"],
            _ => ["", "", "", ""],
        }
    }

    /// Program buttons 1-4 act as soft buttons on this screen.
    pub fn has_soft_buttons(self) -> bool {
        self.soft_labels().iter().any(|l| !l.is_empty())
    }
}

/// How well a control is backed (drives dimming and the tooltip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Works against the engine or the UI as the manual describes.
    Full,
    /// Works, with a documented limit (tooltip only, drawn normally).
    Partial(&'static str),
    /// No engine or UI behind it yet: drawn dimmed with this reason as tooltip.
    Unsupported(&'static str),
}

impl Support {
    /// Drawn dimmed.
    pub fn is_dimmed(self) -> bool {
        matches!(self, Self::Unsupported(_))
    }

    /// Tooltip text, if any.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::Full => None,
            Self::Partial(n) | Self::Unsupported(n) => Some(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prog_view_cycles_through_four_modes() {
        let mut v = ProgView::Name;
        let mut seen = vec![v];
        for _ in 0..3 {
            v = v.next();
            seen.push(v);
        }
        assert_eq!(seen.len(), 4);
        assert_eq!(v.next(), ProgView::Name);
    }

    #[test]
    fn menus_map_from_program_buttons() {
        assert_eq!(MenuId::from_button(1), Some(MenuId::System));
        assert_eq!(MenuId::from_button(8), Some(MenuId::Extern));
        assert_eq!(MenuId::from_button(0), None);
        assert_eq!(MenuId::from_button(9), None);
        assert!(MenuId::ALL.iter().all(|m| m.manual_page() >= 44));
    }

    #[test]
    fn soft_buttons_exist_on_the_manual_screens() {
        assert!(Screen::StoreAs.has_soft_buttons());
        assert!(Screen::Split.has_soft_buttons());
        assert!(!Screen::Home.has_soft_buttons());
        assert_eq!(Screen::Split.soft_labels()[1], "Low");
        assert!(Screen::Menu(MenuId::Midi).has_soft_buttons());
        for m in MenuId::ALL {
            let model = m.model();
            assert_eq!(nord_panel::menus::pages_of(model)[0].menu, model);
        }
    }

    #[test]
    fn kbs_cycles_and_support_flags() {
        assert_eq!(KbsMode::Off.next().next().next(), KbsMode::Off);
        assert_eq!(KbsMode::Soft.label(), "Soft");
        assert!(Support::Unsupported("x").is_dimmed());
        assert!(!Support::Partial("x").is_dimmed());
        assert_eq!(Support::Full.note(), None);
        assert_eq!(PresetKind::Piano.title(), "PIANO PRESET");
    }
}
