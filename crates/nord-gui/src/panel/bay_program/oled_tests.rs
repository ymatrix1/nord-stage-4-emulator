//! More unit tests of `oled`.

use super::super::model::MenuId;
use super::super::store::ProgLoc;
use super::*;
use crate::panel::bay_program::morph::SOURCES;

fn sum() -> Summary {
    Summary {
        organ: "B3".into(),
        piano: "Grand".into(),
        synth: "SuperSaw".into(),
        layers_on: [true, false, true, false, true, false, false],
        ..Default::default()
    }
}

#[test]
fn split_screen_has_note_and_xfade_rows_with_soft_buttons() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Split;
    let t = compose(&u, &sum());
    assert!(
        t.rows[0].contains("Note") && t.rows[0].contains(">M:C4"),
        "{:?}",
        t.rows
    );
    assert!(t.rows[1].contains("xFade"));
    assert_eq!(t.soft, ["Note", "Low", "Mid", "High"]);
    assert!(t.rows[0].contains("L:Off"));
}

#[test]
fn menus_list_and_preset_screens_have_their_titles() {
    let mut u = ProgramUi::default();
    u.open_menu(MenuId::Pedal);
    let t = compose(&u, &sum());
    assert_eq!(t.big, "PEDAL SETTINGS");
    assert_eq!(t.sub, "1/7 Control Pedal");
    assert!(t.rows[0].starts_with(">Type: Nord EP"), "{:?}", t.rows);
    // A menu screen set without `open_menu` shows the title and manual page only.
    u.screen = Screen::Menu(MenuId::Midi);
    let t = compose(&u, &sum());
    assert!(t.sub.contains("p.62") && t.rows.is_empty());
    u.sync_catalog(
        &["P/a.ns4p".into(), "P/b.ns4p".into()],
        &["x/p.npno".into()],
        &[],
    );
    u.screen = Screen::List;
    u.list_cursor = 1;
    let t = compose(&u, &sum());
    assert_eq!(t.sub, "b");
    assert!(t.rows[1].starts_with('>'));
    u.screen = Screen::Preset {
        kind: PresetKind::Piano,
        single: true,
    };
    let t = compose(&u, &sum());
    assert_eq!(t.big, "PIANO PRESET");
    assert!(t.rows[0].starts_with("Layer: Pno A"));
    assert_eq!(
        compose(
            &ProgramUi {
                screen: Screen::Preset {
                    kind: PresetKind::Organ,
                    single: false
                },
                ..ProgramUi::default()
            },
            &sum()
        )
        .sub,
        "B3"
    );
}

#[test]
fn clock_transpose_morph_numpad_and_message_text() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Clock;
    assert_eq!(compose(&u, &sum()).sub, "120 BPM");
    u.screen = Screen::Transpose;
    u.transp.dial(2);
    assert_eq!(compose(&u, &sum()).sub, "+2 st on");
    u.morph.double_tap(SOURCES[0]);
    u.dial(2, false);
    u.screen = Screen::Morph(SOURCES[0]);
    let t = compose(&u, &sum());
    assert_eq!(t.big, "MORPH WHEEL");
    assert_eq!(t.rows[0], "Org A Lvl +0.10");
    u.screen = Screen::NumPad;
    u.num_first = Some(3);
    assert_eq!(compose(&u, &sum()).sub, "A:3-");
    u.screen = Screen::Home;
    u.set_message("Stored A:11");
    assert_eq!(compose(&u, &sum()).rows.last().unwrap(), "Stored A:11");
    assert_eq!(name_at(&u, ProgLoc::default()), "");
    assert!(compose(&u, &sum()).plain().contains("A:11"));
}

/// VERIFIED: `Nothing to Undo` and `Nothing to Paste` occur in `bank0_CODE.bin` (OS v1.66,
/// `0x2BFB3C`, `0x28377D`); `tools/tests/test_ns4_oled_strings.py::test_fixed_wording_is_in_the_firmware`.
#[test]
fn undo_and_paste_messages_use_the_firmware_wording() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Undo;
    u.undo_blocked = true;
    assert_eq!(compose(&u, &sum()).sub, "Nothing to Undo");
    u.screen = Screen::Paste;
    u.copied = None;
    u.soft_down(1);
    assert_eq!(u.message.as_ref().unwrap().0, "Nothing to Paste");
}

#[test]
fn menu_titles_are_the_settings_titles_the_display_prints() {
    // Frames of the ARM emulator: the title bar upper-cases the `... Settings` strings.
    assert_eq!(MenuId::AuxKb.title(), "AUX KB SETTINGS");
    assert_eq!(MenuId::Midi.title(), "MIDI SETTINGS");
    assert_eq!(MenuId::Extern.title(), "EXTERN SETTINGS");
    for m in MenuId::ALL {
        if m != MenuId::Organize {
            assert!(m.title().ends_with(" SETTINGS"), "{m:?}");
            assert_eq!(m.title(), m.title().to_uppercase(), "{m:?}");
        }
    }
    // Program Organize is a list page with its own header: mixed case.
    assert_eq!(MenuId::Organize.title(), "Program Organize");
}
