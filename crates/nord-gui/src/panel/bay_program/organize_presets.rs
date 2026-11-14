//! Preset Organize (Shift+Program 3, page 2, manual OS v1.6x p.45-46): Swap and Move Preset
//! Library entries, same flow as the Program page (`organize.rs`).
//!
//! The manual says presets are organized "in just the same way". It does not say how the
//! section is chosen or how a location is written, so (ASSUMED): Shift + PROGRAM dial steps the
//! section Organ / Piano / Synth, the dial moves the location pointer, and a location reads
//! `bank:position` like the preset ids on the display (`1:05`, manual p.42).
//!
//! The in-memory model holds the entry that sits at each location of each section
//! ([`PresetLibrary`]): an archive entry path, or the virtual path of a preset stored with STORE
//! (`preset_user.rs`). Export backup (`library/export.rs`) writes the new order.

use super::catalog::display_name;
use super::oled::OledText;
use crate::audio::AudioCommand;
use crate::panel::ui_state::ProgramUi;
use nord_core::cbin::{PresetSection, PRESETS_PER_BANK};
use nord_core::library_ops::{OrganizeOp, OrganizeUndo};
use nord_panel::menus::{Menu, Value};
use std::collections::BTreeMap;

const ID_LOCATION: &str = "organize.2.location";
const ID_SECTION: &str = "organize.2.section";
/// Section names in menu choice order (`organize.2.section`).
const NAMES: [&str; 3] = ["Organ", "Piano", "Synth"];

fn slot(section: PresetSection) -> usize {
    PresetSection::ALL
        .iter()
        .position(|s| *s == section)
        .unwrap_or(0)
}

/// `1:05` for flat location 4: bank number and position, both 1-based.
///
/// ASSUMED: 1-based position with two digits (the manual prints `1:27`, `2:53`; whether `1:01` or
/// `1:00` is the first location is not shown).
pub fn preset_label(index: u16) -> String {
    format!(
        "{}:{:02}",
        index / PRESETS_PER_BANK + 1,
        index % PRESETS_PER_BANK + 1
    )
}

/// The archive entries of the Preset Library by section and flat location.
#[derive(Debug, Clone, Default)]
pub struct PresetLibrary {
    pub(super) slots: [BTreeMap<u16, String>; 3],
    /// Presets stored with STORE, by their virtual entry path (see `preset_user.rs`).
    pub(super) user: BTreeMap<String, super::preset_user::UserPreset>,
    /// Entry last loaded into each section: the base a stored preset is patched onto.
    pub(super) loaded: [Option<String>; 3],
    /// Serial of the next stored preset (keeps virtual paths unique).
    pub(super) serial: u32,
}

impl PresetLibrary {
    /// Records which entry sits at which location of `section` (`flat index, entry path`).
    /// Returns `true` when the layout changed.
    pub fn set_factory(&mut self, section: PresetSection, slots: &[(u16, String)]) -> bool {
        let new: BTreeMap<u16, String> = slots
            .iter()
            .filter(|(i, _)| *i < section.capacity())
            .cloned()
            .collect();
        let changed = new != self.slots[slot(section)];
        self.slots[slot(section)] = new;
        self.drop_unreferenced();
        changed
    }

    /// Entry path at flat location `i` of `section`.
    pub fn entry_at(&self, section: PresetSection, i: u16) -> Option<&str> {
        self.slots[slot(section)].get(&i).map(String::as_str)
    }

    /// Preset name at a location, as the display shows it.
    pub fn name_at(&self, section: PresetSection, i: u16) -> Option<String> {
        self.entry_at(section, i).map(display_name)
    }

    /// Number of presets in `section`.
    pub fn count(&self, section: PresetSection) -> usize {
        self.slots[slot(section)].len()
    }

    /// All locations of `section` in order (`capacity` cells).
    pub fn cells(&self, section: PresetSection) -> Vec<Option<String>> {
        (0..section.capacity())
            .map(|i| self.slots[slot(section)].get(&i).cloned())
            .collect()
    }

    fn set_cells(&mut self, section: PresetSection, cells: Vec<Option<String>>) {
        let map = &mut self.slots[slot(section)];
        map.clear();
        for (i, c) in cells.into_iter().enumerate() {
            if let Some(entry) = c {
                map.insert(i as u16, entry);
            }
        }
    }

    /// Runs `op` on `src` and `dst`; `false` when nothing changed (same location).
    pub fn organize(
        &mut self,
        undo: &mut OrganizeUndo<Option<String>>,
        section: PresetSection,
        op: OrganizeOp,
        (src, dst): (u16, u16),
    ) -> bool {
        let mut cells = self.cells(section);
        let changed = undo.apply(&mut cells, op, usize::from(src), usize::from(dst));
        if changed {
            self.set_cells(section, cells);
        }
        changed
    }

    /// Takes back the last Organize of `section`; `false` when there is none.
    pub fn undo_organize(
        &mut self,
        undo: &mut OrganizeUndo<Option<String>>,
        section: PresetSection,
    ) -> bool {
        let mut cells = self.cells(section);
        let done = undo.undo(&mut cells);
        if done {
            self.set_cells(section, cells);
        }
        done
    }

    /// Any section has entries (a `.ns4b` with presets was opened).
    pub fn has_layout(&self) -> bool {
        self.slots.iter().any(|m| !m.is_empty())
    }
}

/// Preset Organize progress: the chosen operation with its source location, and one Undo level
/// per section.
#[derive(Debug, Default)]
pub struct PresetOrganizeFlow {
    pub pending: Option<(OrganizeOp, u16)>,
    undo: [OrganizeUndo<Option<String>>; 3],
}

impl PresetOrganizeFlow {
    /// Library layout changed by other means: forget the Undo state.
    pub fn reset(&mut self) {
        self.pending = None;
        for u in &mut self.undo {
            u.clear();
        }
    }
}

fn op_word(op: OrganizeOp) -> &'static str {
    match op {
        OrganizeOp::Swap => "Swap",
        OrganizeOp::Move => "Move",
    }
}

impl ProgramUi {
    /// The Preset Organize page (menu 3, page 2) is shown.
    pub fn on_preset_organize_page(&self) -> bool {
        self.menu.menu() == Menu::Organize && self.menu.page_index() == 1
    }

    /// Section the page organizes.
    pub fn organize_section(&self) -> PresetSection {
        let item = self.menu.page().items[1];
        debug_assert_eq!(item.id, ID_SECTION);
        match self.menu.value(&item) {
            Value::Index(i) => PresetSection::ALL[i.min(2)],
            _ => PresetSection::Organ,
        }
    }

    /// Flat location the PROGRAM dial points at, clamped to the section.
    pub fn preset_organize_cursor(&self) -> u16 {
        let item = self.menu.page().items[0];
        debug_assert_eq!(item.id, ID_LOCATION);
        let last = i32::from(self.organize_section().capacity()) - 1;
        match self.menu.value(&item) {
            Value::Int(i) => i.clamp(0, last) as u16,
            _ => 0,
        }
    }

    /// Shift + PROGRAM dial: steps the section (Organ, Piano, Synth, no wrap). The location
    /// pointer goes back inside the new section and a pending choice is cancelled.
    pub fn step_organize_section(&mut self, delta: i32) {
        let now = slot(self.organize_section()) as i32;
        let next = (now + delta.signum()).clamp(0, 2);
        if next == now {
            return;
        }
        self.menu.set(ID_SECTION, Value::Index(next as usize));
        let cursor = self.preset_organize_cursor();
        self.menu.set(ID_LOCATION, Value::Int(i32::from(cursor)));
        self.organize.preset.pending = None;
    }

    /// Soft button `n` (1..=4: Swap, Move, Ok, Undo) of the Preset Organize page.
    pub(super) fn preset_organize_soft(&mut self, n: u8) -> Vec<AudioCommand> {
        let cursor = self.preset_organize_cursor();
        match n {
            1 | 2 => {
                let op = if n == 1 {
                    OrganizeOp::Swap
                } else {
                    OrganizeOp::Move
                };
                if matches!(self.organize.preset.pending, Some((p, _)) if p == op) {
                    self.organize.preset.pending = None;
                    self.set_message("Cancelled");
                } else {
                    self.organize.preset.pending = Some((op, cursor));
                    self.set_message(format!(
                        "{} {}: pick target, Ok",
                        op_word(op),
                        preset_label(cursor)
                    ));
                }
            }
            3 => self.preset_organize_ok(cursor),
            _ => self.preset_organize_undo(),
        }
        Vec::new()
    }

    fn preset_organize_ok(&mut self, dst: u16) {
        let Some((op, src)) = self.organize.preset.pending else {
            self.set_message("Pick Swap or Move first");
            return;
        };
        if self.store_blocked() {
            return;
        }
        let section = self.organize_section();
        let flow = &mut self.organize.preset;
        if self
            .library
            .presets
            .organize(&mut flow.undo[slot(section)], section, op, (src, dst))
        {
            flow.pending = None;
            let joint = if op == OrganizeOp::Swap { "<>" } else { "->" };
            self.set_message(format!(
                "{} {} {joint} {}",
                op.done_label(),
                preset_label(src),
                preset_label(dst)
            ));
        } else {
            self.set_message("Source and target are the same");
        }
    }

    fn preset_organize_undo(&mut self) {
        if self.organize.preset.pending.take().is_some() {
            self.set_message("Cancelled");
            return;
        }
        if self.store_blocked() {
            return;
        }
        let section = self.organize_section();
        let flow = &mut self.organize.preset;
        let done = self
            .library
            .presets
            .undo_organize(&mut flow.undo[slot(section)], section);
        self.set_message(if done { "Undone" } else { "Nothing to undo" });
    }
}

/// OLED text of the Preset Organize page.
pub fn compose(ui: &ProgramUi) -> OledText {
    let section = ui.organize_section();
    let cursor = ui.preset_organize_cursor();
    let name = ui
        .library
        .presets
        .name_at(section, cursor)
        .unwrap_or_else(|| "(empty)".to_string());
    let action = match ui.organize.preset.pending {
        _ if !ui.library.presets.has_layout() => "No presets: open a .ns4b".to_string(),
        None => "Choose Swap or Move".to_string(),
        Some((op, src)) => format!(
            "{} {} {} {}",
            op_word(op),
            preset_label(src),
            if op == OrganizeOp::Swap { "with" } else { "to" },
            preset_label(cursor)
        ),
    };
    let page = ui.menu.page();
    OledText {
        big: "PRESET ORGANIZE".into(),
        sub: match &ui.message {
            Some((msg, _)) => msg.clone(),
            None => format!(
                "{}/{} {}",
                ui.menu.page_index() + 1,
                ui.menu.page_count(),
                page.title
            ),
        },
        rows: vec![
            format!(">{}  {name}", preset_label(cursor)),
            format!(" {}: {action}", NAMES[slot(section)]),
        ],
        soft: ["Swap", "Move", "Ok", "Undo"],
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "organize_presets_tests.rs"]
mod tests;

#[cfg(test)]
mod basics {
    use super::*;

    #[test]
    fn sections_map_to_their_menu_choice_order_and_verbs() {
        for (i, s) in PresetSection::ALL.into_iter().enumerate() {
            assert_eq!(slot(s), i);
        }
        assert_eq!(NAMES, ["Organ", "Piano", "Synth"]);
        assert_eq!(op_word(OrganizeOp::Swap), "Swap");
        assert_eq!(op_word(OrganizeOp::Move), "Move");
    }
}
