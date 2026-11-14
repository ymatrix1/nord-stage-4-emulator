//! Unit tests of `oled`: the home views, Layer Init and the Store screens.

use super::*;

fn sum() -> Summary {
    Summary {
        organ: "B3".into(),
        piano: "Grand".into(),
        synth: "SuperSaw".into(),
        piano_size: None,
        organ_bars: None,
        layers_on: [true, false, true, false, true, false, false],
    }
}

#[test]
fn four_prog_views_and_preset_name() {
    let mut u = ProgramUi::default();
    u.title = "Sweet".into();
    let t = compose(&u, &sum());
    assert_eq!((t.big.as_str(), t.sub.as_str()), ("A:11", "Sweet"));
    u.preset_name = true;
    assert!(compose(&u, &sum()).sub.contains("B3 / Grand / SuperSaw"));
    u.preset_name = false;
    u.view = u.view.next();
    let t = compose(&u, &sum());
    let icons = [ICON_ORGAN, ICON_PIANO, ICON_SYNTH];
    assert_eq!(
        t.rows,
        [
            format!("{}B3", icons[0]),
            format!("{}Grand", icons[1]),
            format!("{}SuperSaw", icons[2])
        ]
    );
    assert!(t.big.contains("Sweet"));
    u.view = u.view.next();
    let t = compose(&u, &sum());
    assert!(t.rows[0].contains("Org A") && t.rows[0].contains("Syn A"));
    u.view = u.view.next();
    let t = compose(&u, &sum());
    assert_eq!(t.rows.len(), 8);
    assert!(t.rows[0].starts_with('>'), "current program is highlighted");
}

#[test]
fn layer_init_screen_matches_the_hardware_prompt_and_letters() {
    let mut u = ProgramUi::default();
    u.screen = Screen::LayerInit;
    u.focus = LayerId::SynthC;
    let t = compose(&u, &sum());
    assert_eq!(t.big, "LAYER INIT");
    assert_eq!(t.sub, "Press to initialize");
    assert_eq!(t.rows[0], "Organ AB  Piano A  Synth C");
    assert_eq!(t.soft, ["All", "Organ", "Piano", "Synth"]);
    u.focus = LayerId::PianoB;
    assert_eq!(compose(&u, &sum()).rows[0], "Organ AB  Piano B  Synth A");
}

#[test]
fn store_screens_show_target_and_name_cursor() {
    let mut u = ProgramUi::default();
    u.title = "Rusty".into();
    u.store_as();
    let t = compose(&u, &sum());
    assert_eq!(t.big, "STORE PROGRAM AS");
    assert!(t.sub.starts_with("[R]usty"), "{}", t.sub);
    assert_eq!(t.soft, ["Abc", "Cat", "Del", "Ins"]);
    assert_eq!(t.rows[0], "Cat: Undefined");
    u.store_press();
    let t = compose(&u, &sum());
    assert_eq!(t.big, "STORE PROGRAM TO");
    assert_eq!(t.sub, "A:11");
    // Unused target: `<empty>`, then the name that will be stored.
    assert_eq!(t.rows, ["<empty>", "As: Rusty"]);
}
