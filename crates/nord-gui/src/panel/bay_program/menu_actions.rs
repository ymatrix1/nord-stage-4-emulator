//! Behaviour of the menu screens: open (Shift+Program n), PAGE, soft buttons, dial, Reset, and
//! the Memory Protect gate of STORE (manual OS v1.6x p.57-64).
//!
//! Editing a `Bound` item sends `ProgramEdit::Settings`; a `UiOnly` item keeps its value in
//! `ProgramUi::menu`; an `Unsupported` item does nothing (it is drawn dimmed).

use super::model::{MenuId, Screen};
use super::nav::cmd;
use crate::audio::{AudioCommand, ProgramEdit, SettingsEdit};
use crate::panel::ui_state::{ProgramUi, SoftResult};
use nord_panel::menus::{MenuItem, Value};
use nord_panel::Binding;

/// Item id of System page 5 "Reset All Settings".
const RESET_ALL: &str = "system.5.reset_all";

fn settings_cmd(item: &MenuItem, value: Value) -> Option<AudioCommand> {
    matches!(item.binding, Binding::Bound(_))
        .then(|| cmd(ProgramEdit::Settings(SettingsEdit { id: item.id, value })))
}

impl ProgramUi {
    /// Shift+Program n: opens `m` at its first page.
    pub fn open_menu(&mut self, m: MenuId) {
        self.menu.open(m.model());
        self.screen = Screen::Menu(m);
    }

    /// Soft button `n` (1..=4) on a menu screen: focus the item, scroll, or run an action.
    pub(super) fn menu_soft(&mut self, n: u8) -> Vec<AudioCommand> {
        if self.on_organize_page() {
            return self.organize_soft(n);
        }
        match self.menu.soft(usize::from(n).saturating_sub(1)) {
            SoftResult::Action(item) if item.id == RESET_ALL => self.menu_reset(),
            SoftResult::Action(_) => {
                self.set_message("Not supported yet");
                Vec::new()
            }
            SoftResult::None | SoftResult::Focused | SoftResult::More => Vec::new(),
        }
    }

    /// PROGRAM dial on a menu screen: edits the focused item.
    pub(super) fn menu_dial(&mut self, delta: i32) -> Vec<AudioCommand> {
        match self.menu.dial(delta) {
            Some((item, value)) => settings_cmd(&item, value).into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// Reset All Settings: every item back to its default, sent to the engine.
    fn menu_reset(&mut self) -> Vec<AudioCommand> {
        self.set_message("All settings reset");
        self.menu
            .reset_all()
            .into_iter()
            .filter_map(|(item, value)| settings_cmd(&item, value))
            .collect()
    }

    /// STORE to a bank location is blocked while Memory Protect is on. Live programs and the Sound
    /// Manager are exempt (manual p.57), so only a `Bank` target asks.
    pub(super) fn store_blocked(&mut self) -> bool {
        if !self.menu.memory_protect() {
            return false;
        }
        self.set_message("Memory Protect is on");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::apply_audio_command;
    use nord_core::engine::Stage4Engine;

    fn run(cmds: Vec<AudioCommand>, e: &mut Stage4Engine) {
        let mut vol = 1.0;
        for c in cmds {
            apply_audio_command(c, e, &mut vol);
        }
    }

    #[test]
    fn open_menu_resets_to_page_one_and_selects_the_screen() {
        let mut u = ProgramUi::default();
        u.open_menu(MenuId::Midi);
        u.page(2, false);
        assert_eq!(u.menu.page_index(), 2);
        u.open_menu(MenuId::Midi);
        assert_eq!(
            (u.screen, u.menu.page_index()),
            (Screen::Menu(MenuId::Midi), 0)
        );
    }

    #[test]
    fn dial_on_a_bound_item_sends_a_settings_edit_that_reaches_the_engine() {
        let mut u = ProgramUi::default();
        let mut e = Stage4Engine::new(48_000.0);
        u.open_menu(MenuId::System);
        u.page(1, false);
        let out = u.dial(4, false);
        assert_eq!(out.len(), 1);
        run(out, &mut e);
        assert_eq!(e.global_settings().transpose, 4);
        assert!(u.dial(0, false).is_empty());
    }

    #[test]
    fn ui_only_items_keep_their_value_without_a_command() {
        let mut u = ProgramUi::default();
        u.open_menu(MenuId::System);
        u.page(3, false); // LED Intensity
        assert!(u.dial(-1, false).is_empty());
        let item = *u.menu.focused_item();
        assert_eq!(u.menu.value(&item), Value::Index(0), "Low");
    }

    #[test]
    fn reset_all_sends_the_defaults_and_keeps_memory_protect() {
        let mut u = ProgramUi::default();
        let mut e = Stage4Engine::new(48_000.0);
        u.open_menu(MenuId::System);
        u.dial(-1, false); // Memory Protect off
        u.page(1, false);
        run(u.dial(5, false), &mut e);
        assert_eq!(e.global_settings().transpose, 5);
        u.page(3, false); // Reset All Settings
        let out = u.menu_soft(1);
        assert!(!out.is_empty());
        run(out, &mut e);
        assert_eq!(e.global_settings().transpose, 0);
        assert!(!u.menu.memory_protect());
        assert_eq!(u.message.as_ref().unwrap().0, "All settings reset");
    }

    #[test]
    fn memory_protect_gate_names_itself_and_follows_the_menu_value() {
        let mut u = ProgramUi::default();
        assert!(u.store_blocked(), "factory default is protected");
        assert_eq!(u.message.as_ref().unwrap().0, "Memory Protect is on");
        u.open_menu(MenuId::System);
        u.dial(-1, false);
        u.message = None;
        assert!(!u.store_blocked());
        assert!(u.message.is_none());
    }

    #[test]
    fn unsupported_action_says_so() {
        let mut u = ProgramUi::default();
        u.open_menu(MenuId::Organize);
        u.page(1, false); // Preset Organize has no engine behind it yet
        assert!(u.menu_soft(1).is_empty());
        assert_eq!(u.message.as_ref().unwrap().0, "Not supported yet");
    }
}
