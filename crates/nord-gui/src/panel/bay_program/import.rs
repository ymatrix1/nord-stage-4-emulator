//! The program list `Import into A:11`: the Program dial's browse list of the real unit.
//!
//! VERIFIED (ARM frames, research note s16.5 / s17.5): turning the Program dial on the home screen
//! opens an inverted bar with the list icon and `Import into A:11`, four rows `A:26 <empty>` (the
//! selected one lit, second row), soft buttons `Ok` (first slot) and `Cancel` (fourth slot) and a
//! blank display 1. The manual (OS v1.6x) does not describe importing, so what `Ok` does is
//! ASSUMED: it loads the chosen program into the edit buffer of the location in the title (the
//! location keeps its place; STORE writes it), `Cancel` closes the list.
//!
//! The dial opens the list only when [`ImportFlow::on_dial`] is set; by default the dial keeps
//! stepping the programs of the home screen (the manual's behaviour, p.42), and the list is also
//! reachable with [`ProgramUi::open_import`].

use super::model::Screen;
use super::oled::{name_at, OledText};
use super::oled_icons::ICON_LIST;
use super::oled_lists::{entry_text, window_start, LIST_ROWS, SEL_ROW};
use super::store::{ProgLoc, TOTAL};
use crate::audio::AudioCommand;
use crate::panel::ui_state::ProgramUi;

/// State of the import list.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportFlow {
    /// Location the lit row points at (the program to import).
    pub cursor: ProgLoc,
    /// The Program dial on the home screen opens the list (the measured behaviour of the unit).
    pub on_dial: bool,
}

impl ProgramUi {
    /// Open the import list on the current location.
    pub fn open_import(&mut self) {
        self.import.cursor = self.loc;
        self.screen = Screen::Import;
    }

    /// A program list is on screen: display 1 (the Synth OLED) is blank (ARM frames, s16.5).
    pub fn display1_blank(&self) -> bool {
        match self.screen {
            Screen::List | Screen::Import => true,
            Screen::Menu(_) => self.on_organize_page(),
            _ => false,
        }
    }

    pub(super) fn import_dial(&mut self, delta: i32) -> Vec<AudioCommand> {
        self.import.cursor = self.import.cursor.step(delta);
        Vec::new()
    }

    /// Soft button `n` of the import list: 1 = Ok, 4 = Cancel.
    pub(super) fn import_soft(&mut self, n: u8) -> Vec<AudioCommand> {
        match n {
            1 => self.import_ok(),
            4 => {
                self.screen = Screen::Home;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn import_ok(&mut self) -> Vec<AudioCommand> {
        let (src, dst) = (self.import.cursor, self.loc);
        self.screen = Screen::Home;
        if src == dst {
            return Vec::new();
        }
        if self.name_of(src).is_empty() {
            self.set_message("Nothing to import");
            return Vec::new();
        }
        let out = self.select_loc(src);
        // The sound is loaded; the location (and its place in the lists) stays the target's.
        self.loc = dst;
        self.browse = dst;
        self.set_message(format!("Imported {} into {}", src.label(), dst.label()));
        out
    }

    /// Name at `loc` in the library or the backup (empty when nothing is there).
    fn name_of(&self, loc: ProgLoc) -> String {
        self.library
            .name_at(loc)
            .unwrap_or_else(|| name_at(self, loc))
    }
}

/// OLED text of the import list.
pub fn compose(ui: &ProgramUi) -> OledText {
    let cursor = usize::from(ui.import.cursor.index());
    let total = usize::from(TOTAL);
    let (start, sel) = window_start(total, cursor, SEL_ROW);
    let rows = (start..total.min(start + LIST_ROWS))
        .map(|i| {
            let loc = ProgLoc::from_index(i as u16);
            entry_text(loc, &ui.name_of(loc))
        })
        .collect();
    OledText {
        bar: Some(String::new()),
        big: format!("{ICON_LIST}Import into {}", ui.loc.label()),
        rows,
        sel: Some(sel),
        soft: ui.screen.soft_labels(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::bay_program::model::MenuId;
    use crate::panel::bay_program::store::UserProgram;
    use nord_core::engine::Stage4Engine;

    fn loc(s: &str) -> ProgLoc {
        ProgLoc::parse(s).unwrap()
    }

    fn ui_with(names: &[(&str, &str)]) -> ProgramUi {
        let mut u = ProgramUi::default();
        for (l, n) in names {
            u.library.put(
                loc(l),
                UserProgram {
                    name: (*n).into(),
                    category: 0,
                    state: Stage4Engine::new(48_000.0).capture_state(),
                },
            );
        }
        u
    }

    #[test]
    fn the_dial_opens_the_list_only_when_enabled_and_steps_the_location() {
        let mut u = ui_with(&[]);
        u.dial(1, false);
        assert_eq!(u.screen, Screen::Home, "off by default: the dial steps programs");
        assert_eq!(u.loc.label(), "A:12");
        let mut u = ui_with(&[]);
        u.import.on_dial = true;
        u.dial(1, false);
        assert_eq!(u.screen, Screen::Import);
        assert_eq!(u.import.cursor.label(), "A:12");
        assert_eq!(u.loc.label(), "A:11", "the target stays put");
        u.dial(7, false);
        assert_eq!(u.import.cursor.label(), "A:21", "digits run 1 to 8");
    }

    #[test]
    fn the_screen_matches_the_measured_frame() {
        let mut u = ui_with(&[("A:12", "Two")]);
        u.open_import();
        u.dial(1, false);
        let t = compose(&u);
        assert_eq!(t.big, format!("{ICON_LIST}Import into A:11"));
        assert_eq!(t.bar.as_deref(), Some(""));
        assert_eq!(t.rows[0], "A:11 <empty>");
        assert_eq!(t.rows[1], "A:12 Two");
        assert_eq!((t.rows.len(), t.sel), (4, Some(1)));
        assert_eq!(t.soft, ["Ok", "", "", "Cancel"]);
        assert!(u.display1_blank());
    }

    #[test]
    fn ok_loads_the_chosen_program_into_the_location_and_cancel_closes() {
        let mut u = ui_with(&[("A:13", "Three")]);
        u.open_import();
        u.dial(2, false);
        let out = u.soft_down(1);
        assert_eq!(u.screen, Screen::Home);
        assert_eq!(out.len(), 1, "one recall of the chosen program");
        assert_eq!((u.loc.label().as_str(), u.title.as_str()), ("A:11", "Three"));
        assert!(u.message.as_ref().unwrap().0.contains("A:13 into A:11"));
        u.open_import();
        u.dial(5, false);
        assert!(u.soft_down(4).is_empty());
        assert_eq!(u.screen, Screen::Home);
        assert_eq!(u.title, "Three");
    }

    #[test]
    fn ok_on_an_empty_row_or_the_same_location_changes_nothing() {
        let mut u = ui_with(&[]);
        u.open_import();
        assert!(u.soft_down(1).is_empty());
        assert_eq!(u.screen, Screen::Home, "same location just closes");
        u.open_import();
        u.dial(3, false);
        u.soft_down(1);
        assert_eq!(u.message.as_ref().unwrap().0, "Nothing to import");
        assert_eq!(u.title, "Nord Stage 4");
    }

    #[test]
    fn display_1_is_blank_for_every_program_list_only() {
        let mut u = ui_with(&[]);
        assert!(!u.display1_blank());
        u.screen = Screen::List;
        assert!(u.display1_blank());
        u.open_import();
        assert!(u.display1_blank());
        u.screen = Screen::Clock;
        assert!(!u.display1_blank());
        u.open_menu(MenuId::Organize);
        assert!(u.display1_blank(), "the Organize page is a program list");
        u.open_menu(MenuId::System);
        assert!(!u.display1_blank());
    }
}
