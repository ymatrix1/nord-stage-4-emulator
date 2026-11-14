//! Headless tests of the menu screens: every page of every menu renders, a click and dial reach
//! the engine for at least one item per menu, and Memory Protect gates STORE.

use super::controls::{Fed, BUTTONS, PROGRAM_BUTTONS};
use super::interact::press;
use super::menu_screen::DIM;
use super::model::{MenuId, Screen};
use super::oled::{compose, Summary};
use crate::audio::{apply_audio_command, AudioCommand};
use crate::panel::ui_state::ProgramUi;
use nord_core::engine::{AuxVelocityCurve, Stage4Engine, SwellFunc, TonewheelMode};
use nord_panel::behaviour::Gesture;
use nord_panel::menus::pages_of;
use nord_panel::Binding;

/// Page counts of the manual (p.45-46, p.57-64), independent of the model under test.
const PAGES: [(MenuId, usize); 8] = [
    (MenuId::System, 6),
    (MenuId::Sound, 11),
    (MenuId::Organize, 2),
    (MenuId::AuxKb, 3),
    (MenuId::Output, 6),
    (MenuId::Pedal, 7),
    (MenuId::Midi, 10),
    (MenuId::Extern, 13),
];

fn spec(id: &str) -> super::controls::Spec {
    *PROGRAM_BUTTONS
        .iter()
        .chain(BUTTONS.iter())
        .find(|s| s.id == id)
        .unwrap()
}

fn tap() -> Fed {
    Fed {
        gesture: Some(Gesture::Tap),
        went_down: false,
        went_up: true,
    }
}

fn down() -> Fed {
    Fed {
        gesture: None,
        went_down: true,
        went_up: false,
    }
}

/// A scripted panel: pointer presses on the Program buttons, the dial, and the DSP thread.
struct Panel {
    ui: ProgramUi,
    engine: Stage4Engine,
}

impl Panel {
    fn new() -> Self {
        Self {
            ui: ProgramUi::default(),
            engine: Stage4Engine::new(48_000.0),
        }
    }

    fn send(&mut self, cmds: Vec<AudioCommand>) {
        let mut vol = 1.0;
        for c in cmds {
            apply_audio_command(c, &mut self.engine, &mut vol);
        }
        while let Ok((t, s)) = self.ui.reply_rx.try_recv() {
            self.ui.accept_snapshot(t, s);
        }
    }

    fn click(&mut self, id: &str, shift: bool) {
        let mut cmds = Vec::new();
        press(&mut self.ui, &spec(id), &tap(), shift, &mut cmds);
        self.send(cmds);
    }

    /// Shift+Program `n` opens menu `n`.
    fn open(&mut self, n: u8) {
        self.click(&format!("program.button.{n}"), true);
    }

    /// SHIFT pressed and released alone is EXIT.
    fn exit(&mut self) {
        self.ui.shift_used = false;
        self.click("program.shift.left", false);
    }

    fn page_up(&mut self, times: usize) {
        for _ in 0..times {
            self.click("program.page_up", false);
        }
    }

    /// Soft button `n` (Program button `n`, pressed without Shift).
    fn soft(&mut self, n: u8) {
        let mut cmds = Vec::new();
        press(
            &mut self.ui,
            &spec(&format!("program.button.{n}")),
            &down(),
            false,
            &mut cmds,
        );
        self.send(cmds);
    }

    fn dial(&mut self, delta: i32) -> usize {
        let cmds = self.ui.dial(delta, false);
        let n = cmds.len();
        self.send(cmds);
        n
    }

    fn text(&self) -> super::oled::OledText {
        compose(&self.ui, &Summary::default())
    }
}

#[test]
fn manual_page_counts_equal_the_model() {
    for (m, n) in PAGES {
        assert_eq!(pages_of(m.model()).len(), n, "{m:?}");
    }
}

#[test]
fn every_page_of_every_menu_renders_all_its_items() {
    for (m, n) in PAGES {
        let mut p = Panel::new();
        p.open(m as u8 + 1);
        assert_eq!(p.ui.screen, Screen::Menu(m));
        for page in 0..n {
            let model = pages_of(m.model())[page];
            if m == MenuId::Organize && page == 0 {
                // Dedicated Organize page: location row, choice row, Swap/Move/Ok/Undo soft buttons.
                let t = p.text();
                assert_eq!(t.sub, "1/2 Program Organize");
                assert_eq!(t.soft, ["Swap", "Move", "Ok", "Undo"]);
                assert!(t.rows[0].starts_with(">A:11"), "{:?}", t.rows);
                p.page_up(1);
                continue;
            }
            if m == MenuId::Organize && page == 1 {
                // Dedicated Preset Organize page: same soft buttons, own title.
                let t = p.text();
                assert_eq!(
                    (t.big.as_str(), t.sub.as_str()),
                    ("PRESET ORGANIZE", "2/2 Preset Organize")
                );
                assert_eq!(t.soft, ["Swap", "Move", "Ok", "Undo"]);
                continue;
            }
            let mut rows = 0;
            let blocks = model.items.len().div_ceil(p.ui.menu.block_size());
            for _ in 0..blocks {
                let t = p.text();
                assert_eq!(t.big, m.title());
                assert_eq!(t.sub, format!("{}/{n} {}", page + 1, model.title), "{m:?}");
                assert!(!t.rows.is_empty() && t.rows.iter().any(|r| r.contains('>')));
                rows += t.rows.len();
                if p.ui.menu.paged() {
                    assert_eq!(t.soft[3], "More");
                    p.soft(4);
                }
            }
            assert_eq!(rows, model.items.len(), "{m:?} page {}", page + 1);
            p.page_up(1);
        }
        assert_eq!(
            p.ui.menu.page_index(),
            n - 1,
            "PAGE+ stops at the last page"
        );
    }
}

#[test]
fn no_menu_page_renders_dimmed_any_more() {
    // Every menu item has a binding; Preset Organize was the last gap (ns4-zb9o).
    for (m, n) in PAGES {
        let mut p = Panel::new();
        p.open(match m {
            MenuId::System => 1,
            MenuId::Sound => 2,
            MenuId::Organize => 3,
            MenuId::AuxKb => 4,
            MenuId::Output => 5,
            MenuId::Pedal => 6,
            MenuId::Midi => 7,
            MenuId::Extern => 8,
        });
        for page in 0..n {
            assert!(
                p.text().rows.iter().all(|r| !r.starts_with(DIM)),
                "{m:?} page {page}"
            );
            p.page_up(1);
        }
    }
}

#[test]
fn preset_organize_page_is_live_and_swaps_through_the_panel_buttons() {
    let mut p = Panel::new();
    p.open(3);
    p.page_up(1);
    let t = p.text();
    assert_eq!(t.big, "PRESET ORGANIZE");
    assert!(t.rows[0].starts_with(">1:01"), "{:?}", t.rows);
    p.soft(1);
    assert!(p.ui.organize.preset.pending.is_some(), "Swap chosen");
}

#[test]
fn a_dial_edit_of_one_item_per_menu_reaches_the_engine() {
    // System: Global Transpose.
    let mut p = Panel::new();
    p.open(1);
    p.page_up(1);
    assert_eq!(p.dial(3), 1);
    assert_eq!(p.engine.global_settings().transpose, 3);
    assert!(p.text().rows[0].contains("+3"), "{:?}", p.text().rows);

    // Sound: B3 Tonewheel Mode (Vintage 1 -> Vintage 2).
    let mut p = Panel::new();
    p.open(2);
    p.page_up(4);
    p.dial(1);
    assert_eq!(
        p.engine.global_settings().tonewheel_mode,
        TonewheelMode::Vintage2
    );

    // Aux KB: Keyboard Curve.
    let mut p = Panel::new();
    p.open(4);
    p.page_up(2);
    p.dial(1);
    assert_eq!(p.engine.aux_kb.curve, AuxVelocityCurve::Hard);

    // Output: Mono Output sits on the second block of page 2 (soft 4 = More).
    let mut p = Panel::new();
    p.open(5);
    p.page_up(1);
    p.soft(4);
    p.soft(2);
    assert!(p.text().rows[1].starts_with('>'));
    p.dial(1);
    assert!(p.engine.global_settings().mono_output);

    // Pedal: Swell pedal function, second soft button.
    let mut p = Panel::new();
    p.open(6);
    p.page_up(1);
    p.soft(2);
    p.dial(1);
    assert_eq!(p.engine.pedals.swell_func, SwellFunc::Volume);

    // MIDI: Organ A channel 0 (Off) -> 5, stored zero-based.
    let mut p = Panel::new();
    p.open(7);
    p.page_up(2);
    p.dial(5);
    assert_eq!(p.engine.layer_midi_channels[0], Some(4));

    // Extern: Program Change of Extern A.
    let mut p = Panel::new();
    p.open(8);
    p.page_up(4);
    p.dial(7);
    assert_eq!(p.engine.extern_out.layers[0].program, 7);
    assert!(p.text().rows[0].contains("PC A: 7"), "{:?}", p.text().rows);
}

#[test]
fn every_bound_item_is_applied_by_the_engine() {
    for (_, item) in nord_panel::menus::all_items() {
        if matches!(item.binding, Binding::Bound(_)) {
            let mut e = Stage4Engine::new(48_000.0);
            let edit = crate::audio::SettingsEdit {
                id: item.id,
                value: item.default,
            };
            assert!(edit.apply(&mut e), "{} has no engine setter", item.id);
        }
    }
}

#[test]
fn exit_leaves_the_menu_and_keeps_the_values() {
    let mut p = Panel::new();
    p.open(1);
    p.page_up(1);
    p.dial(2);
    p.exit();
    assert_eq!(p.ui.screen, Screen::Home);
    p.open(1);
    p.page_up(1);
    assert!(p.text().rows[0].contains("+2"), "the value survives Exit");
}

#[test]
fn memory_protect_blocks_a_bank_store_but_not_live_and_can_be_switched_off() {
    let mut p = Panel::new();
    p.ui.title = "Rusty".into();
    // Factory default: protected.
    p.click("program.store", false);
    p.click("program.store", false); // confirm to the offered bank location
    assert!(p.ui.library.get(p.ui.loc).is_none(), "nothing stored");
    assert_eq!(p.ui.message.as_ref().unwrap().0, "Memory Protected");
    assert_eq!(p.ui.screen, Screen::Home);
    assert!(!p.ui.store.is_active());

    // Live programs are exempt.
    p.click("program.store", false);
    p.click("program.live_mode", false); // choose Live
    p.click("program.store", false);
    assert!(p.ui.library.live(0).is_some(), "Live store went through");

    // System menu page 1: switch Memory Protect off, then the bank store works.
    p.open(1);
    assert_eq!(p.dial(-1), 0, "UiOnly: no engine command");
    assert!(!p.ui.menu.memory_protect());
    p.exit();
    p.click("program.store", false);
    p.click("program.store", false);
    assert_eq!(
        p.ui.library.get(p.ui.loc).map(|u| u.name.as_str()),
        Some("Rusty")
    );
}
