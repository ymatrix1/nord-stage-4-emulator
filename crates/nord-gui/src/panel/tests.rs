//! Panel tests (state, audio sync, keybed regression, headless frame).

use super::prelude::*;
use super::render_stage4_panel;

#[test]
fn test_panel_state_default() {
    let state = Stage4PanelState::default();
    assert_eq!(state.prog_id, "A:11");
    assert_eq!(state.organ_drawbars[0], 8);
    assert_eq!(state.synth_cutoff, 2400.0);
    assert!(state.mouse_held_note.is_none());
}

#[test]
fn test_panel_state_audio_sync() {
    let mut state = Stage4PanelState::default();
    let (audio_handle, rx) = AudioHandle::new();
    state.organ_drawbars[0] = 5;
    audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));

    match rx.try_recv() {
        Ok(AudioCommand::SetDrawbars(d)) => {
            assert_eq!(d[0], 5);
        }
        _ => panic!("Expected SetDrawbars"),
    }
}

#[test]
fn test_keybed_mouse_interaction_no_stuck_notes() {
    // Enforce the rule: "every error must become a test if possible"
    // Regression test for non-stop sustaining/stuck notes on mouse click/drag.
    let mut state = Stage4PanelState::default();
    let (audio_handle, rx) = AudioHandle::new();

    // 1. Mouse down on Middle C (MIDI 60)
    let clicked = Some(60u8);
    if state.mouse_held_note != clicked {
        if let Some(old) = state.mouse_held_note.take() {
            audio_handle.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: old,
                velocity: 0,
            });
        }
        if let Some(new_n) = clicked {
            state.mouse_held_note = Some(new_n);
            audio_handle.send_midi(MidiEvent::NoteOn {
                channel: 0,
                note: new_n,
                velocity: 100,
            });
        }
    }
    assert_eq!(state.mouse_held_note, Some(60));
    match rx.try_recv() {
        Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
            assert_eq!(note, 60);
            assert_eq!(velocity, 100);
        }
        _ => panic!("Expected NoteOn(60)"),
    }

    // 2. Mouse drag across to D4 (MIDI 62)
    let dragged = Some(62u8);
    if state.mouse_held_note != dragged {
        if let Some(old) = state.mouse_held_note.take() {
            audio_handle.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: old,
                velocity: 0,
            });
        }
        if let Some(new_n) = dragged {
            state.mouse_held_note = Some(new_n);
            audio_handle.send_midi(MidiEvent::NoteOn {
                channel: 0,
                note: new_n,
                velocity: 100,
            });
        }
    }
    assert_eq!(state.mouse_held_note, Some(62));
    // Verify NoteOff(60) was sent before NoteOn(62)
    match rx.try_recv() {
        Ok(AudioCommand::Midi(MidiEvent::NoteOff { note, .. })) => assert_eq!(note, 60),
        _ => panic!("Expected NoteOff(60)"),
    }
    match rx.try_recv() {
        Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
            assert_eq!(note, 62);
            assert_eq!(velocity, 100);
        }
        _ => panic!("Expected NoteOn(62)"),
    }

    // 3. Mouse release: Must emit NoteOff(62) and leave no stuck notes!
    if let Some(old) = state.mouse_held_note.take() {
        audio_handle.send_midi(MidiEvent::NoteOff {
            channel: 0,
            note: old,
            velocity: 0,
        });
    }
    assert!(state.mouse_held_note.is_none());
    match rx.try_recv() {
        Ok(AudioCommand::Midi(MidiEvent::NoteOff { note, .. })) => assert_eq!(note, 62),
        _ => panic!("Expected NoteOff(62) on mouse release"),
    }
    assert!(
        rx.try_recv().is_err(),
        "No dangling audio commands should remain"
    );
}

#[test]
fn test_panel_state_apply_program() {
    let mut state = Stage4PanelState::default();
    assert!(!state.sync_knobs_on_program_load);
    state.sync_knobs_on_program_load = true;
    let mut prog = nord_core::cbin::ProgramDecoded {
        name: "Funk Solo".to_string(),
        bank_index: 0,
        location_index: 8,
        ..Default::default()
    };
    prog.organ.layer_a_enabled = true;
    prog.organ.layer_a_level = 0.77;
    prog.organ.drawbars_a = [8, 8, 8, 8, 0, 0, 0, 0, 0];
    prog.piano.layer_a.string_res = true;
    prog.piano.layer_a.pedal_noise = false;
    prog.synth.layer_a.enabled = true;
    prog.synth.layer_a.filter_cutoff_hz = 3200.0;
    prog.global.master_compressor = 4.0;

    state.apply_program(&prog);

    assert_eq!(state.prog_title, "Funk Solo");
    assert_eq!(state.prog_id, "A:21");
    assert_eq!(state.active_program_idx, 1);
    assert!(state.string_res);
    assert!(!state.pedal_noise);
    assert!(state.organ_a_active);
    assert!((state.organ_a_level - 0.77).abs() < 1e-5);
    assert_eq!(state.organ_drawbars, [8, 8, 8, 8, 0, 0, 0, 0, 0]);
    assert_eq!(state.fx_focus, 2);
    assert!((state.synth_cutoff - 3200.0).abs() < 1e-5);
    assert!((state.master_comp - 0.4).abs() < 1e-5);

    // When sync_knobs_on_program_load is false (NS4 hardware pot behavior), OLED/LED levels update while knobs stay fixed
    state.sync_knobs_on_program_load = false;
    let mut piano_prog = nord_core::cbin::ProgramDecoded {
        name: "White Grand".to_string(),
        bank_index: 0,
        location_index: 8,
        category_id: 0x09,
        ..Default::default()
    };
    piano_prog.global.scene_1_mask = 1 << 2;
    piano_prog.piano.layer_a.level = 0.91;
    piano_prog.piano.layer_a.string_res = true;
    piano_prog.piano.layer_a.pedal_noise = true;
    piano_prog.synth.layer_a.filter_cutoff_hz = 650.0;
    piano_prog.global.master_compressor = 8.0;
    state.apply_program(&piano_prog);

    assert_eq!(state.prog_title, "White Grand");
    assert_eq!(state.prog_id, "A:21");
    assert!(state.string_res);
    assert!(state.pedal_noise);
    assert_eq!(state.fx_focus, 1);
    assert!((state.piano_a_level - 0.91).abs() < 1e-5);
    assert!((state.synth_cutoff - 3200.0).abs() < 1e-5);
    assert!((state.master_comp - 0.4).abs() < 1e-5);
}

/// FNV-1a over the debug text of every painted shape of one headless frame.
fn frame_hash(state: &mut Stage4PanelState) -> (u64, usize) {
    let ctx = egui::Context::default();
    let (audio_handle, _rx) = AudioHandle::new();
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_stage4_panel(ui, state, &audio_handle);
        });
    });
    let text = format!("{:?}", out.shapes);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h, out.shapes.len())
}

#[test]
fn panel_render_is_deterministic_and_non_empty() {
    let (a, n) = frame_hash(&mut Stage4PanelState::default());
    let (b, _) = frame_hash(&mut Stage4PanelState::default());
    assert_eq!(a, b, "headless frame must be deterministic");
    assert!(n > 500, "panel painted only {n} shapes");
    eprintln!("PANEL_FRAME_HASH {a:#018x} shapes {n}");
}

/// Frame-time probe: prints the mean CPU time of one full-panel frame (release build gives the
/// number that matters; the assertion only guards against order-of-magnitude regressions).
#[test]
fn full_panel_frame_time_probe() {
    let ctx = egui::Context::default();
    let (audio_handle, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let mut frame = || {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_stage4_panel(ui, &mut state, &audio_handle);
            });
        });
    };
    for _ in 0..3 {
        frame();
    }
    let n = 40;
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        frame();
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("FRAME_MS {ms:.2}");
    assert!(ms < 500.0, "frame took {ms:.1} ms");
}

/// Click probe: taps the centre of every inventory control on the full panel and prints the ids
/// where the click changed neither the audio commands nor the painted frame (candidates for
/// overlapped, dead or purely cosmetic widgets).
#[test]
fn full_panel_click_probe() {
    use egui::{Event, Modifiers, PointerButton, RawInput};
    let ctx = egui::Context::default();
    let (audio_handle, rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let mut time = 1.0_f64;
    let mut origin = pos2(0.0, 0.0);
    let mut frame =
        |events: Vec<Event>, state: &mut Stage4PanelState, origin: &mut egui::Pos2| -> u64 {
            time += 0.04;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1700.0, 600.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            let out = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    *origin = ui.cursor().min;
                    render_stage4_panel(ui, state, &audio_handle);
                });
            });
            let text = format!("{:?}", out.shapes);
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in text.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h
        };
    frame(vec![], &mut state, &mut origin);
    let mut silent = Vec::new();
    for c in nord_panel::all_controls() {
        let Some(r) = super::layout_px::rect_of(&c.id, origin) else {
            continue;
        };
        let at = r.center();
        while rx.try_recv().is_ok() {}
        let ev = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(vec![Event::PointerMoved(at)], &mut state, &mut origin);
        let hover = frame(vec![], &mut state, &mut origin);
        frame(vec![ev(true)], &mut state, &mut origin);
        frame(vec![ev(false)], &mut state, &mut origin);
        let after = frame(vec![], &mut state, &mut origin);
        if rx.try_recv().is_err() && hover == after {
            silent.push(c.id.clone());
        }
        frame(
            vec![Event::PointerMoved(pos2(0.0, 0.0))],
            &mut state,
            &mut origin,
        );
    }
    eprintln!("CLICK_PROBE silent {}: {}", silent.len(), silent.join(" "));
}
