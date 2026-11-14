//! STORE of a section preset: location choice, confirm, Memory Protect, both button orders.

use super::*;
use crate::audio::apply_audio_command;
use nord_core::cbin::{PresetSection, SectionPreset};
use nord_core::engine::Stage4Engine;
use nord_panel::behaviour::Trigger;

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

fn ui() -> ProgramUi {
    let mut u = ProgramUi::default();
    u.menu.set_memory_protect(false);
    u.library.presets.set_factory(
        PresetSection::Synth,
        &[
            (0, "Synth Preset/Bank 1/Alpha.ns4y".into()),
            (1, "Synth Preset/Bank 1/Beta.ns4y".into()),
        ],
    );
    u.title = "My Program".into();
    u
}

#[test]
fn store_then_a_preset_button_picks_the_section_and_confirm_stores_it() {
    let mut e = Stage4Engine::new(48_000.0);
    e.layer_levels[4].base = 0.35;
    let mut u = ui();
    u.library
        .presets
        .mark_loaded(PresetSection::Synth, "Synth Preset/Bank 1/Beta.ns4y");
    u.store_press();
    assert!(u.preset_store_text().is_none(), "a program store so far");
    assert!(u
        .activate("preset_library.synth", Trigger::Primary)
        .is_empty());
    assert_eq!(u.screen, Screen::StoreTo);
    // starts on the loaded preset's cell and name
    let t = u.preset_store_text().unwrap();
    assert_eq!(t.big, "STORE PRESET TO");
    assert_eq!(t.sub, "Synth 1:02");
    assert_eq!(t.rows, ["Beta", "As: Beta"]);
    u.dial(2, false); // 1:04
    u.page(1, false); // next page: 1:12
    u.program_button(3); // position 3 of that page: 1:11
    assert_eq!(u.store.preset, Some((PresetKind::Synth, 10)));
    u.page(1, true); // next bank: 2:11
    assert_eq!(u.store.preset, Some((PresetKind::Synth, 74)));
    let out = u.store_press();
    run(&mut u, &mut e, out);
    assert_eq!(u.screen, Screen::Home);
    assert!(!u.store.is_active());
    let lib = &u.library.presets;
    let entry = lib.entry_at(PresetSection::Synth, 74).unwrap().to_string();
    let stored = lib.user(&entry).expect("stored preset");
    assert_eq!(stored.name, "Beta");
    assert_eq!(
        stored.base.as_deref(),
        Some("Synth Preset/Bank 1/Beta.ns4y")
    );
    let SectionPreset::Synth(syn) = stored
        .state
        .capture_section_preset(PresetSection::Synth, None)
    else {
        panic!("synth expected")
    };
    assert!((syn.layer_a.level - 0.35).abs() < 0.01);
    assert_eq!(
        lib.entry_at(PresetSection::Synth, 1),
        Some("Synth Preset/Bank 1/Beta.ns4y")
    );
    assert!(u.library.is_empty(), "no program was stored");
}

#[test]
fn a_preset_button_first_then_store_also_stores_and_store_as_names_it() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ui();
    u.activate("preset_library.organ", Trigger::Primary);
    assert!(matches!(u.screen, Screen::Preset { .. }));
    u.store_press();
    assert_eq!(u.screen, Screen::StoreTo);
    assert_eq!(
        u.store.preset,
        Some((PresetKind::Organ, 0)),
        "first free cell"
    );
    u.exit();
    assert!(u.store.preset.is_none() && !u.store.is_active());
    // STORE AS: name first, then the preset button
    u.store_as();
    u.soft_down(1);
    u.dial(1, false);
    u.soft_up(1);
    u.store_press();
    u.activate("preset_library.organ", Trigger::Primary);
    let out = u.store_press();
    run(&mut u, &mut e, out);
    let lib = &u.library.presets;
    let entry = lib.entry_at(PresetSection::Organ, 0).unwrap();
    assert_eq!(lib.user(entry).unwrap().name, "Ny Program");
}

#[test]
fn memory_protect_blocks_a_preset_store_and_the_location_is_clamped() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ui();
    u.menu.set_memory_protect(true);
    u.store_press();
    u.activate("preset_library.piano", Trigger::Primary);
    u.dial(-5, false);
    assert_eq!(
        u.store.preset,
        Some((PresetKind::Piano, 0)),
        "no negative location"
    );
    u.dial(1000, false);
    assert_eq!(u.store.preset, Some((PresetKind::Piano, 127)), "no wrap");
    let out = u.store_press();
    run(&mut u, &mut e, out);
    assert_eq!(u.library.presets.count(PresetSection::Piano), 0);
    assert_eq!(u.screen, Screen::Home);
    assert!(!u.store.is_active());
}
