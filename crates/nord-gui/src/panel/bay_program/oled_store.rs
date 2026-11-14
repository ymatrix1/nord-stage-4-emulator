//! Text of the Store screens (`STORE PROGRAM TO` / `STORE PROGRAM AS`) on their measured rows.

use super::name::{CATEGORIES, UNDEFINED_CATEGORY};
use super::oled::{name_at, Line, OledText};
use super::store::{StoreStage, StoreTarget};
use crate::panel::ui_state::ProgramUi;

/// VERIFIED (ARM frames of `STORE PROGRAM TO`, research note s16.3, measured on the 128 x 64
/// grid with `tools/arm_menu_edit.py`'s `save_screen`): the title sits in the inverted bar, the
/// location (13 px) at y = 18, the name at the location (13 px) at y = 33 and the `As:` line
/// (8 px, no space after the colon) at y = 51.
const TO_ROWS: [u8; 3] = [18, 33, 51];
/// VERIFIED (ARM frame of `STORE PROGRAM AS`): the name (13 px, the cursor cell lit) at y = 19
/// and the `Cat:` line (8 px, starting at x = 2) at y = 39.
const AS_ROWS: [u8; 2] = [19, 39];
const AS_CAT_X: u8 = 2;

/// VERIFIED (same frame): a program that has no category reads `Cat: <None>`, not the last entry
/// of the category list (`Undefined`) that the Cat soft button offers.
fn category_caption(label: &str) -> String {
    if label == CATEGORIES[UNDEFINED_CATEGORY] {
        "<None>".to_string()
    } else {
        label.to_string()
    }
}

/// The text of `STORE PROGRAM TO` / `STORE PROGRAM AS` (or `Store Preset To` for a preset).
pub fn compose(ui: &ProgramUi) -> OledText {
    let ed = &ui.store.editor;
    if ui.store.stage == StoreStage::Naming {
        let [name_y, cat_y] = AS_ROWS;
        let mut name = Line::big(name_y, ed.display());
        name.cursor = Some(ed.cursor());
        return OledText {
            bar: Some(String::new()),
            big: "STORE PROGRAM AS".into(),
            lines: vec![
                name,
                Line::small(
                    cat_y,
                    AS_CAT_X,
                    format!("Cat: {}", category_caption(ed.category_label())),
                ),
            ],
            soft: ui.screen.soft_labels(),
            ..Default::default()
        };
    }
    if let Some(t) = ui.preset_store_text() {
        return t;
    }
    let (target, existing) = match ui.store.target {
        Some(StoreTarget::Live(s)) => (format!("Live {}", s + 1), String::new()),
        Some(StoreTarget::Bank(l)) => (l.label(), name_at(ui, l)),
        None => (String::new(), String::new()),
    };
    // VERIFIED: hardware lists the name now at the target (`<empty>` when unused) and then
    // `As:<name>` (ARM frame; the piano-pad video, 5:40-5:45, shows the same two lines).
    let existing = if existing.is_empty() {
        super::nav::EMPTY_NAME.to_string()
    } else {
        existing
    };
    let [loc_y, name_y, as_y] = TO_ROWS;
    OledText {
        bar: Some(String::new()),
        big: "STORE PROGRAM TO".into(),
        lines: vec![
            Line::big(loc_y, target),
            Line::big(name_y, existing),
            Line::small(as_y, 0, format!("As:{}", ed.name())),
        ],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_to_uses_the_measured_rows_and_has_no_space_after_as() {
        let mut u = ProgramUi::default();
        u.title = "Rusty".into();
        u.store_as();
        u.store_press();
        let t = compose(&u);
        assert_eq!(t.big, "STORE PROGRAM TO");
        assert_eq!(t.bar, Some(String::new()), "a title-bar screen");
        let rows: Vec<(u8, bool, &str)> = t
            .lines
            .iter()
            .map(|l| (l.y, l.big, l.text.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (18, true, "A:11"),
                (33, true, "<empty>"),
                (51, false, "As:Rusty")
            ]
        );
    }

    #[test]
    fn store_as_lights_the_cursor_cell_and_reads_none_for_no_category() {
        let mut u = ProgramUi::default();
        u.title = "Rusty".into();
        u.store_as();
        let t = compose(&u);
        assert_eq!(t.big, "STORE PROGRAM AS");
        assert_eq!(t.soft, ["Abc", "Cat", "Del", "Ins"]);
        let name = &t.lines[0];
        assert_eq!((name.y, name.big, name.cursor), (19, true, Some(0)));
        assert!(name.text.starts_with("Rusty"), "{}", name.text);
        let cat = &t.lines[1];
        assert_eq!((cat.y, cat.x, cat.text.as_str()), (39, 2, "Cat: <None>"));
    }

    #[test]
    fn a_real_category_is_shown_by_name() {
        assert_eq!(category_caption("Piano"), "Piano");
        assert_eq!(category_caption("Undefined"), "<None>");
    }

    #[test]
    fn the_tooltip_text_lists_the_lines() {
        let mut u = ProgramUi::default();
        u.store_as();
        let plain = compose(&u).plain();
        assert!(plain.contains("STORE PROGRAM AS") && plain.contains("Cat: <None>"));
    }
}
