//! Organize (manual OS v1.6x p.45-46). SHIFT + Program 3. Programs and presets can be rearranged
//! with Swap and Move; the same flow works for Presets.

use super::{Menu, MenuItem as I, MenuPage};
use crate::types::Binding::UiOnly;

/// Page 1 acts on the in-memory program library, not on a host parameter or an engine field.
const LIBRARY: &str =
    "acts on the in-memory program library (nord_core::library_ops), no engine parameter";

/// Page 2 acts on the in-memory Preset Library (`.ns4o` / `.ns4n` / `.ns4y` entries of the backup).
const PRESETS: &str =
    "acts on the in-memory preset library of the opened backup (nord_core::library_ops), no engine parameter";

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::Organize,
        page: 1,
        title: "Program Organize",
        items: &[
            // 8 banks (A-H) x 64 programs = 512 locations (manual p.12); 0 = A:1.
            I::int(
                "organize.1.location",
                "Program location",
                (0, 511, 1),
                0,
                45,
                UiOnly("location pointer of the Organize page; the PROGRAM dial moves it"),
            ),
            I::action("organize.1.swap", "Swap", 45, UiOnly(LIBRARY)),
            I::action("organize.1.move", "Move", 45, UiOnly(LIBRARY)),
            I::action("organize.1.ok", "Ok", 46, UiOnly(LIBRARY)),
            I::action("organize.1.undo", "Undo", 46, UiOnly(LIBRARY)),
        ],
    },
    MenuPage {
        menu: Menu::Organize,
        page: 2,
        title: "Preset Organize",
        items: &[
            // Location inside the section chosen below: bank * 64 + position, shown `1:05`.
            // Organ and Piano hold 2 banks, Synth 8 (`nord_core::cbin::PresetSection::capacity`);
            // the range is the largest, the page clamps it to the section.
            I::int(
                "organize.2.location",
                "Preset location",
                (0, 511, 1),
                0,
                45,
                UiOnly("location pointer of the Preset Organize page; the PROGRAM dial moves it"),
            )
            .assumed(
                "manual p.45 says presets are organized 'in just the same way'; the preset location range and the location text are not stated",
            ),
            I::choice(
                "organize.2.section",
                "Preset section",
                &["Organ", "Piano", "Synth"],
                0,
                45,
                UiOnly("chooses which Preset Library section the page organizes; Shift + PROGRAM dial"),
            )
            .assumed(
                "the manual does not say how the section is chosen: Organ, Piano and Synth presets live in separate banks (p.41)",
            ),
            I::action("organize.2.swap", "Swap", 45, UiOnly(PRESETS)).assumed(
                "manual p.45 says presets are organized 'in just the same way'; preset location ranges are not stated",
            ),
            I::action("organize.2.move", "Move", 45, UiOnly(PRESETS)).assumed("same as organize.2.swap"),
            I::action("organize.2.ok", "Ok", 46, UiOnly(PRESETS)).assumed("same as organize.2.swap"),
            I::action("organize.2.undo", "Undo", 46, UiOnly(PRESETS)).assumed("same as organize.2.swap"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pages_program_then_preset() {
        assert_eq!(PAGES.len(), 2);
        assert_eq!(PAGES[0].items.len(), 5);
        assert_eq!(PAGES[1].items.len(), 6);
    }

    #[test]
    fn both_pages_are_ui_only_with_a_reason() {
        use crate::types::Binding;
        for page in PAGES {
            assert!(page
                .items
                .iter()
                .all(|i| matches!(i.binding, Binding::UiOnly(r) if !r.is_empty())));
        }
    }

    #[test]
    fn preset_page_leads_with_the_location_the_dial_edits() {
        let ids: Vec<_> = PAGES[1].items.iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            [
                "organize.2.location",
                "organize.2.section",
                "organize.2.swap",
                "organize.2.move",
                "organize.2.ok",
                "organize.2.undo"
            ]
        );
        assert!(PAGES[1].items.iter().all(|i| i.note.len() > 10));
    }
}
