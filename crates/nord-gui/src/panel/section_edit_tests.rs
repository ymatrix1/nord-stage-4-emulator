//! SECTION EDIT (manual p.43) through the real bays: a headless full-panel egui session latches
//! the button, moves one control and applies the commands it sent to a real `Stage4Engine`. Each
//! bay has a positive test (every layer of the section changed) and a negative one (latch off:
//! the focused layer only).

use super::prelude::*;
use super::sim::Sim;
use nord_core::organ::ScannerMode;
use nord_panel::behaviour::Trigger;

/// Fresh engine with everything the sim sent applied to it.
fn engine_after(sim: &Sim) -> Stage4Engine {
    let mut e = Stage4Engine::new(48_000.0);
    let mut master = 1.0;
    for c in sim.cmds() {
        apply_audio_command(c, &mut e, &mut master);
    }
    e
}

/// Full panel at zoom 1 on a screen big enough to reach every bay.
fn full() -> Sim {
    let mut sim = Sim::full_panel();
    sim.screen = vec2(3200.0, 900.0);
    sim.frame(vec![], egui::Modifiers::NONE);
    sim
}

fn latched() -> Sim {
    let mut sim = full();
    sim.state
        .program_ui
        .activate("program.section_edit", Trigger::Latch);
    sim.frame(vec![], egui::Modifiers::NONE);
    sim
}

fn unlatched() -> Sim {
    let mut sim = full();
    sim.frame(vec![], egui::Modifiers::NONE);
    sim
}

/// Drag the knob `id` upwards by `px` screen pixels.
fn turn_up(sim: &mut Sim, id: &str, px: f32) {
    let a = sim.center(id);
    sim.drag(a, a + vec2(0.0, -px));
}

#[test]
fn a_double_tap_on_the_button_latches_it_in_the_real_panel() {
    let mut sim = full();
    sim.click("program.section_edit");
    sim.click("program.section_edit");
    assert!(sim.state.program_ui.section_edit_active());
    sim.click("program.section_edit");
    assert!(!sim.state.program_ui.section_edit_active(), "a tap leaves");
}

#[test]
fn organ_octave_and_vibrato_reach_both_layers() {
    let mut sim = latched();
    sim.click("organ.octave_up");
    sim.click("organ.vibrato_on.a");
    let e = engine_after(&sim);
    assert_eq!((e.organ_a.octave_shift, e.organ_b.octave_shift), (1, 1));
    assert_ne!(e.organ_a.scanner_mode, ScannerMode::Off);
    assert_eq!(e.organ_a.scanner_mode, e.organ_b.scanner_mode);
    assert_eq!(
        sim.state.organ_ui.octave_b, 1,
        "the panel shows layer B too"
    );
    assert!(sim.state.organ_ui.vib_on_b);
}

#[test]
fn organ_edit_stays_on_the_focused_layer_without_section_edit() {
    let mut sim = unlatched();
    sim.click("organ.octave_up");
    sim.click("organ.vibrato_on.a");
    let e = engine_after(&sim);
    assert_eq!((e.organ_a.octave_shift, e.organ_b.octave_shift), (1, 0));
    assert_eq!(e.organ_b.scanner_mode, ScannerMode::Off);
}

#[test]
fn organ_level_fader_moves_both_layers() {
    let mut sim = latched();
    sim.state.organ_b_active = true; // an off layer sends level 0
    let a = sim.center("organ.fader.a");
    sim.drag(a, a + vec2(0.0, 20.0));
    let moved = sim.state.organ_a_level;
    assert_ne!(moved, 0.85, "the fader moved");
    assert_eq!(sim.state.organ_b_level, moved);
    let e = engine_after(&sim);
    assert_eq!(e.layer_levels[0].base, e.layer_levels[1].base);
}

#[test]
fn piano_unison_and_level_reach_both_layers() {
    let mut sim = latched();
    sim.click("piano.unison");
    let a = sim.center("piano.fader.a");
    sim.drag(a, a + vec2(0.0, 30.0));
    let e = engine_after(&sim);
    assert_eq!((e.piano_a.unison, e.piano_b.unison), (1, 1));
    assert_ne!(sim.state.piano_a_level, 0.90, "the fader moved");
    assert_eq!(sim.state.piano_b_level, sim.state.piano_a_level);
    assert_eq!(e.layer_levels[2].base, e.layer_levels[3].base);
    assert_eq!(sim.state.piano_ui.unison, [1, 1]);
}

#[test]
fn piano_edit_stays_on_the_focused_layer_without_section_edit() {
    let mut sim = unlatched();
    sim.click("piano.unison");
    let e = engine_after(&sim);
    assert_eq!((e.piano_a.unison, e.piano_b.unison), (1, 0));
}

#[test]
fn synth_button_and_knob_reach_all_three_layers() {
    let mut sim = latched();
    sim.click("synth.octave_up");
    turn_up(&mut sim, "synth.filter.freq", 40.0);
    let e = engine_after(&sim);
    for (i, l) in [&e.synth_a, &e.synth_b, &e.synth_c].into_iter().enumerate() {
        assert_eq!(l.octave_offset, 1, "layer {i} octave");
    }
    assert_eq!(e.synth_a.cutoff_hz, e.synth_b.cutoff_hz);
    assert_eq!(e.synth_a.cutoff_hz, e.synth_c.cutoff_hz);
    assert!(sim.state.synth_ui.layers.iter().all(|l| l.octave == 1));
}

#[test]
fn synth_edit_stays_on_the_focused_layer_without_section_edit() {
    let mut sim = unlatched();
    sim.click("synth.octave_up");
    let e = engine_after(&sim);
    assert_eq!(e.synth_a.octave_offset, 1);
    assert_eq!((e.synth_b.octave_offset, e.synth_c.octave_offset), (0, 0));
}

#[test]
fn synth_fader_moves_all_three_levels() {
    let mut sim = latched();
    let a = sim.center("synth.fader.b");
    sim.drag(a, a + vec2(0.0, 30.0));
    let e = engine_after(&sim);
    assert_ne!(sim.state.synth_b_level, 0.0, "the fader moved");
    assert_eq!(sim.state.synth_a_level, sim.state.synth_b_level);
    assert_eq!(e.layer_levels[4].base, e.layer_levels[5].base);
    assert_eq!(e.layer_levels[5].base, e.layer_levels[6].base);
}

#[test]
fn effects_unit_reaches_every_chain_of_the_synth_section() {
    let mut sim = latched();
    sim.state.fx_focus = 2; // Synth A
    sim.frame(vec![], egui::Modifiers::NONE);
    turn_up(&mut sim, "reverb.dry_wet", 40.0);
    let e = engine_after(&sim);
    let mix = e.fx_chains[4].reverb_mix;
    assert!(mix > 0.2, "the knob moved ({mix})");
    assert_eq!(e.fx_chains[5].reverb_mix, mix);
    assert_eq!(e.fx_chains[6].reverb_mix, mix);
    assert_ne!(e.fx_chains[2].reverb_mix, mix, "Piano keeps its own chain");
    assert!(sim.state.fx_ui.chains[3..6]
        .iter()
        .all(|c| c.reverb_mix == sim.state.fx_ui.chains[3].reverb_mix));
}

#[test]
fn effects_unit_stays_on_one_chain_without_section_edit() {
    let mut sim = unlatched();
    sim.state.fx_focus = 2;
    sim.frame(vec![], egui::Modifiers::NONE);
    turn_up(&mut sim, "reverb.dry_wet", 40.0);
    let e = engine_after(&sim);
    assert!(e.fx_chains[4].reverb_mix > 0.2);
    assert_ne!(e.fx_chains[5].reverb_mix, e.fx_chains[4].reverb_mix);
    assert_ne!(e.fx_chains[6].reverb_mix, e.fx_chains[4].reverb_mix);
}
