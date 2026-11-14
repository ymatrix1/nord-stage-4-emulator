//! Extern menu, 13 pages (manual OS v1.6x p.63-64). SHIFT + Program 8. Stored per program.
//!
//! Each per-layer page has three items (Extern A, B, C). The manual numbers the entries 1-13 with
//! "8-12" for the five Device MIDI entries; their order (Mod Wheel, Aftertouch, Control Pedal,
//! Swell Pedal, Volume Pedal) follows the list in the text and is `ASSUMED`.

use super::{Menu, MenuItem as I, MenuPage};
use crate::types::Binding::Bound;

const VELOCITY: &[&str] = &["Soft", "Mid", "Hard"];
/// Bank select values; -1 is "--" (Off).
const BANK: (i32, i32, i32) = (-1, 127, 1);
const PC: (i32, i32, i32) = (0, 127, 1);
const CC: (i32, i32, i32) = (0, 119, 1);

const DEVICE_ORDER: &str =
    "page order of the five Device entries is ASSUMED from the manual's list";
const BANK_NOTE: &str = "-1 = \"--\" (Off); the engine stores a plain byte and has no Off state";

macro_rules! per_layer {
    ($page:literal, $name:literal, $label:literal, $choice:expr, $def:expr, $bind:expr) => {
        [
            I::choice(
                concat!("extern.", $page, ".", $name, "_a"),
                concat!($label, " A"),
                $choice,
                $def,
                63,
                $bind,
            ),
            I::choice(
                concat!("extern.", $page, ".", $name, "_b"),
                concat!($label, " B"),
                $choice,
                $def,
                63,
                $bind,
            ),
            I::choice(
                concat!("extern.", $page, ".", $name, "_c"),
                concat!($label, " C"),
                $choice,
                $def,
                63,
                $bind,
            ),
        ]
    };
}

macro_rules! device_toggles {
    ($page:literal, $name:literal, $label:literal) => {
        [
            I::toggle(
                concat!("extern.", $page, ".", $name, "_a"),
                concat!($label, " A"),
                true,
                64,
                Bound("engine:extern_menu.devices"),
            )
            .assumed(DEVICE_ORDER),
            I::toggle(
                concat!("extern.", $page, ".", $name, "_b"),
                concat!($label, " B"),
                true,
                64,
                Bound("engine:extern_menu.devices"),
            )
            .assumed(DEVICE_ORDER),
            I::toggle(
                concat!("extern.", $page, ".", $name, "_c"),
                concat!($label, " C"),
                true,
                64,
                Bound("engine:extern_menu.devices"),
            )
            .assumed(DEVICE_ORDER),
        ]
    };
}

macro_rules! per_layer_int {
    ($page:literal, $name:literal, $label:literal, $range:expr, $def:expr, $bind:expr, $note:expr) => {
        [
            I::int(
                concat!("extern.", $page, ".", $name, "_a"),
                concat!($label, " A"),
                $range,
                $def,
                64,
                $bind,
            )
            .note($note),
            I::int(
                concat!("extern.", $page, ".", $name, "_b"),
                concat!($label, " B"),
                $range,
                $def,
                64,
                $bind,
            )
            .note($note),
            I::int(
                concat!("extern.", $page, ".", $name, "_c"),
                concat!($label, " C"),
                $range,
                $def,
                64,
                $bind,
            )
            .note($note),
        ]
    };
}

static ECHO: [I; 1] = [I::toggle(
    "extern.1.echo_global",
    "Echo Global to Extern Ch",
    true,
    63,
    Bound("engine:extern_menu.echo_global"),
)
.assumed(
    "manual default is Off; the emulator feeds the keybed and MIDI In through one path, so On keeps Extern layers playable from MIDI In (engine/extern_menu.rs)",
)];
static VEL: [I; 3] = per_layer!(
    "2",
    "velocity",
    "Keyboard Velocity",
    VELOCITY,
    1,
    Bound("engine:extern_menu.velocity")
);
static MSB: [I; 3] = per_layer_int!(
    "3",
    "bank_msb",
    "Program Change MSB",
    BANK,
    -1,
    Bound("engine:extern_out.layers.bank_msb"),
    BANK_NOTE
);
static LSB: [I; 3] = per_layer_int!(
    "4",
    "bank_lsb",
    "Program Change LSB",
    BANK,
    -1,
    Bound("engine:extern_out.layers.bank_lsb"),
    BANK_NOTE
);
static PCH: [I; 3] = per_layer_int!(
    "5",
    "program",
    "PC",
    PC,
    0,
    Bound("engine:extern_out.layers.program"),
    "0-127"
);
static CC1: [I; 3] = per_layer_int!(
    "6",
    "cc1",
    "MIDI CC #1",
    CC,
    2,
    Bound("engine:extern_out.layers.cc_numbers"),
    "first of the two user controller numbers"
);
static CC2: [I; 3] = per_layer_int!(
    "7",
    "cc2",
    "MIDI CC #2",
    CC,
    2,
    Bound("engine:extern_out.layers.cc_numbers"),
    "second of the two user controller numbers"
);
static MOD: [I; 3] = device_toggles!("8", "mod_wheel", "Mod Wheel");
static AFT: [I; 3] = device_toggles!("9", "aftertouch", "Aftertouch");
static CTL: [I; 3] = device_toggles!("10", "control_pedal", "Control Pedal");
static SWL: [I; 3] = device_toggles!("11", "swell_pedal", "Swell Pedal");
static VOL: [I; 3] = device_toggles!("12", "volume_pedal", "Volume Pedal");
static SOL: [I; 3] = [
    I::toggle(
        "extern.13.send_on_load_a",
        "Send On Load A",
        false,
        64,
        Bound("engine:extern_out.layers.send_on_load"),
    ),
    I::toggle(
        "extern.13.send_on_load_b",
        "Send On Load B",
        false,
        64,
        Bound("engine:extern_out.layers.send_on_load"),
    ),
    I::toggle(
        "extern.13.send_on_load_c",
        "Send On Load C",
        false,
        64,
        Bound("engine:extern_out.layers.send_on_load"),
    ),
];

const fn page(n: u8, title: &'static str, items: &'static [I]) -> MenuPage {
    MenuPage {
        menu: Menu::Extern,
        page: n,
        title,
        items,
    }
}

pub(super) const PAGES: &[MenuPage] = &[
    page(1, "Echo Global to Extern Ch", &ECHO),
    page(2, "Keyboard Velocity A/B/C", &VEL),
    page(3, "Program Change MSB A/B/C", &MSB),
    page(4, "Program Change LSB A/B/C", &LSB),
    page(5, "Program Change A/B/C", &PCH),
    page(6, "MIDI CC #1 A/B/C", &CC1),
    page(7, "MIDI CC #2 A/B/C", &CC2),
    page(8, "ModWheel MIDI", &MOD),
    page(9, "Aftertouch MIDI", &AFT),
    page(10, "Ctrl Pedal MIDI", &CTL),
    page(11, "Swell Pedal MIDI", &SWL),
    page(12, "Volume Pedal MIDI", &VOL),
    page(13, "Send On Load A/B/C", &SOL),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_thirteen_pages_with_one_item_per_layer() {
        assert_eq!(PAGES.len(), 13);
        assert_eq!(PAGES[0].items.len(), 1);
        assert!(PAGES[1..].iter().all(|p| p.items.len() == 3));
    }
}
