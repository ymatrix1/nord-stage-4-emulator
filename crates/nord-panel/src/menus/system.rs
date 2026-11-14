//! System menu, 6 pages (manual OS v1.6x p.57). SHIFT + Program 1.

use super::{Menu, MenuItem as I, MenuPage};
use crate::types::Binding::{Bound, UiOnly};

const LED: &[&str] = &["Low", "High"];
const INFO: &[&str] = &["Version", "HW"];

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::System,
        page: 1,
        title: "Memory Protect",
        items: &[I::toggle(
            "system.1.memory_protect",
            "Memory Protect",
            true,
            57,
            UiOnly("stores are never blocked in the plugin; the host owns persistence"),
        )],
    },
    MenuPage {
        menu: Menu::System,
        page: 2,
        title: "Global Transpose",
        items: &[I::int(
            "system.2.transpose",
            "Global Transpose",
            (-6, 6, 1),
            0,
            57,
            Bound("sys_transpose"),
        )],
    },
    MenuPage {
        menu: Menu::System,
        page: 3,
        title: "Fine Tune",
        items: &[I::int(
            "system.3.fine_tune",
            "Fine Tune",
            (-50, 50, 1),
            0,
            57,
            Bound("sys_fine_tune"),
        )],
    },
    MenuPage {
        menu: Menu::System,
        page: 4,
        title: "LED Intensity",
        items: &[I::choice(
            "system.4.led_intensity",
            "LED Intensity",
            LED,
            1,
            57,
            UiOnly("panel lighting only"),
        )],
    },
    MenuPage {
        menu: Menu::System,
        page: 5,
        title: "Reset all settings",
        items: &[I::action(
            "system.5.reset_all",
            "Reset",
            57,
            UiOnly("panel action; resets menu values to the defaults of this model"),
        )],
    },
    MenuPage {
        menu: Menu::System,
        page: 6,
        title: "Version and Model Info",
        items: &[I::choice(
            "system.6.version_info",
            "Version / HW",
            INFO,
            0,
            57,
            UiOnly("read-only information"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_six_pages_in_order() {
        let pages: Vec<u8> = PAGES.iter().map(|p| p.page).collect();
        assert_eq!(pages, vec![1, 2, 3, 4, 5, 6]);
        assert!(PAGES.iter().all(|p| p.menu == Menu::System));
    }
}
