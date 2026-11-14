//! Proof for the `ui:program.*` bindings of the control inventory (`ns4-lu50`): each control is
//! pressed through `interact::press` (the function the painted button feeds), the commands are
//! applied to a real engine and the result is asserted. A final test requires the proven ids to be
//! exactly the Program-section controls bound to a `ui:` path, so a new binding needs a proof.

use super::catalog::LoadRequest;
use super::controls::{Fed, BUTTONS, MORPH_BUTTONS, PROGRAM_BUTTONS};
use super::interact::press;
use super::model::{PresetKind, Screen};
use crate::audio::{apply_audio_command, AudioCommand};
use crate::panel::ui_state::ProgramUi;
use nord_core::engine::Stage4Engine;
use nord_core::morph::LayerId;
use nord_panel::behaviour::Gesture;
use nord_panel::{all_controls, Binding, Section};

/// Press the control `id` (a shifted id presses its base button with Shift down).
fn tap(u: &mut ProgramUi, id: &str) -> Vec<AudioCommand> {
    let all = PROGRAM_BUTTONS
        .iter()
        .chain(BUTTONS.iter())
        .chain(MORPH_BUTTONS.iter());
    let (spec, shifted) = all
        .clone()
        .find(|s| s.id == id)
        .map(|s| (*s, false))
        .or_else(|| {
            all.clone()
                .find(|s| s.shift.is_some_and(|f| f.id == id))
                .map(|s| (*s, true))
        })
        .unwrap_or_else(|| panic!("no button for {id}"));
    let fed = Fed {
        gesture: Some(Gesture::Tap),
        went_down: false,
        went_up: true,
    };
    let mut cmds = Vec::new();
    press(u, &spec, &fed, shifted, &mut cmds);
    cmds
}

/// Apply `cmds` as the DSP thread would and feed snapshot replies back.
fn run(u: &mut ProgramUi, e: &mut Stage4Engine, cmds: Vec<AudioCommand>) {
    let mut vol = 1.0;
    for c in cmds {
        apply_audio_command(c, e, &mut vol);
    }
    while let Ok((t, s)) = u.reply_rx.try_recv() {
        u.accept_snapshot(t, s);
    }
}

fn ui_with_catalog() -> ProgramUi {
    let mut u = ProgramUi::default();
    let names: Vec<String> = (0..128)
        .map(|i| format!("Program/Bank A/P{i:03}.ns4p"))
        .collect();
    u.sync_catalog(
        &names,
        &["Piano/a.npno".into(), "Piano/b.npno".into()],
        &["Samp/a.nsmp4".into()],
    );
    u
}

fn dial_and_slots() -> Vec<&'static str> {
    let mut u = ui_with_catalog();
    u.dial(1, false);
    assert_eq!(u.loc.label(), "A:12");
    assert_eq!(
        u.take_requests(),
        vec![LoadRequest::Program("Program/Bank A/P001.ns4p".into())]
    );
    for n in 1..=8u8 {
        let before = u.take_requests();
        assert!(before.is_empty());
        tap(&mut u, &format!("program.button.{n}"));
        assert_eq!(u.loc.slot, n - 1, "button {n}");
        assert_eq!(
            u.take_requests().len(),
            1,
            "button {n} requests its program"
        );
    }
    vec![
        "program.dial",
        "program.button.1",
        "program.button.2",
        "program.button.3",
        "program.button.4",
        "program.button.5",
        "program.button.6",
        "program.button.7",
        "program.button.8",
    ]
}

fn num_pad() -> Vec<&'static str> {
    let mut u = ui_with_catalog();
    tap(&mut u, "program.num_pad");
    assert!(u.num_pad && u.screen == Screen::NumPad);
    tap(&mut u, "program.button.2"); // page 2
    tap(&mut u, "program.button.5"); // program 5
    assert_eq!(u.loc.label(), "A:25");
    assert_eq!(u.take_requests().len(), 1);
    vec!["program.num_pad"]
}

fn store_flow() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    e.layer_levels[2].base = 0.42;
    let mut u = ProgramUi::default();
    u.menu.set_memory_protect(false);
    // STORE: choose a slot, STORE again confirms.
    tap(&mut u, "program.store");
    assert_eq!(u.screen, Screen::StoreTo);
    tap(&mut u, "program.button.7");
    let target = u.store.bank_target().expect("bank target");
    let out = tap(&mut u, "program.store");
    run(&mut u, &mut e, out);
    assert!(u.library.get(target).is_some(), "STORE wrote the library");
    // STORE AS: name first, then choose and confirm.
    u.title = "Proof".into();
    tap(&mut u, "program.store_as");
    assert_eq!(u.screen, Screen::StoreAs);
    tap(&mut u, "program.store"); // naming done
    tap(&mut u, "program.button.3");
    let target = u.store.bank_target().expect("bank target");
    let out = tap(&mut u, "program.store");
    run(&mut u, &mut e, out);
    assert_eq!(
        u.library.get(target).map(|p| p.name.as_str()),
        Some("Proof")
    );
    // LIVE MODE during STORE picks the Live slots.
    tap(&mut u, "program.store");
    tap(&mut u, "program.live_mode");
    tap(&mut u, "program.button.4");
    let out = tap(&mut u, "program.store");
    run(&mut u, &mut e, out);
    assert!(u.library.live(3).is_some(), "stored into Live slot 4");
    vec!["program.store", "program.store_as", "program.live_mode"]
}

fn init_undo_copy_paste() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    // LAYER INIT (soft button 1 = All) resets the levels, UNDO (soft 1) brings them back.
    e.layer_levels[2].base = 0.33;
    tap(&mut u, "program.layer_init");
    assert_eq!(u.screen, Screen::LayerInit);
    let out = u.soft_down(1);
    run(&mut u, &mut e, out);
    assert_ne!(e.layer_levels[2].base, 0.33);
    tap(&mut u, "program.undo");
    assert_eq!(u.screen, Screen::Undo);
    let out = u.soft_down(1);
    run(&mut u, &mut e, out);
    assert_eq!(e.layer_levels[2].base, 0.33);
    // MON/COPY the focused layer, PASTE it into another one.
    e.layer_levels[0].base = 0.1;
    e.layer_levels[1].base = 0.9;
    u.focus = LayerId::OrganB;
    tap(&mut u, "program.mon_copy");
    assert_eq!(u.screen, Screen::Copy);
    let out = u.soft_down(1);
    run(&mut u, &mut e, out);
    u.focus = LayerId::OrganA;
    tap(&mut u, "program.paste");
    assert_eq!(u.screen, Screen::Paste);
    let out = u.soft_down(1);
    run(&mut u, &mut e, out);
    assert_eq!(e.layer_levels[0].base, 0.9);
    vec![
        "program.layer_init",
        "program.undo",
        "program.mon_copy",
        "program.paste",
    ]
}

fn morph() -> Vec<&'static str> {
    use nord_core::morph::MorphSource::{Aftertouch, ControlPedal, ModWheel};
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    for (id, src) in [
        ("morph.wheel", ModWheel),
        ("morph.at", Aftertouch),
        ("morph.ctrlped", ControlPedal),
    ] {
        let spec = MORPH_BUTTONS.iter().find(|s| s.id == id).unwrap();
        let mut cmds = Vec::new();
        let down = Fed {
            gesture: None,
            went_down: true,
            went_up: false,
        };
        press(&mut u, spec, &down, false, &mut cmds);
        assert_eq!(u.morph.assigning(), Some(src), "{id} held");
        let out = u.dial(5, false); // depth +0.25 of the first target (Organ A level)
        run(&mut u, &mut e, out);
        let up = Fed {
            gesture: None,
            went_down: false,
            went_up: true,
        };
        press(&mut u, spec, &up, false, &mut cmds);
    }
    let d = &e.layer_levels[0];
    assert!((d.delta_wheel - 0.25).abs() < 1e-6);
    assert!((d.delta_aftertouch - 0.25).abs() < 1e-6);
    assert!((d.delta_pedal - 0.25).abs() < 1e-6);
    // CLEAR (Shift + a Morph Assign button) clears that source.
    let out = tap(&mut u, "morph.clear");
    run(&mut u, &mut e, out);
    assert_eq!(e.layer_levels[0].delta_wheel, 0.0);
    vec!["morph.wheel", "morph.at", "morph.ctrlped", "morph.clear"]
}

fn split_key_pedal_panic() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    let out = tap(&mut u, "split.on_set");
    run(&mut u, &mut e, out);
    tap(&mut u, "split.set_key");
    assert!(u.split.set_key_armed);
    let out = u.key_pressed(55);
    run(&mut u, &mut e, out);
    assert_eq!(e.split_router.splits[u.split.point.min(2)].midi_note, 55);
    // PEDAL TAP
    let out = tap(&mut u, "mst_clk.pedal_tap");
    run(&mut u, &mut e, out);
    assert_eq!(
        e.pedals.triple.panel_assigned,
        nord_core::engine::PanelFunc::MstClkTap
    );
    // PANIC resets the Morph controllers (and releases every note).
    e.morph_controllers.control_pedal = 0.8;
    let out = tap(&mut u, "transp.panic");
    run(&mut u, &mut e, out);
    assert_eq!(e.morph_controllers.control_pedal, 0.0);
    vec!["split.set_key", "mst_clk.pedal_tap", "transp.panic"]
}

fn preset_library() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut u = ui_with_catalog();
    // ORGAN: the dial picks a model and sends it to the engine.
    tap(&mut u, "preset_library.organ");
    assert_eq!(
        u.screen,
        Screen::Preset {
            kind: PresetKind::Organ,
            single: false
        }
    );
    let out = u.dial(1, false);
    assert!(matches!(out.as_slice(), [AudioCommand::SetOrganModel(_)]));
    run(&mut u, &mut e, out);
    // PIANO / SYNTH: the dial requests the backup entry for the focused layer.
    u.exit();
    tap(&mut u, "preset_library.piano");
    u.dial(1, false);
    assert!(u
        .take_requests()
        .iter()
        .any(|r| matches!(r, LoadRequest::Piano { .. })));
    u.exit();
    tap(&mut u, "preset_library.synth");
    u.dial(1, false);
    assert!(u
        .take_requests()
        .iter()
        .any(|r| matches!(r, LoadRequest::Sample { .. })));
    // Shift + PIANO / SYNTH: the single-layer variants move the focus to that section.
    u.exit();
    tap(&mut u, "preset_library.single_piano");
    assert_eq!(u.focus, LayerId::PianoA);
    u.exit();
    tap(&mut u, "preset_library.single_synth");
    assert_eq!(u.focus, LayerId::SynthA);
    vec![
        "preset_library.organ",
        "preset_library.piano",
        "preset_library.synth",
        "preset_library.single_piano",
        "preset_library.single_synth",
    ]
}

#[test]
fn every_program_ui_binding_is_performed_by_the_gui() {
    let mut proven: Vec<&str> = Vec::new();
    for ids in [
        dial_and_slots(),
        num_pad(),
        store_flow(),
        init_undo_copy_paste(),
        morph(),
        split_key_pedal_panic(),
        preset_library(),
    ] {
        proven.extend(ids);
    }
    proven.sort_unstable();
    let mut bound: Vec<String> = all_controls()
        .into_iter()
        .filter(|c| {
            c.section == Section::Program
                && matches!(c.binding, Binding::Bound(b) if b.starts_with("ui:"))
        })
        .map(|c| c.id)
        .collect();
    bound.sort_unstable();
    assert_eq!(
        proven.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        bound,
        "the proven ids must be exactly the Program controls bound to ui:program.*"
    );
}

#[test]
fn page_bank_view_and_name_are_pure_display_state() {
    // The four UiOnly Program controls: they change the browse cursor / display, never the loaded
    // program (no load request, no audio command).
    let mut u = ui_with_catalog();
    let loc = u.loc;
    assert!(tap(&mut u, "program.page_up").is_empty());
    assert_eq!(
        u.browse.page,
        loc.page + 1,
        "PAGE + moved the browse cursor"
    );
    assert!(tap(&mut u, "program.bank_up").is_empty());
    assert_eq!(
        u.browse.bank,
        loc.bank + 1,
        "BANK + moved the browse cursor"
    );
    assert!(tap(&mut u, "program.page_down").is_empty());
    assert!(tap(&mut u, "program.bank_down").is_empty());
    assert_eq!(u.browse.bank, loc.bank);
    let view = u.view;
    assert!(tap(&mut u, "program.prog_view").is_empty());
    assert_ne!(u.view, view, "PROG VIEW cycled the display view");
    assert!(tap(&mut u, "program.preset_name").is_empty());
    assert!(u.preset_name, "PRESET NAME latched the name display");
    assert_eq!(u.loc, loc);
    assert!(u.take_requests().is_empty(), "nothing was loaded");
}
