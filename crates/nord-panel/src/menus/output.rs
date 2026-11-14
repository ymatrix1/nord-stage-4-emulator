//! Output menu, 6 pages (manual OS v1.6x p.59). SHIFT + Program 5.
//!
//! Routing is not modelled in the engine (single stereo pair); the one honoured setting is the
//! mono downmix (`sys_mono`), the emulator's form of "Mono Output" (a bus on a single output).

use super::{Menu, MenuItem as I, MenuPage, GAP};
use crate::types::Binding::{Bound, UiOnly, Unsupported as U};

const MODE: &[&str] = &["Global", "Per Program"];
/// Output choices of Global mode.
const OUT: &[&str] = &["1-2", "3-4", "1", "2", "3", "4", "12+34"];
/// Output choices with the "follows the layer to the left" arrow (Layer B/C, Rotary).
const OUT_ARROW: &[&str] = &["1-2", "3-4", "1", "2", "3", "4", "12+34", "<-"];
const OUT_AUTO: &[&str] = &["1-2", "3-4", "1", "2", "3", "4", "12+34", "Auto"];
/// Bus choices of Per Program mode.
const BUS: &[&str] = &["Main", "Sub I", "Sub II", "Sub III"];
const BUS_ARROW: &[&str] = &["Main", "Sub I", "Sub II", "Sub III", "<-"];
const BUS_AUTO: &[&str] = &["Main", "Sub I", "Sub II", "Sub III", "Auto"];

const ARROW: &str = "the arrow follows the layer to the left; its glyph is lost in the PDF text";

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::Output,
        page: 1,
        title: "Output Routing Mode",
        items: &[I::choice(
            "output.1.routing_mode",
            "Output Routing Mode",
            MODE,
            0,
            59,
            U(GAP),
        )],
    },
    MenuPage {
        menu: Menu::Output,
        page: 2,
        title: "Output Bus Channels (Per Program)",
        items: &[
            I::choice("output.2.bus_main", "Main", OUT, 0, 59, U(GAP)),
            I::choice("output.2.bus_sub1", "Sub I", OUT, 1, 59, U(GAP)),
            I::choice("output.2.bus_sub2", "Sub II", OUT, 4, 59, U(GAP)),
            I::choice("output.2.bus_sub3", "Sub III", OUT, 5, 59, U(GAP)),
            I::toggle("output.2.mono", "Mono Output", false, 59, Bound("sys_mono")).assumed(
                "manual p.59 describes Mono Output as a bus on a single output; this switch is the emulator's stereo-pair equivalent",
            ),
        ],
    },
    MenuPage {
        menu: Menu::Output,
        page: 3,
        title: "Organ/Rotary Output Channel/Bus",
        items: &[
            I::choice("output.3.organ_out", "Organ output (Global)", OUT, 0, 59, U(GAP))
                .assumed("Global-mode default not stated; 1-2 assumed"),
            I::choice("output.3.organ_bus", "Organ bus (Per Program)", BUS, 0, 59, U(GAP)),
            I::choice(
                "output.3.rotary_out",
                "Rotary output (Global)",
                OUT_AUTO,
                7,
                59,
                U(GAP),
            ),
            I::choice(
                "output.3.rotary_bus",
                "Rotary bus (Per Program)",
                BUS_AUTO,
                4,
                59,
                U(GAP),
            ),
        ],
    },
    MenuPage {
        menu: Menu::Output,
        page: 4,
        title: "Piano Output Channel/Bus",
        items: &[
            I::choice("output.4.piano_a_out", "Piano A output (Global)", OUT, 0, 59, U(GAP))
                .assumed("Global-mode default not stated; 1-2 assumed"),
            I::choice("output.4.piano_a_bus", "Piano A bus (Per Program)", BUS, 0, 59, U(GAP)),
            I::choice(
                "output.4.piano_b_out",
                "Piano B output (Global)",
                OUT_ARROW,
                7,
                59,
                U(GAP),
            )
            .note(ARROW),
            I::choice(
                "output.4.piano_b_bus",
                "Piano B bus (Per Program)",
                BUS_ARROW,
                4,
                59,
                U(GAP),
            )
            .note(ARROW),
        ],
    },
    MenuPage {
        menu: Menu::Output,
        page: 5,
        title: "Synth Output Channel/Bus",
        items: &[
            I::choice("output.5.synth_a_out", "Synth A output (Global)", OUT, 0, 59, U(GAP))
                .assumed("Global-mode default not stated; 1-2 assumed"),
            I::choice("output.5.synth_a_bus", "Synth A bus (Per Program)", BUS, 0, 59, U(GAP)),
            I::choice(
                "output.5.synth_b_out",
                "Synth B output (Global)",
                OUT_ARROW,
                7,
                59,
                U(GAP),
            )
            .note(ARROW),
            I::choice(
                "output.5.synth_b_bus",
                "Synth B bus (Per Program)",
                BUS_ARROW,
                4,
                59,
                U(GAP),
            )
            .note(ARROW),
            I::choice(
                "output.5.synth_c_out",
                "Synth C output (Global)",
                OUT_ARROW,
                7,
                59,
                U(GAP),
            )
            .note(ARROW),
            I::choice(
                "output.5.synth_c_bus",
                "Synth C bus (Per Program)",
                BUS_ARROW,
                4,
                59,
                U(GAP),
            )
            .note(ARROW),
        ],
    },
    MenuPage {
        menu: Menu::Output,
        page: 6,
        title: "Output Overview",
        items: &[I::action(
            "output.6.show_outputs",
            "Out (show physical outputs)",
            59,
            UiOnly("display toggle; the overview edits the settings of pages 3-5"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_six_pages_and_bus_defaults_match_the_manual() {
        assert_eq!(PAGES.len(), 6);
        let defaults: Vec<_> = PAGES[1].items[..4].iter().map(|i| i.default).collect();
        // Main 1-2, Sub I 3-4, Sub II 3, Sub III 4 (manual p.59)
        use super::super::Value::Index;
        assert_eq!(defaults, vec![Index(0), Index(1), Index(4), Index(5)]);
        assert_eq!(OUT[4], "3");
        assert_eq!(OUT[5], "4");
    }
}
