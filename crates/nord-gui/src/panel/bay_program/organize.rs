//! Organize (Shift+Program 3, manual OS v1.6x p.45-46): Swap and Move programs in the in-memory
//! library.
//!
//! Flow on the Program Organize page: the PROGRAM dial picks a location, Swap or Move remembers
//! it as the source, the dial then picks the destination, Ok runs the operation (Swap
//! interchanges the two programs, Move inserts the source at the destination and shifts the
//! programs in between by one; see `nord_core::library_ops`), Undo takes back the last one.
//! Memory Protect blocks Ok and Undo like it blocks STORE (manual p.57).
//!
//! Each location is a [`Cell`]: the program the user stored there and/or the archive entry that
//! the opened `.ns4b` keeps there. Cells move as a whole, so Export backup (`library/export.rs`)
//! can write the new layout.

use super::catalog::display_name;
use super::model::MenuId;
use super::oled::OledText;
use super::store::{ProgLoc, UserLibrary, UserProgram, TOTAL};
use crate::audio::AudioCommand;
use crate::panel::ui_state::ProgramUi;
use nord_core::library_ops::{OrganizeOp, OrganizeUndo};
use nord_panel::menus::{Menu, Value};

/// Item id of the location item (Organize page 1).
const ID_LOCATION: &str = "organize.1.location";

/// What one program location holds.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// Program stored with STORE (overrides the archive entry on export).
    pub user: Option<UserProgram>,
    /// Entry path of the archive's program at this location.
    pub factory: Option<String>,
}

impl Cell {
    /// Nothing stored and no archive entry.
    pub fn is_empty(&self) -> bool {
        self.user.is_none() && self.factory.is_none()
    }

    /// Program name as the display shows it.
    pub fn name(&self) -> Option<String> {
        match (&self.user, &self.factory) {
            (Some(u), _) => Some(u.name.clone()),
            (None, Some(f)) => Some(display_name(f)),
            (None, None) => None,
        }
    }
}

impl UserLibrary {
    /// Records which archive entry sits at which location (`flat index, entry path`). Returns
    /// `true` when the layout changed; Undo history is dropped then.
    pub fn set_factory(&mut self, slots: &[(u16, String)]) -> bool {
        let new: std::collections::BTreeMap<u16, String> =
            slots.iter().filter(|(i, _)| *i < TOTAL).cloned().collect();
        let changed = new != self.factory;
        self.factory = new;
        changed
    }

    /// An archive layout is known (a `.ns4b` was opened).
    pub fn has_factory_layout(&self) -> bool {
        !self.factory.is_empty()
    }

    /// Archive entry at `loc`.
    pub fn factory_at(&self, loc: ProgLoc) -> Option<&str> {
        self.factory.get(&loc.index()).map(String::as_str)
    }

    /// Program name at `loc`: the stored program, else the archive entry.
    pub fn name_at(&self, loc: ProgLoc) -> Option<String> {
        self.cells_at(loc.index()).name()
    }

    fn cells_at(&self, i: u16) -> Cell {
        Cell {
            user: self.programs.get(&i).cloned(),
            factory: self.factory.get(&i).cloned(),
        }
    }

    /// All [`TOTAL`] locations in order.
    pub fn cells(&self) -> Vec<Cell> {
        (0..TOTAL).map(|i| self.cells_at(i)).collect()
    }

    fn set_cells(&mut self, cells: Vec<Cell>) {
        self.programs.clear();
        self.factory.clear();
        for (i, c) in cells.into_iter().enumerate() {
            let i = i as u16;
            if let Some(u) = c.user {
                self.programs.insert(i, u);
            }
            if let Some(f) = c.factory {
                self.factory.insert(i, f);
            }
        }
    }

    /// Runs `op` on `src` and `dst`; `false` when nothing changed (same location).
    pub fn organize(
        &mut self,
        undo: &mut OrganizeUndo<Cell>,
        op: OrganizeOp,
        src: ProgLoc,
        dst: ProgLoc,
    ) -> bool {
        let mut cells = self.cells();
        let changed = undo.apply(
            &mut cells,
            op,
            usize::from(src.index()),
            usize::from(dst.index()),
        );
        if changed {
            self.set_cells(cells);
        }
        changed
    }

    /// Takes back the last Organize; `false` when there is none.
    pub fn undo_organize(&mut self, undo: &mut OrganizeUndo<Cell>) -> bool {
        let mut cells = self.cells();
        let done = undo.undo(&mut cells);
        if done {
            self.set_cells(cells);
        }
        done
    }
}

/// Organize progress of the Program area.
#[derive(Debug, Default)]
pub struct OrganizeFlow {
    /// Swap or Move chosen, with the source location picked by the dial at that moment.
    pub pending: Option<(OrganizeOp, ProgLoc)>,
    undo: OrganizeUndo<Cell>,
    /// The Preset Organize page (`organize_presets.rs`).
    pub preset: super::organize_presets::PresetOrganizeFlow,
}

impl OrganizeFlow {
    /// Library or archive layout changed by other means: forget the Undo state.
    pub fn reset(&mut self) {
        self.pending = None;
        self.undo.clear();
        self.preset.reset();
    }
}

impl ProgramUi {
    /// The Program Organize page (menu 3, page 1) is shown.
    pub fn on_organize_page(&self) -> bool {
        self.menu.menu() == Menu::Organize && self.menu.page_index() == 0
    }

    /// Location the PROGRAM dial points at on the Organize page.
    pub fn organize_cursor(&self) -> ProgLoc {
        let item = self.menu.page().items[0];
        debug_assert_eq!(item.id, ID_LOCATION);
        match self.menu.value(&item) {
            Value::Int(i) => ProgLoc::from_index(i.clamp(0, i32::from(TOTAL) - 1) as u16),
            _ => ProgLoc::default(),
        }
    }

    /// Soft button `n` (1..=4: Swap, Move, Ok, Undo) of the Organize page.
    pub(super) fn organize_soft(&mut self, n: u8) -> Vec<AudioCommand> {
        let cursor = self.organize_cursor();
        match n {
            1 | 2 => {
                let op = if n == 1 {
                    OrganizeOp::Swap
                } else {
                    OrganizeOp::Move
                };
                if matches!(self.organize.pending, Some((p, _)) if p == op) {
                    self.organize.pending = None;
                    self.set_message("Cancelled");
                } else {
                    self.organize.pending = Some((op, cursor));
                    self.set_message(format!("{} {}: pick target, Ok", label(op), cursor.label()));
                }
            }
            3 => self.organize_ok(cursor),
            _ => self.organize_undo(),
        }
        Vec::new()
    }

    fn organize_ok(&mut self, dst: ProgLoc) {
        let Some((op, src)) = self.organize.pending else {
            self.set_message("Pick Swap or Move first");
            return;
        };
        if self.store_blocked() {
            return;
        }
        let flow = &mut self.organize;
        if self.library.organize(&mut flow.undo, op, src, dst) {
            flow.pending = None;
            let joint = if op == OrganizeOp::Swap { "<>" } else { "->" };
            self.set_message(format!(
                "{} {} {joint} {}",
                op.done_label(),
                src.label(),
                dst.label()
            ));
        } else {
            self.set_message("Source and target are the same");
        }
    }

    fn organize_undo(&mut self) {
        if self.organize.pending.take().is_some() {
            self.set_message("Cancelled");
            return;
        }
        if self.store_blocked() {
            return;
        }
        let done = self.library.undo_organize(&mut self.organize.undo);
        self.set_message(if done { "Undone" } else { "Nothing to undo" });
    }
}

fn label(op: OrganizeOp) -> &'static str {
    match op {
        OrganizeOp::Swap => "Swap",
        OrganizeOp::Move => "Move",
    }
}

/// OLED text of the Program Organize page.
pub fn compose(ui: &ProgramUi) -> OledText {
    let cursor = ui.organize_cursor();
    let name = ui
        .library
        .name_at(cursor)
        .unwrap_or_else(|| "(empty)".to_string());
    let action = match ui.organize.pending {
        None => "Choose Swap or Move".to_string(),
        Some((OrganizeOp::Swap, src)) => format!("Swap {} with {}", src.label(), cursor.label()),
        Some((OrganizeOp::Move, src)) => format!("Move {} to {}", src.label(), cursor.label()),
    };
    let page = ui.menu.page();
    OledText {
        big: MenuId::Organize.title().into(),
        sub: match &ui.message {
            Some((msg, _)) => msg.clone(),
            None => format!(
                "{}/{} {}",
                ui.menu.page_index() + 1,
                ui.menu.page_count(),
                page.title
            ),
        },
        rows: vec![format!(">{}  {name}", cursor.label()), format!(" {action}")],
        soft: ["Swap", "Move", "Ok", "Undo"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::bay_program::model::Screen;
    use nord_core::engine::Stage4Engine;

    fn prog(name: &str) -> UserProgram {
        UserProgram {
            name: name.into(),
            category: 0,
            state: Stage4Engine::new(48_000.0).capture_state(),
        }
    }

    fn loc(s: &str) -> ProgLoc {
        ProgLoc::parse(s).unwrap()
    }

    /// Organize page open, Memory Protect off, three user programs at A:11, A:12, A:13.
    fn ui() -> ProgramUi {
        let mut u = ProgramUi::default();
        u.menu.set_memory_protect(false);
        u.open_menu(MenuId::Organize);
        for (l, n) in [("A:11", "One"), ("A:12", "Two"), ("A:13", "Three")] {
            u.library.put(loc(l), prog(n));
        }
        u
    }

    fn goto(u: &mut ProgramUi, l: &str) {
        let delta = i32::from(loc(l).index()) - i32::from(u.organize_cursor().index());
        u.dial(delta, false);
        assert_eq!(u.organize_cursor(), loc(l));
    }

    fn names(u: &ProgramUi, ls: &[&str]) -> Vec<Option<String>> {
        ls.iter().map(|l| u.library.name_at(loc(l))).collect()
    }

    #[test]
    fn swap_picks_source_then_target_and_ok_interchanges() {
        let mut u = ui();
        assert!(u.on_organize_page() && u.screen == Screen::Menu(MenuId::Organize));
        u.organize_soft(1);
        goto(&mut u, "A:13");
        assert!(compose(&u).rows[1].contains("Swap A:11 with A:13"));
        u.organize_soft(3);
        assert_eq!(u.message.as_ref().unwrap().0, "Swapped A:11 <> A:13");
        let want = [Some("Three"), Some("Two"), Some("One")];
        assert_eq!(
            names(&u, &["A:11", "A:12", "A:13"]),
            want.map(|s| s.map(String::from))
        );
        assert!(u.organize.pending.is_none());
    }

    #[test]
    fn move_shifts_the_programs_in_between_and_undo_restores() {
        let mut u = ui();
        u.organize_soft(2);
        goto(&mut u, "A:13");
        u.organize_soft(3);
        assert_eq!(u.message.as_ref().unwrap().0, "Moved A:11 -> A:13");
        let want = [Some("Two"), Some("Three"), Some("One")];
        assert_eq!(
            names(&u, &["A:11", "A:12", "A:13"]),
            want.map(|s| s.map(String::from))
        );
        u.organize_soft(4);
        assert_eq!(u.message.as_ref().unwrap().0, "Undone");
        assert_eq!(u.library.name_at(loc("A:11")).as_deref(), Some("One"));
        u.organize_soft(4);
        assert_eq!(u.message.as_ref().unwrap().0, "Nothing to undo");
    }

    #[test]
    fn memory_protect_blocks_ok_and_undo_but_not_choosing() {
        let mut u = ui();
        u.menu.set_memory_protect(true);
        u.organize_soft(1);
        goto(&mut u, "A:12");
        u.organize_soft(3);
        assert_eq!(u.message.as_ref().unwrap().0, "Memory Protected");
        assert_eq!(u.library.name_at(loc("A:11")).as_deref(), Some("One"));
        assert!(u.organize.pending.is_some(), "the choice stays");
        u.menu.set_memory_protect(false);
        u.organize_soft(3);
        assert_eq!(u.library.name_at(loc("A:11")).as_deref(), Some("Two"));
        u.menu.set_memory_protect(true);
        u.organize_soft(4);
        assert_eq!(u.message.as_ref().unwrap().0, "Memory Protected");
        assert_eq!(u.library.name_at(loc("A:11")).as_deref(), Some("Two"));
    }

    #[test]
    fn ok_without_a_choice_same_location_and_cancel_say_so() {
        let mut u = ui();
        u.organize_soft(3);
        assert_eq!(u.message.as_ref().unwrap().0, "Pick Swap or Move first");
        u.organize_soft(1);
        u.organize_soft(3);
        assert_eq!(
            u.message.as_ref().unwrap().0,
            "Source and target are the same"
        );
        u.organize_soft(1); // Swap again cancels
        assert!(u.organize.pending.is_none());
        u.organize_soft(2);
        u.organize_soft(1); // other op replaces
        assert_eq!(u.organize.pending.unwrap().0, OrganizeOp::Swap);
        u.organize_soft(4); // Undo cancels a pending choice first
        assert!(u.organize.pending.is_none());
        assert_eq!(u.message.as_ref().unwrap().0, "Cancelled");
    }

    #[test]
    fn archive_entries_move_with_the_cells_and_empty_slots_work() {
        let mut u = ui();
        u.library.set_factory(&[
            (0, "Program/Bank A/Alpha.ns4p".into()),
            (5, "Program/Bank A/Beta.ns4p".into()),
        ]);
        assert!(!u.library.set_factory(&[
            (0, "Program/Bank A/Alpha.ns4p".into()),
            (5, "Program/Bank A/Beta.ns4p".into())
        ]));
        assert_eq!(
            u.library.name_at(loc("A:11")).as_deref(),
            Some("One"),
            "user wins"
        );
        assert_eq!(u.library.name_at(loc("A:16")).as_deref(), Some("Beta"));
        u.organize_soft(1);
        goto(&mut u, "A:16");
        u.organize_soft(3);
        assert_eq!(
            u.library.factory_at(loc("A:16")),
            Some("Program/Bank A/Alpha.ns4p")
        );
        assert_eq!(
            u.library.factory_at(loc("A:11")),
            Some("Program/Bank A/Beta.ns4p")
        );
        assert_eq!(u.library.name_at(loc("A:16")).as_deref(), Some("One"));
        // Move into an empty location keeps the count of programs.
        let before = u.library.cells().iter().filter(|c| !c.is_empty()).count();
        u.organize_soft(2);
        goto(&mut u, "A:18");
        u.organize_soft(3);
        assert_eq!(
            u.library.cells().iter().filter(|c| !c.is_empty()).count(),
            before
        );
        assert_eq!(compose(&u).soft, ["Swap", "Move", "Ok", "Undo"]);
    }

    #[test]
    fn organize_page_text_shows_location_name_and_page_title() {
        let mut u = ui();
        goto(&mut u, "A:12");
        let t = compose(&u);
        assert_eq!(t.big, "Program Organize");
        assert_eq!(t.sub, "1/2 Program Organize");
        assert_eq!(t.rows[0], ">A:12  Two");
        assert_eq!(t.rows[1], " Choose Swap or Move");
        goto(&mut u, "H:88");
        assert!(compose(&u).rows[0].ends_with("(empty)"));
        u.menu.step_page(1);
        assert!(!u.on_organize_page());
        u.organize.reset();
    }
}
