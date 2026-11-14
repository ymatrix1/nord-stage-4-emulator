//! More unit tests of `actions`.

use super::*;
use crate::audio::apply_audio_command;
use crate::panel::bay_program::model::MenuId;
use nord_core::engine::Stage4Engine;

/// Plays the DSP thread: applies `cmds` and feeds snapshot replies back to the UI.
fn run(u: &mut ProgramUi, e: &mut Stage4Engine, cmds: Vec<AudioCommand>) {
    let mut vol = 1.0;
    for c in cmds {
        apply_audio_command(c, e, &mut vol);
    }
    while let Ok((t, s)) = u.reply_rx.try_recv() {
        u.accept_snapshot(t, s);
    }
}

#[test]
fn store_as_names_with_abc_cat_ins_del_then_stores() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    u.title = "Rusty".into();
    u.store_as();
    assert_eq!(u.screen, Screen::StoreAs);
    u.soft_down(1); // ABC held
    u.dial(1, false); // R -> S
    u.soft_up(1);
    u.soft_down(4); // Ins at cursor 1
    u.soft_down(3); // Del it again
    u.soft_down(2); // Cat held
    u.dial(2, false);
    u.soft_up(2);
    assert_eq!(u.store.editor.name(), "Susty");
    assert_eq!(u.store.editor.category, 2);
    u.store_press(); // naming done -> choose location
    assert_eq!(u.screen, Screen::StoreTo);
    let out = u.store_press();
    run(&mut u, &mut e, out);
    let p = u.library.get(u.loc).unwrap();
    assert_eq!((p.name.as_str(), p.category), ("Susty", 2));
    assert_eq!(u.title, "Susty");
}

#[test]
fn exit_cancels_a_store_and_live_target_stores_into_the_live_slot() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    u.store_press();
    u.exit();
    assert!(!u.store.is_active());
    u.store_press();
    u.activate("program.live_mode", Trigger::Primary);
    u.program_button(4);
    let out = u.store_press();
    run(&mut u, &mut e, out);
    assert!(u.library.live(3).is_some());
    assert!(u.library.is_empty());
}

#[test]
fn buttons_open_the_manual_screens_and_a_second_press_closes() {
    let mut u = ProgramUi::default();
    for (id, screen) in [
        ("program.undo", Screen::Undo),
        ("program.layer_init", Screen::LayerInit),
        ("program.mon_copy", Screen::Copy),
        ("program.paste", Screen::Paste),
    ] {
        u.activate(id, Trigger::Primary);
        assert_eq!(u.screen, screen, "{id}");
        u.activate(id, Trigger::Primary);
        assert_eq!(u.screen, Screen::Home);
    }
    u.activate("program.menu.midi", Trigger::Shifted);
    assert_eq!(u.screen, Screen::Menu(MenuId::Midi));
    u.activate("program.prog_view", Trigger::Primary);
    assert_eq!(u.screen, Screen::Home);
    assert_ne!(u.view, super::super::model::ProgView::Name);
}

#[test]
fn split_clock_transpose_panic_and_scene_send_engine_edits() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    let go = |u: &mut ProgramUi, e: &mut Stage4Engine, id: &str, t: Trigger| {
        let out = u.activate(id, t);
        run(u, e, out);
    };
    go(&mut u, &mut e, "split.on_set", Trigger::Primary);
    go(&mut u, &mut e, "transp.on_set", Trigger::Primary);
    go(&mut u, &mut e, "program.layer_scene_ii", Trigger::Primary);
    go(&mut u, &mut e, "transp.panic", Trigger::Shifted);
    for t in [0, 500, 1000, 1500] {
        u.now_ms = t;
        go(&mut u, &mut e, "mst_clk.tap_set", Trigger::Primary);
    }
    assert!(e.split_router.splits[1].enabled);
    assert!(e.scene.scene_2_active);
    assert!((e.master_clock.bpm - 120.0).abs() < 0.01);
    assert_eq!(e.transpose(), 0, "on with 0 semitones");
    // Hold opens the settings screens; the dial then edits their value.
    u.activate("transp.on_set", Trigger::Hold);
    assert_eq!(u.screen, Screen::Transpose);
    let out = u.dial(3, false);
    run(&mut u, &mut e, out);
    assert_eq!(e.transpose(), 3);
    u.exit();
    u.activate("mst_clk.tap_set", Trigger::Hold);
    assert_eq!(u.screen, Screen::Clock);
    let out = u.dial(10, false);
    run(&mut u, &mut e, out);
    assert!((e.master_clock.bpm - 130.0).abs() < 0.01);
    u.exit();
    u.activate("split.on_set", Trigger::Hold);
    assert_eq!(u.screen, Screen::Split);
}

#[test]
fn morph_hold_latch_depth_and_clear_reach_the_engine() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    u.morph_down(MorphSource::ModWheel);
    let out = u.dial(5, false);
    run(&mut u, &mut e, out);
    assert!((e.layer_levels[0].delta_wheel - 0.25).abs() < 1e-6);
    u.morph_up(MorphSource::ModWheel);
    assert!(u.dial(1, false).is_empty());
    u.morph_latch(MorphSource::Aftertouch);
    u.page(1, false);
    let out = u.dial(-4, false);
    run(&mut u, &mut e, out);
    assert!((e.layer_levels[1].delta_aftertouch + 0.2).abs() < 1e-6);
    u.exit();
    let out = u.morph_clear(MorphSource::ModWheel);
    run(&mut u, &mut e, out);
    assert_eq!(e.layer_levels[0].delta_wheel, 0.0);
    u.morph_tap(MorphSource::ControlPedal);
    assert_eq!(u.screen, Screen::Morph(MorphSource::ControlPedal));
    u.morph_tap(MorphSource::ControlPedal);
    assert_eq!(u.screen, Screen::Home);
}

#[test]
fn shift_tap_exits_only_when_it_was_not_used_as_a_modifier() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Clock;
    u.shift_used = true;
    u.activate("program.shift.left", Trigger::Primary);
    assert_eq!(u.screen, Screen::Clock);
    u.shift_used = false;
    u.activate("program.shift.right", Trigger::Primary);
    assert_eq!(u.screen, Screen::Home);
}

#[test]
fn section_edit_latches_on_double_tap_and_tap_leaves() {
    let mut u = ProgramUi::default();
    u.activate("program.section_edit", Trigger::Latch);
    assert!(u.section_edit_active());
    u.activate("program.section_edit", Trigger::Primary);
    assert!(!u.section_edit_active());
}

#[test]
fn solo_pedal_tap_set_key_and_presets_report_instead_of_dropping() {
    let mut u = ProgramUi::default();
    u.activate("program.solo", Trigger::Primary);
    assert!(u.solo && u.message.is_some());
    u.message = None;
    u.activate("mst_clk.pedal_tap", Trigger::Shifted);
    assert!(u.message.is_some());
    u.activate("split.set_key", Trigger::Shifted);
    assert!(u.split.set_key_armed);
    u.activate("preset_library.single_synth", Trigger::Shifted);
    assert_eq!(u.focus, LayerId::SynthA);
    assert_eq!(
        u.screen,
        Screen::Preset {
            kind: PresetKind::Synth,
            single: true
        }
    );
    u.exit();
    u.store_press();
    u.message = None;
    u.activate("preset_library.organ", Trigger::Primary);
    assert!(
        u.message.is_some(),
        "preset store is reported as unsupported"
    );
}
