//! Aux KB menu, 3 pages (manual OS v1.6x p.58; behaviour p.43-45). SHIFT + Program 4.
//! Stored per program. The Aux KB MIDI channel lives on MIDI menu page 2.

use super::{Menu, MenuItem as I, MenuPage};
use crate::types::Binding::Bound;

const ORGAN_PIANO: &[&str] = &["--", "A-", "-B", "AB"];
const SYNTH: &[&str] = &["---", "A--", "-B-", "--C", "AB-", "A-C", "-BC", "ABC"];
const CURVE: &[&str] = &["Soft", "Medium", "Hard"];

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::AuxKb,
        page: 1,
        title: "Aux Keyboard Enable",
        items: &[I::toggle(
            "aux_kb.1.enable",
            "Aux Keyboard Enable",
            false,
            58,
            Bound("auxkb_enable"),
        )],
    },
    MenuPage {
        menu: Menu::AuxKb,
        page: 2,
        title: "Aux Keyboard Layers",
        items: &[
            I::choice(
                "aux_kb.2.organ",
                "Organ",
                ORGAN_PIANO,
                0,
                58,
                Bound("auxkb_organ"),
            ),
            I::choice(
                "aux_kb.2.piano",
                "Piano",
                ORGAN_PIANO,
                0,
                58,
                Bound("auxkb_piano"),
            ),
            I::choice(
                "aux_kb.2.synth",
                "Synth",
                SYNTH,
                0,
                58,
                Bound("auxkb_synth"),
            ),
        ],
    },
    MenuPage {
        menu: Menu::AuxKb,
        page: 3,
        title: "Keyboard Curve",
        items: &[I::choice(
            "aux_kb.3.curve",
            "Keyboard Curve",
            CURVE,
            1,
            58,
            Bound("auxkb_curve"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_three_pages_and_all_synth_layer_combinations() {
        assert_eq!(PAGES.len(), 3);
        assert_eq!(SYNTH.len(), 8);
    }
}
