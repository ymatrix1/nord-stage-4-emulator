//! Sound menu, 11 pages (manual OS v1.6x p.58). SHIFT + Program 2.
//!
//! Web cross-check (names and choice sets only, no values taken): the Stage 3 Sound menu lists the
//! same tonewheel, click and rotary entries, <https://manualshive.com/en/manual/nord/stage-3/1701214?page=20>;
//! a Stage 4 user forum post shows the same B3 menu labels, <https://www.norduserforum.com/viewtopic.php?p=183703>.

use super::{Menu, MenuItem as I, MenuPage, GAP};
use crate::types::Binding::{Bound, Unsupported};

const LNH: &[&str] = &["Low", "Normal", "High"];
const TRIGGER: &[&str] = &["High", "Low"];
const TONEWHEEL: &[&str] = &["Clean", "Vintage 1", "Vintage 2"];
const CLICK: &[&str] = &["Normal", "High"];
const BALANCE: &[&str] = &["70/30", "60/40", "50/50", "40/60", "30/70"];

const DB_NOTE: &str = "range and default from manual p.58; the 0.5 dB step is ASSUMED";

pub(super) const PAGES: &[MenuPage] = &[
    MenuPage {
        menu: Menu::Sound,
        page: 1,
        title: "Program Levels",
        items: &[I::float(
            "sound.1.program_levels",
            "Program Levels",
            (-12.0, 12.0, 0.1),
            0.0,
            58,
            Bound("prog_level_db"),
        )
        .assumed(
            "manual p.46/58 give no range (shows -3.5 dB); +/-12 dB matches the engine limit",
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 2,
        title: "Piano Pedal Noise Level",
        items: &[I::float(
            "sound.2.piano_pedal_noise",
            "Piano Pedal Noise Level",
            (-6.0, 6.0, 0.5),
            0.0,
            58,
            Bound("sys_piano_noise_db"),
        )
        .note(DB_NOTE)],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 3,
        title: "Piano String Res Level",
        items: &[I::float(
            "sound.3.piano_string_res",
            "Piano String Res Level",
            (-6.0, 6.0, 0.5),
            0.0,
            58,
            Bound("sys_piano_res_db"),
        )
        .note(DB_NOTE)],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 4,
        title: "Organ Keyboard Trig",
        items: &[I::choice(
            "sound.4.organ_trigger_point",
            "Organ Keyboard Trig",
            TRIGGER,
            0,
            58,
            Unsupported(GAP),
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 5,
        title: "B3 Organ Tone-Wheel",
        items: &[I::choice(
            "sound.5.b3_tonewheel_mode",
            "B3 Organ Tone-Wheel",
            TONEWHEEL,
            1,
            58,
            Bound("sys_tonewheel"),
        )
        .empirical(
            "choices from manual p.58, labels corroborated by https://www.norduserforum.com/viewtopic.php?p=183703; crosstalk factors 0 / 1 / 1.5 are ASSUMED",
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 6,
        title: "B3 Organ Click Level",
        items: &[I::choice(
            "sound.6.b3_click_level",
            "B3 Organ Click Level",
            CLICK,
            0,
            58,
            Bound("sys_click_high"),
        )
        .note("choices and default from manual p.58; High = 2x amplitude is ASSUMED")],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 7,
        title: "Rotary Bass/Horn level",
        items: &[I::choice(
            "sound.7.rotary_bass_horn",
            "Rotary Bass/Horn level",
            BALANCE,
            2,
            58,
            Bound("sys_rotary_balance"),
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 8,
        title: "Rotary Rotor Speed",
        items: &[I::choice(
            "sound.8.rotary_rotor_speed",
            "Rotary Rotor Speed",
            LNH,
            1,
            58,
            Bound("sys_rotor_speed"),
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 9,
        title: "Rotary Rotor Acc.",
        items: &[I::choice(
            "sound.9.rotary_rotor_acc",
            "Rotary Rotor Acc.",
            LNH,
            1,
            58,
            Bound("sys_rotor_acc"),
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 10,
        title: "Rotary Horn Speed",
        items: &[I::choice(
            "sound.10.rotary_horn_speed",
            "Rotary Horn Speed",
            LNH,
            1,
            58,
            Bound("sys_horn_speed"),
        )],
    },
    MenuPage {
        menu: Menu::Sound,
        page: 11,
        title: "Rotary Horn Acc.",
        items: &[I::choice(
            "sound.11.rotary_horn_acc",
            "Rotary Horn Acc.",
            LNH,
            1,
            58,
            Bound("sys_horn_acc"),
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_eleven_single_item_pages() {
        assert_eq!(PAGES.len(), 11);
        assert!(PAGES.iter().all(|p| p.items.len() == 1));
    }
}
