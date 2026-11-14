//! Pieces of the Program home screen shared by its views: the location title with the boxed `E`
//! marker, the `Live N` title of Live Mode and the per-section rows.

use super::oled::Summary;
use crate::panel::ui_state::ProgramUi;

/// Marks the boxed `E` inside a text line. The painter draws a box with an `E` in its place;
/// [`strip_marks`] turns it into plain text for the tooltip.
///
/// ASSUMED: a private-use character as in-band marker (no real text uses it), chosen so
/// `OledText` keeps its shape for every screen.
pub const EDITED: char = '\u{E0E1}';

/// Plain-text form of the boxed `E` (tooltip).
const EDITED_PLAIN: &str = " [E]";

/// `text` with every [`EDITED`] mark written out for plain text.
pub fn strip_marks(text: &str) -> String {
    text.replace(EDITED, EDITED_PLAIN)
}

/// Big title of the home screen: the location (`A:11`) followed by the boxed `E` while the
/// program is edited, or `Live N` in Live Mode.
///
/// VERIFIED: Live Mode shows `Live 1` as the big title (DSoundman course, Bank Games 19:45).
/// ASSUMED: Live Mode shows no `E` because Live programs store their edits by themselves
/// (manual p.13); footage of an edited Live program would confirm.
pub fn location_title(ui: &ProgramUi) -> String {
    if let Some(slot) = ui.live.slot() {
        return format!("Live {}", slot + 1);
    }
    let mut t = ui.loc.label();
    if ui.is_edited() {
        t.push(EDITED);
    }
    t
}

/// Program name for the line under the title; empty when it only repeats `Live N`.
pub fn name_line(ui: &ProgramUi) -> String {
    if ui.live.is_active() && ui.title.starts_with("Live ") {
        return String::new();
    }
    ui.title.clone()
}

/// One row per section that has a layer on, named as the hardware names its sections.
///
/// ASSUMED: section names only. Hardware also prints the preset id (`1:05`) or source name with a
/// size letter (`XL`, `M`) and mini drawbars for the organ (course footage PL 11:25, BG 17:10,
/// SG 8:40, ORG 5:30); the size letter and preset id have no source in the emulator yet.
pub fn section_rows(sum: &Summary) -> Vec<String> {
    let on = |a: usize| sum.layers_on[a] || sum.layers_on[a + 1];
    let mut rows = Vec::new();
    if on(0) {
        rows.push(format!("Organ {}", sum.organ));
    }
    if on(2) {
        rows.push(format!("Piano {}", sum.piano));
    }
    if on(4) || sum.layers_on[6] {
        rows.push(format!("Synth {}", sum.synth));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_shows_the_location_then_the_edited_mark() {
        let mut u = ProgramUi::default();
        assert_eq!(location_title(&u), "A:11");
        u.sync_edits(1);
        assert_eq!(location_title(&u), format!("A:11{EDITED}"));
        assert_eq!(strip_marks(&location_title(&u)), "A:11 [E]");
    }

    #[test]
    fn live_mode_titles_the_slot_and_drops_the_mark() {
        let mut u = ProgramUi::default();
        u.sync_edits(1);
        u.live.select(1);
        u.title = "Live 1".into();
        assert_eq!(location_title(&u), "Live 1");
        assert_eq!(name_line(&u), "");
        u.title = "Pad".into();
        assert_eq!(name_line(&u), "Pad");
    }

    #[test]
    fn rows_list_only_sections_with_a_layer_on() {
        let sum = Summary {
            organ: "B3".into(),
            piano: "Grand".into(),
            synth: "Saw".into(),
            layers_on: [true, false, false, false, false, false, true],
        };
        assert_eq!(section_rows(&sum), ["Organ B3", "Synth Saw"]);
        assert!(section_rows(&Summary::default()).is_empty());
    }
}
