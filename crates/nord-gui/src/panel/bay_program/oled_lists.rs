//! Row lists of the Program OLED: the program list, Keyboard Split and the Preset browser.

use super::catalog::display_name;
use super::model::PresetKind;
use super::nav::ORGAN_PRESETS;
use super::oled::{layer_label, OledText};
use super::oled_icons::ICON_LIST;
use super::split::{note_name, xfade_label, SplitRow};
use super::store::ProgLoc;
use crate::panel::ui_state::ProgramUi;

/// Rows of a program list on screen (rows at y = 13, 23, 33, 43).
pub const LIST_ROWS: usize = 4;

/// Row the selection sits on when the list has entries before the cursor.
///
/// VERIFIED: ARM frame of the program list (research note s16.5): rows y = 13 / 23 / 33 / 43, the
/// selected one lit in rows 22 to 31 (the second row). ASSUMED: it stays on the second row while
/// there is an entry above it, and moves to the first row at the top of the list (the Program
/// Organize frame of note s9 lit the first row, rows 12 to 21).
pub const SEL_ROW: usize = 1;

/// One list entry text: location and name (`A:26 <empty>`), no focus marker.
pub fn entry_text(loc: ProgLoc, name: &str) -> String {
    let name = if name.is_empty() {
        super::nav::EMPTY_NAME
    } else {
        name
    };
    format!("{} {name}", loc.label())
}

/// The four visible rows of a program list and the index (within them) of the lit one.
///
/// `entries` are the whole list, `cursor` the selected entry, `sel_row` the row it sits on when
/// there are enough entries before and after it.
pub fn list_rows(entries: &[String], cursor: usize, sel_row: usize) -> (Vec<String>, usize) {
    let (start, sel) = window_start(entries.len(), cursor, sel_row);
    let rows: Vec<String> = entries
        .iter()
        .skip(start)
        .take(LIST_ROWS)
        .cloned()
        .collect();
    (rows, sel)
}

/// First visible entry and lit row index for a list of `len` entries with the cursor at `cursor`.
pub fn window_start(len: usize, cursor: usize, sel_row: usize) -> (usize, usize) {
    if len == 0 {
        return (0, 0);
    }
    let cursor = cursor.min(len - 1);
    let sel = sel_row.min(cursor).min(LIST_ROWS - 1);
    let start = (cursor - sel).min(len.saturating_sub(LIST_ROWS));
    (start, cursor - start)
}

/// A title-bar program list: the list icon and `title` in the bar, `right` right-aligned.
pub fn program_list(title: &str, right: &str, entries: &[String], cursor: usize) -> OledText {
    let (rows, sel) = list_rows(entries, cursor, SEL_ROW);
    OledText {
        bar: Some(right.to_string()),
        big: format!("{ICON_LIST}{title}"),
        rows,
        sel: Some(sel),
        ..Default::default()
    }
}

/// The LIST view: every program of the catalog in the LIST order, the cursor lit.
pub fn list(ui: &ProgramUi) -> OledText {
    let order = ui.catalog.order(ui.list_mode);
    let entries: Vec<String> = order
        .iter()
        .map(|&i| {
            entry_text(
                ProgLoc::from_index(i as u16),
                &display_name(&ui.catalog.programs[i]),
            )
        })
        .collect();
    let mode = ["Num", "Abc", "Cat"][ui.list_mode as usize];
    let mut t = program_list(&format!("LIST {mode}"), "", &entries, ui.list_cursor);
    if entries.is_empty() {
        t.sub = "No backup open".into();
    }
    t.soft = ui.screen.soft_labels();
    t.soft_on = Some(ui.list_mode as usize);
    t
}

fn window(items: &[String], cursor: usize, n: usize) -> Vec<String> {
    let start = cursor
        .saturating_sub(n / 2)
        .min(items.len().saturating_sub(n));
    items
        .iter()
        .enumerate()
        .skip(start)
        .take(n)
        .map(|(i, t)| format!("{}{t}", if i == cursor { ">" } else { " " }))
        .collect()
}

pub fn split(ui: &ProgramUi) -> OledText {
    let s = &ui.split;
    let pts = |f: &dyn Fn(usize) -> String| {
        ["L", "M", "H"]
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{}{n}:{}", if i == s.point { ">" } else { "" }, f(i)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let note = pts(&|i| {
        if s.enabled[i] {
            note_name(s.notes[i])
        } else {
            "Off".into()
        }
    });
    let xf = pts(&|i| xfade_label(s.xfade[i]).into());
    let (a, b) = if s.row == SplitRow::Note {
        (">", " ")
    } else {
        (" ", ">")
    };
    OledText {
        big: format!("KEYBOARD SPLIT {}", if s.on { "ON" } else { "OFF" }),
        sub: String::new(),
        rows: vec![format!("{a}Note {note}"), format!("{b}xFade {xf}")],
        soft: ui.screen.soft_labels(),
        ..Default::default()
    }
}

pub fn preset(ui: &ProgramUi, kind: PresetKind, single: bool) -> OledText {
    let names: Vec<String> = match kind {
        PresetKind::Organ => ORGAN_PRESETS.iter().map(|m| format!("{m:?}")).collect(),
        PresetKind::Piano => ui.catalog.pianos.iter().map(|e| display_name(e)).collect(),
        PresetKind::Synth => ui.catalog.samples.iter().map(|e| display_name(e)).collect(),
    };
    let names = ui.preset_browser_names(kind).unwrap_or(names);
    let mut rows = window(&names, ui.preset_cursor, 4);
    if single {
        rows.insert(0, format!("Layer: {}", layer_label(ui.focus)));
    }
    OledText {
        bar: Some(String::new()),
        big: kind.title().into(),
        sub: names
            .get(ui.preset_cursor)
            .cloned()
            .unwrap_or_else(|| "No backup open".into()),
        rows,
        soft: ui.screen.soft_labels(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::bay_program::model::Screen;

    #[test]
    fn rows_keep_the_cursor_on_the_second_row_and_stop_at_the_ends() {
        let all: Vec<String> = (0..10).map(|i| format!("e{i}")).collect();
        assert_eq!(list_rows(&all, 0, 1), (all[0..4].to_vec(), 0));
        assert_eq!(list_rows(&all, 1, 1), (all[0..4].to_vec(), 1));
        assert_eq!(list_rows(&all, 5, 1), (all[4..8].to_vec(), 1));
        assert_eq!(list_rows(&all, 9, 1), (all[6..10].to_vec(), 3));
        assert_eq!(list_rows(&all, 99, 1).1, 3, "a cursor past the end clamps");
        assert!(list_rows(&[], 0, 1).0.is_empty());
    }

    #[test]
    fn an_entry_reads_location_then_name_with_empty_for_no_name() {
        let loc = ProgLoc::from_index(25);
        assert_eq!(entry_text(loc, "Pure Saw"), "A:42 Pure Saw");
        assert_eq!(entry_text(loc, ""), "A:42 <empty>");
        let a26 = ProgLoc {
            bank: 0,
            page: 1,
            slot: 5,
        };
        assert_eq!(entry_text(a26, ""), "A:26 <empty>");
    }

    #[test]
    fn the_program_list_has_the_icon_bar_four_rows_and_a_lit_one() {
        let entries: Vec<String> = (0..8)
            .map(|i| entry_text(ProgLoc::from_index(i), ""))
            .collect();
        let t = program_list("Import into A:13", "", &entries, 2);
        assert_eq!(t.big, format!("{ICON_LIST}Import into A:13"));
        assert_eq!((t.rows.len(), t.sel), (4, Some(1)));
        assert_eq!(t.rows[1], "A:13 <empty>");
        assert!(t.plain().contains("A:13 <empty>"));
    }

    #[test]
    fn the_list_screen_is_a_program_list_with_the_sort_mode_lit() {
        let mut u = ProgramUi::default();
        u.screen = Screen::List;
        let t = list(&u);
        assert!(t.big.ends_with("LIST Num") && t.bar.is_some());
        assert_eq!(t.soft_on, Some(0));
        assert_eq!(t.sub, "No backup open");
    }

    #[test]
    fn window_centres_the_cursor_and_marks_it() {
        let items: Vec<String> = ["a", "b", "c", "d", "e", "f"].map(String::from).to_vec();
        assert_eq!(window(&items, 0, 4), [">a", " b", " c", " d"]);
        assert_eq!(window(&items, 3, 4), [" b", " c", ">d", " e"]);
        assert_eq!(window(&items, 5, 4), [" c", " d", " e", ">f"]);
        assert!(window(&[], 0, 4).is_empty());
    }
}
