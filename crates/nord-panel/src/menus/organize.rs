//! Organize (manual OS v1.6x p.45-46). SHIFT + Program 3. Programs and presets can be rearranged
//! with Swap and Move; the same flow works for Presets.

use super::{Menu, MenuItem as I, MenuPage, GAP};
use crate::types::Binding::{UiOnly, Unsupported as U};

/// Page 1 acts on the in-memory program library, not on a host parameter or an engine field.
const LIBRARY: &str =
    "acts on the in-memory program library (nord_core::library_ops), no engine parameter";

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
            I::action("organize.2.swap", "Swap", 45, U(GAP)).assumed(
                "manual p.45 says presets are organized 'in just the same way'; preset location ranges are not stated",
            ),
            I::action("organize.2.move", "Move", 45, U(GAP)).assumed("same as organize.2.swap"),
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
    }

    #[test]
    fn program_page_is_ui_only_and_preset_page_still_a_gap() {
        use crate::types::Binding;
        assert!(PAGES[0]
            .items
            .iter()
            .all(|i| matches!(i.binding, Binding::UiOnly(r) if !r.is_empty())));
        assert!(PAGES[1]
            .items
            .iter()
            .all(|i| matches!(i.binding, Binding::Unsupported(GAP))));
    }
}
