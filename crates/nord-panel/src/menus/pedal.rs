//! Pedal menu, 7 pages (manual OS v1.6x p.60-61). SHIFT + Program 6.
//!
//! Pedal "Type" entries describe the electrical behaviour of a physical jack; a plugin receives
//! normalised CC values, so they are `UiOnly`.

use super::{Menu, MenuItem as I, MenuPage, GAP};
use crate::types::Binding::{Bound, UiOnly, Unsupported as U};

const EXPR: &[&str] = &["Nord EP", "Roland", "Yamaha", "Korg", "Fatar"];
const CTRL_TYPE: &[&str] = &["Nord EP", "Roland", "Yamaha", "Korg", "Fatar", "Nord SP-2"];
const CTRL_FUNC: &[&str] = &["Control", "Ctrl + Swell"];
const SWELL_FUNC: &[&str] = &["Swell", "Volume", "Wheel Morph"];
const SUSTAIN_TYPE: &[&str] = &[
    "Nord SP-1",
    "Nord SP-2",
    "Nord TP-1",
    "Normally Open",
    "Normally Closed",
];
const RIGHT: &[&str] = &["Sustain", "Sus+Rotor Hold", "Sus+Rotor Toggle"];
const LEFT: &[&str] = &[
    "Una Corda",
    "Panel Assigned",
    "Layer Scene Hold",
    "Layer Scene Toggle",
    "Pump",
    "Mst Clk Tap",
    "Synth Vibrato",
    "Program Up",
    "Program Down",
    "Rotor Hold",
    "Rotor Toggle",
    "Control",
];
const MIDDLE: &[&str] = &[
    "Sostenuto",
    "User Assigned",
    "Layer Scene Hold",
    "Layer Scene Toggle",
    "Pump",
    "Mst Clk Tap",
    "Synth Vibrato",
    "Program Up",
    "Program Down",
    "Rotor Hold",
    "Rotor Toggle",
    "Control",
];
const ROTOR_TYPE: &[&str] = &[
    "Nord SP-1",
    "Normally Open",
    "Normally Closed",
    "Half Moon Switch",
];
const ROTOR_FUNC: &[&str] = &["Rotor Toggle", "Rotor Hold"];
const FS_TYPE: &[&str] = &["Single Open", "Single Closed", "Dual Open", "Dual Closed"];
const FS_FUNC: &[&str] = &[
    "User Assigned",
    "Layer Scene Hold",
    "Layer Scene Toggle",
    "Pump",
    "Mst Clk Tap",
    "Synth Vibrato",
    "Program Up",
    "Program Down",
    "Rotor Hold",
    "Rotor Toggle",
];

const GAIN: (f32, f32, f32) = (1.0, 10.0, 0.1);
const GAIN_NOTE: &str =
    "range 1-10 from manual; default 1 and the 0.1 step are ASSUMED (engine default)";
const JACK: &str = "jack electrical type; the host sends normalised values";
const TRIPLE: &str = "engine:pedals.triple.funcs";
const SUSTAIN_NOTE: &str = "choices and default from manual p.60; the engine default is SP-2 (continuous) so half pedaling works, the hardware default SP-1 is not applied";
const TP1: &str = "present only when Sustain Type is Nord TP-1";

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::Pedal,
        page: 1,
        title: "Control Pedal",
        items: &[
            I::choice("pedal.1.ctrl_type", "Type", CTRL_TYPE, 0, 60, UiOnly(JACK)),
            I::choice(
                "pedal.1.ctrl_func",
                "Func",
                CTRL_FUNC,
                0,
                60,
                Bound("ped_ctrl_func"),
            ),
            I::float("pedal.1.ctrl_gain", "Gain", GAIN, 1.0, 60, Bound("ped_ctrl_gain"))
                .assumed(GAIN_NOTE),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 2,
        title: "Swell Pedal",
        items: &[
            I::choice("pedal.2.swell_type", "Type", EXPR, 0, 60, UiOnly(JACK)),
            I::choice(
                "pedal.2.swell_func",
                "Func",
                SWELL_FUNC,
                0,
                60,
                Bound("ped_swell_func"),
            ),
            I::float("pedal.2.swell_gain", "Gain", GAIN, 1.0, 60, Bound("ped_swell_gain"))
                .assumed(GAIN_NOTE),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 3,
        title: "Sustain Pedal",
        items: &[
            I::choice(
                "pedal.3.sustain_type",
                "Type",
                SUSTAIN_TYPE,
                0,
                60,
                Bound("engine:pedals.triple.sustain_type"),
            )
            .note(SUSTAIN_NOTE),
            I::choice("pedal.3.left", "Left", LEFT, 0, 60, Bound(TRIPLE)).note(TP1),
            I::choice("pedal.3.middle", "Mid", MIDDLE, 0, 60, Bound(TRIPLE)).note(TP1),
            I::choice(
                "pedal.3.right",
                "Right",
                RIGHT,
                0,
                60,
                Bound("ped_right"),
            ),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 4,
        title: "Triple Pedal Function",
        items: &[
            I::choice("pedal.4.left", "Left", LEFT, 0, 60, Bound(TRIPLE)),
            I::choice("pedal.4.middle", "Middle", MIDDLE, 0, 60, Bound(TRIPLE)),
            I::choice("pedal.4.right", "Right", RIGHT, 0, 60, Bound("ped_right"))
                .note("same host parameter as pedal.3.right; the engine has one right-pedal state"),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 5,
        title: "Rotor Pedal",
        items: &[
            I::choice("pedal.5.rotor_type", "Type", ROTOR_TYPE, 0, 61, UiOnly(JACK)),
            I::choice(
                "pedal.5.rotor_func",
                "Func",
                ROTOR_FUNC,
                0,
                61,
                Bound("ped_rotor_func"),
            ),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 6,
        title: "Foot Switch",
        items: &[
            I::choice("pedal.6.fs_type", "Type", FS_TYPE, 0, 61, UiOnly(JACK))
                .assumed("manual lists the choices without a default; first assumed"),
            I::choice("pedal.6.fs_a", "SW A", FS_FUNC, 0, 61, Bound("ped_foot_func")).assumed(
                "default not stated; ped_foot_func acts on 4 of the 10 choices (Layer Scene Hold/Toggle, Rotor Hold/Toggle)",
            ),
            I::choice("pedal.6.fs_b", "SW B", FS_FUNC, 0, 61, U(GAP))
                .assumed("default not stated; the engine has a single foot switch input"),
        ],
    },
    MenuPage {
        menu: Menu::Pedal,
        page: 7,
        title: "Pedal Overview",
        items: &[I::action(
            "pedal.7.focus",
            "Focus a pedal setting",
            61,
            UiOnly("the overview edits the settings of pages 1-6"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_seven_pages_and_the_manual_defaults() {
        assert_eq!(PAGES.len(), 7);
        assert_eq!(LEFT[0], "Una Corda");
        assert_eq!(MIDDLE[0], "Sostenuto");
        assert_eq!(RIGHT[0], "Sustain");
    }
}
