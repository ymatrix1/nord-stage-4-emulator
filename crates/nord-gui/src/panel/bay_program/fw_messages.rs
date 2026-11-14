//! The firmware's own OLED messages and the transient toast / confirm mechanism on the Program
//! OLED.
//!
//! Every string is checked against `bank0_CODE.bin` (OS v1.66, local-only under
//! `target/fw_unpacked/`, never committed) by `tools/tests/test_ns4_oled_strings.py::
//! test_message_strings_are_in_the_firmware`; the offsets below are file offsets. Time comes from
//! `ProgramUi::now_ms` (the frame clock the app injects through `begin_frame`), so tests drive the
//! toast lifetime by passing a time instead of sleeping.

use crate::panel::ui_state::ProgramUi;

/// How long a toast stays on the OLED, and how long an `Are you sure?` stays armed.
///
/// ASSUMED: 2000 ms, the same lifetime as every other emulator message; the firmware timing is
/// not measured (needs the ARM display path, bean `ns4-2gdc`).
pub const TOAST_MS: u32 = 2000;

/// The argument the firmware passes for `%c` in `%c Memory Protected`: the code-point `0x89`.
///
/// VERIFIED: `mov r3, #0x89` before `bl 0xC0257AD4` at `0xC003D32C` (Swap, string `0xC02837FC`) and
/// `0xC003D344` (Paste, string `0xC0283790`), `python tools/arm_xref.py dis bank0_CODE.bin
/// 0xC003D2E0 0xC003D360`. It is a glyph of the panel's own OLED font (a symbol preceding the
/// words, ASSUMED to be a padlock from context); the emulator font has no such glyph, so the
/// glyph is left out and only `Memory Protected` is drawn.
#[allow(dead_code)] // documents the decoded argument; no glyph is drawn
pub const MEMORY_PROTECTED_GLYPH: u8 = 0x89;

/// A message the firmware shows on the Program OLED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwMessage {
    /// A program refers to a piano model the library does not hold.
    PianoNotFound,
    /// A program refers to a sample the library does not hold.
    SampleNotFound,
    /// A background load is running.
    PleaseWait,
    /// Layer / FX / Morph / Program pasted.
    Pasted,
    /// Layers swapped.
    Swapped,
    /// Paste with an empty clipboard.
    NothingToPaste,
    /// Swap with an empty clipboard.
    NothingToSwapWith,
    /// Confirmation prompt of a destructive action.
    AreYouSure,
    /// First line of the Paste refusal under Memory Protect.
    UnableToPaste,
    /// A store / organize / paste blocked by Memory Protect (`%c Memory Protected`).
    MemoryProtected,
}

impl FwMessage {
    /// The text as drawn (without the `%c` glyph, see [`MEMORY_PROTECTED_GLYPH`]).
    pub const fn text(self) -> &'static str {
        match self {
            Self::PianoNotFound => "Piano not found",
            Self::SampleNotFound => "Sample not found",
            Self::PleaseWait => "Please wait...",
            Self::Pasted => "Pasted",
            Self::Swapped => "Swapped",
            Self::NothingToPaste => "Nothing to Paste",
            Self::NothingToSwapWith => "Nothing to Swap with",
            Self::AreYouSure => "Are you sure?",
            Self::UnableToPaste => "Unable to Paste",
            Self::MemoryProtected => "Memory Protected",
        }
    }

    /// File offset of the string in `bank0_CODE.bin`. For `MemoryProtected` it is the offset of
    /// `%c Memory Protected`, whose text starts 3 bytes later.
    ///
    /// VERIFIED: `test_message_strings_are_in_the_firmware` reads each string at its offset.
    pub const fn offset(self) -> usize {
        match self {
            Self::PianoNotFound => 0x2CA180,
            Self::SampleNotFound => 0x2CA198,
            Self::PleaseWait => 0x2C75D0,
            Self::Pasted => 0x283758,
            Self::Swapped => 0x283760,
            Self::NothingToPaste => 0x28377D,
            Self::NothingToSwapWith => 0x2837E5,
            Self::AreYouSure => 0x2C0E38,
            Self::UnableToPaste => 0x283790,
            Self::MemoryProtected => 0x2837A0,
        }
    }

    /// Every message, for the tests.
    pub const ALL: [FwMessage; 10] = [
        Self::PianoNotFound,
        Self::SampleNotFound,
        Self::PleaseWait,
        Self::Pasted,
        Self::Swapped,
        Self::NothingToPaste,
        Self::NothingToSwapWith,
        Self::AreYouSure,
        Self::UnableToPaste,
        Self::MemoryProtected,
    ];
}

/// A destructive action that asks `Are you sure?` before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// System menu, Reset all settings (`0x2C0E08`, followed by `Are you sure?` at `0x2C0E38`).
    ResetAllSettings,
}

impl ProgramUi {
    /// Shows a firmware message for [`TOAST_MS`].
    pub fn show(&mut self, m: FwMessage) {
        self.set_message(m.text());
    }

    /// Shows `Please wait...` unless another message is up.
    pub fn show_wait(&mut self) {
        let busy = self
            .message
            .as_ref()
            .is_some_and(|(t, _)| t != FwMessage::PleaseWait.text());
        if !busy {
            self.show(FwMessage::PleaseWait);
        }
    }

    /// Takes `Please wait...` down (the load finished); other messages stay.
    pub fn clear_wait(&mut self) {
        if self
            .message
            .as_ref()
            .is_some_and(|(t, _)| t == FwMessage::PleaseWait.text())
        {
            self.message = None;
        }
    }

    /// Two-press confirmation: the first press shows `Are you sure?` and arms `what` for
    /// [`TOAST_MS`]; the same action pressed again while armed returns `true` and disarms.
    pub fn confirm(&mut self, what: Confirm) -> bool {
        let armed = self
            .confirming
            .is_some_and(|(w, until)| w == what && self.now_ms < until);
        if armed {
            self.confirming = None;
            return true;
        }
        self.confirming = Some((what, self.now_ms.saturating_add(TOAST_MS)));
        self.show(FwMessage::AreYouSure);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_the_firmware_strings() {
        let t: Vec<_> = FwMessage::ALL.iter().map(|m| m.text()).collect();
        assert!(t.contains(&"Piano not found") && t.contains(&"Sample not found"));
        assert!(t.contains(&"Please wait...") && t.contains(&"Nothing to Swap with"));
        assert!(t.contains(&"Are you sure?") && t.contains(&"Unable to Paste"));
        assert_eq!(MEMORY_PROTECTED_GLYPH, 0x89);
    }

    #[test]
    fn toast_lifetime_follows_the_injected_frame_clock() {
        let mut u = ProgramUi::default();
        u.now_ms = 1000;
        u.show(FwMessage::Pasted);
        assert_eq!(u.message, Some(("Pasted".to_string(), 1000 + TOAST_MS)));
        u.begin_frame(1000 + TOAST_MS - 1, &[false; 128], "A:11", "t");
        assert!(u.message.is_some(), "still up one ms before the end");
        u.begin_frame(1000 + TOAST_MS, &[false; 128], "A:11", "t");
        assert!(u.message.is_none(), "gone at the end time");
    }

    #[test]
    fn wait_does_not_hide_other_messages_and_clears_only_itself() {
        let mut u = ProgramUi::default();
        u.show_wait();
        assert_eq!(u.message.as_ref().unwrap().0, "Please wait...");
        u.clear_wait();
        assert!(u.message.is_none());
        u.show(FwMessage::PianoNotFound);
        u.show_wait();
        assert_eq!(u.message.as_ref().unwrap().0, "Piano not found");
        u.clear_wait();
        assert!(u.message.is_some(), "another message stays");
    }

    #[test]
    fn paste_and_swap_flows_show_the_firmware_messages() {
        use crate::audio::CopyItem;
        use crate::panel::bay_program::model::Screen;
        let mut u = ProgramUi::default();
        u.screen = Screen::Paste;
        u.soft_down(2);
        assert_eq!(u.message.as_ref().unwrap().0, "Nothing to Swap with");
        u.soft_down(1);
        assert_eq!(u.message.as_ref().unwrap().0, "Nothing to Paste");
        u.copied = Some(CopyItem::Layer(u.focus));
        u.screen = Screen::Paste;
        u.soft_down(1);
        assert_eq!(u.message.as_ref().unwrap().0, "Pasted");
        u.screen = Screen::Paste;
        u.soft_down(2);
        assert_eq!(u.message.as_ref().unwrap().0, "Swapped");
    }

    #[test]
    fn pasting_a_program_under_memory_protect_is_refused_on_two_lines() {
        use crate::audio::CopyItem;
        use crate::panel::bay_program::model::Screen;
        let mut u = ProgramUi::default();
        u.copied = Some(CopyItem::Program);
        u.menu.set_memory_protect(true);
        u.screen = Screen::Paste;
        assert!(u.soft_down(1).is_empty(), "nothing reaches the engine");
        assert_eq!(
            u.message.as_ref().unwrap().0,
            "Unable to Paste
Memory Protected"
        );
        u.menu.set_memory_protect(false);
        u.screen = Screen::Paste;
        assert_eq!(u.soft_down(1).len(), 1);
        assert_eq!(u.message.as_ref().unwrap().0, "Pasted");
    }

    #[test]
    fn the_oled_draws_each_message_line_as_its_own_row() {
        let mut u = ProgramUi::default();
        u.set_message(
            "Unable to Paste
Memory Protected",
        );
        let t = crate::panel::bay_program::oled::compose(&u, &Default::default());
        assert!(t.rows.ends_with(&[
            "Unable to Paste".to_string(),
            "Memory Protected".to_string()
        ]));
    }

    #[test]
    fn confirm_needs_a_second_press_inside_the_window() {
        let mut u = ProgramUi::default();
        u.now_ms = 100;
        assert!(!u.confirm(Confirm::ResetAllSettings));
        assert_eq!(u.message.as_ref().unwrap().0, "Are you sure?");
        u.now_ms = 100 + TOAST_MS - 1;
        assert!(u.confirm(Confirm::ResetAllSettings));
        assert!(!u.confirm(Confirm::ResetAllSettings), "disarmed after use");
        u.now_ms = 100 + 2 * TOAST_MS + 5;
        assert!(
            !u.confirm(Confirm::ResetAllSettings),
            "expired window re-arms"
        );
    }
}
