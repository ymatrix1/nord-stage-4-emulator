//! MIDI menu, 10 pages (manual OS v1.6x p.61-62). SHIFT + Program 7.
//!
//! Channels use `0 = Off`, `1..=16` as printed, matching `auxkb_channel` and the engine helpers.

use super::{Menu, MenuItem as I, MenuPage};
use crate::types::Binding::{Bound, UiOnly};

const CH: (i32, i32, i32) = (0, 16, 1);
const CH_NOTE: &str = "0 = Off";
const MODE: &[&str] = &["Off", "Send", "Receive", "Send & Receive"];
const CTRL_TYPE: &[&str] = &["CC", "CC & NRPN"];
const TRANSPOSE_AT: &[&str] = &["In", "Out"];
const ARROW: &str = "0 = Off; the Layer B/C arrow (use Layer A's channel) is not a value";

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::Midi,
        page: 1,
        title: "Local Control",
        items: &[
            I::toggle(
                "midi.1.local_control",
                "Local Control",
                true,
                61,
                Bound("engine:midi_menu.local_control"),
            )
            .note("always reverts to On at power-up (manual p.57), so hosts do not persist it"),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 2,
        title: "Global Channel / Aux KB",
        items: &[
            I::int(
                "midi.2.global_channel",
                "Global Channel",
                CH,
                1,
                61,
                Bound("engine:global_midi_channel"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.2.aux_channel",
                "Aux Keyboard Channel",
                CH,
                16,
                61,
                Bound("auxkb_channel"),
            )
            .note(CH_NOTE),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 3,
        title: "Organ Channel",
        items: &[
            I::int(
                "midi.3.organ_a",
                "Organ A",
                CH,
                0,
                61,
                Bound("engine:layer_midi_channels"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.3.organ_b",
                "Organ B",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(ARROW),
            I::toggle("midi.3.organ_sect", "Sect On", false, 62, Bound("engine:midi_menu.sect_on")),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 4,
        title: "Piano Channel",
        items: &[
            I::int(
                "midi.4.piano_a",
                "Piano A",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.4.piano_b",
                "Piano B",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(ARROW),
            I::toggle("midi.4.piano_sect", "Sect On", false, 62, Bound("engine:midi_menu.sect_on")),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 5,
        title: "Synth Channel",
        items: &[
            I::int(
                "midi.5.synth_a",
                "Synth A",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.5.synth_b",
                "Synth B",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(ARROW),
            I::int(
                "midi.5.synth_c",
                "Synth C",
                CH,
                0,
                62,
                Bound("engine:layer_midi_channels"),
            )
            .note(ARROW),
            I::toggle("midi.5.synth_sect", "Sect On", false, 62, Bound("engine:midi_menu.sect_on")),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 6,
        title: "Extern Channel",
        items: &[
            I::int(
                "midi.6.extern_a",
                "Extern A",
                CH,
                13,
                62,
                Bound("engine:extern_out.layers.channel"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.6.extern_b",
                "Extern B",
                CH,
                14,
                62,
                Bound("engine:extern_out.layers.channel"),
            )
            .note(CH_NOTE),
            I::int(
                "midi.6.extern_c",
                "Extern C",
                CH,
                15,
                62,
                Bound("engine:extern_out.layers.channel"),
            )
            .note(CH_NOTE),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 7,
        title: "Control / NRPN / Device Mode",
        items: &[
            I::choice("midi.7.type", "Type", CTRL_TYPE, 1, 62, Bound("engine:midi_menu.nrpn")),
            I::choice("midi.7.ctrl", "Ctrl", MODE, 3, 63, Bound("engine:midi_menu.ctrl")).assumed(
                "manual default is Off; the engine has always accepted panel CCs on the Global channel, so Send & Receive is the default",
            ),
            I::choice("midi.7.dev", "Dev", MODE, 3, 63, Bound("engine:midi_menu.dev")),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 8,
        title: "Program Change Mode",
        items: &[
            I::choice("midi.8.pc", "PC", MODE, 3, 63, Bound("engine:midi_menu.pc")),
            I::choice("midi.8.pc_ext", "PC Ext", MODE, 3, 63, Bound("engine:midi_menu.pc_ext")),
        ],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 9,
        title: "Transpose MIDI At",
        items: &[I::choice(
            "midi.9.transpose_at",
            "Transpose MIDI At",
            TRANSPOSE_AT,
            0,
            63,
            Bound("engine:midi_menu.transpose_at"),
        )],
    },
    MenuPage {
        menu: Menu::Midi,
        page: 10,
        title: "MIDI Overview",
        items: &[I::action(
            "midi.10.focus",
            "Focus a MIDI setting",
            63,
            UiOnly("the overview edits the settings of pages 1-9"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_ten_pages_and_extern_channel_defaults() {
        assert_eq!(PAGES.len(), 10);
        let ch: Vec<_> = PAGES[5].items.iter().map(|i| i.default).collect();
        use super::super::Value::Int;
        assert_eq!(ch, vec![Int(13), Int(14), Int(15)]);
    }
}
