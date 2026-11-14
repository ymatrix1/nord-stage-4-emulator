//! Bay 3: Piano section of the front panel (manual p.23-26).
//!
//! Every control is placed with `layout_px::rect_or_empty(id, panel_origin)`. This file draws the
//! shared strip (ON/SOLO, layers, faders, octave, KB zone, pedals); `bay_piano_select.rs` draws
//! the Piano Select and acoustics groups and the model overlays. Behaviour lives in
//! `piano_logic.rs`, widgets in `piano_widgets.rs`.

use super::aux_flag::{aux_flag_from_telemetry, aux_led};
use super::bay_piano_select::render_select;
use super::kb_zone;
use super::layout_px::{rect_or_empty, to_px};
use super::piano_logic as logic;
use super::piano_widgets as w;
use super::prelude::*;
use super::ui_state;
use crate::audio::PianoEdit;
use nord_panel::behaviour::Trigger;
use nord_panel::{section_rect, Section};

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_piano(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let o = panel_rect.min;
    state.piano_ui.begin_frame();
    let mut out: Vec<PianoEdit> = Vec::new();
    let shift = ui.input(|i| i.modifiers.shift) || state.piano_ui.shift_latch;

    section_frame(painter, to_px(section_rect(Section::Piano), o), "PIANO");
    super::frames::paint_frames(painter, o, Section::Piano);
    draw_shift_latch(ui, painter, o, state);
    draw_strip(ui, painter, o, state, shift, &mut out);
    render_select(ui, painter, o, state, shift, &mut out);

    for e in out {
        audio_handle.send_command(AudioCommand::Piano(e));
    }
}

/// Blink phase of the focus LED (2 Hz).
fn blink(ui: &Ui) -> bool {
    (ui.input(|i| i.time) * 4.0) as i64 % 2 == 0
}

/// On-panel SHIFT latch (stand-in for the Program bay's SHIFT key; the keyboard Shift key works too).
fn draw_shift_latch(ui: &mut Ui, p: &Painter, o: egui::Pos2, state: &mut Stage4PanelState) {
    let sec = to_px(section_rect(Section::Piano), o);
    let r = Rect::from_min_size(pos2(sec.right() - 30.0, sec.top() + 4.0), vec2(26.0, 14.0));
    let pr = w::press(ui, r, "piano_shift", &mut Default::default());
    if pr.resp.clicked() {
        state.piano_ui.shift_latch = !state.piano_ui.shift_latch;
    }
    w::face(p, r, pr.down, false);
    w::inside(p, r, "SHIFT", false);
    if state.piano_ui.shift_latch {
        w::mini_led(
            p,
            pos2(r.left() - 5.0, r.center().y),
            true,
            LED_AMBER,
            LED_AMBER_DIM,
            2.0,
            false,
        );
    }
    pr.resp.on_hover_text(
        "Piano-bay SHIFT latch (or hold the keyboard Shift key). Cleared after one shifted action.",
    );
}

fn draw_strip(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    shift: bool,
    out: &mut Vec<PianoEdit>,
) {
    use super::ui_state::slot;
    let r = |id: &str| rect_or_empty(id, o);

    // ON / SOLO (hold).
    let on = r("piano.on");
    let pr = w::press(ui, on, "piano_on", &mut state.piano_ui.trackers[slot::ON]);
    if let Some(g) = pr.gesture {
        match w::trigger(g, false, false, false) {
            Trigger::Hold => logic::solo_start(state, out),
            _ => logic::on_tap(state, out),
        }
    }
    if !pr.down {
        logic::solo_end(state, out);
    }
    let on_dim = ui_state::inventory_unsupported("piano.on");
    w::face(p, on, pr.down, on_dim);
    w::inside(p, on, "ON", on_dim);
    w::legend(p, on, "SOLO", on_dim);
    pr.resp.on_hover_text(
        "Piano ON/OFF. Hold roughly half a second for SOLO (only the Piano sounds while held).",
    );
    state.piano_ui.mark("piano.on");
    state.piano_ui.mark("piano.solo");

    // FX focus LED mirrors the focused section.
    let fx = r("piano.fx_focus_led");
    super::ctl::fx_focus_caption(p, fx.center());
    w::mini_led(
        p,
        fx.center(),
        state.fx_focus == 1,
        LED_GREEN,
        LED_GREEN_DIM,
        2.2,
        false,
    );
    state.piano_ui.mark("piano.fx_focus_led");

    // Layer buttons, focus LEDs, faders, ladders, aux LEDs.
    let ids = [
        (
            "a",
            "piano.layer.a",
            "piano.focus_led.a",
            "piano.fader.a",
            "piano.ladder.a",
            "piano.aux_led.a",
        ),
        (
            "b",
            "piano.layer.b",
            "piano.focus_led.b",
            "piano.fader.b",
            "piano.ladder.b",
            "piano.aux_led.b",
        ),
    ];
    let both = state.piano_a_active && state.piano_b_active;
    for (l, (name, btn, focus, fader, ladder, aux)) in ids.into_iter().enumerate() {
        let tr = slot::LAYER_A + l;
        let rect = r(btn);
        let pr = w::press(ui, rect, btn, &mut state.piano_ui.trackers[tr]);
        if let Some(g) = pr.gesture {
            match w::trigger(g, shift, true, false) {
                Trigger::Shifted => {
                    if l == 0 {
                        state.piano_ui.sustped = !state.piano_ui.sustped;
                        out.push(PianoEdit::Sustped(state.piano_ui.sustped));
                    } else {
                        state.piano_ui.pstick = !state.piano_ui.pstick;
                        out.push(PianoEdit::Pstick(state.piano_ui.pstick));
                    }
                    state.piano_ui.shift_latch = false;
                }
                Trigger::Hold => logic::layer_hold(state, l, out),
                _ => {
                    let keep = ui.input(|i| i.modifiers.command);
                    logic::layer_tap(state, l, keep, out);
                }
            }
        }
        w::face(p, rect, pr.down, false);
        w::inside(p, rect, &name.to_uppercase(), false);
        let (legend, ped_on) = if l == 0 {
            ("SUSTPED", state.piano_ui.sustped)
        } else {
            ("PSTICK", state.piano_ui.pstick)
        };
        w::legend(p, rect, legend, false);
        pr.resp.on_hover_text(format!(
            "Layer {}: tap = select, tap the other layer to switch (Ctrl+tap adds it), hold = off. Shift+tap = {legend} (now {}; both piano layers, via the engine pedal routing).",
            name.to_uppercase(),
            if ped_on { "on" } else { "off" }
        ));
        state.piano_ui.mark(btn);
        state.piano_ui.mark(if l == 0 {
            "piano.sustped"
        } else {
            "piano.pstick"
        });

        // Focus LED: steady when it is the only active layer, blinking when both are active.
        let active = if l == 0 {
            state.piano_a_active
        } else {
            state.piano_b_active
        };
        let focused = state.piano_ui.focus == l;
        let lit = active && focused && (!both || blink(ui));
        w::mini_led(
            p,
            r(focus).center(),
            lit,
            LED_GREEN,
            LED_GREEN_DIM,
            2.4,
            false,
        );
        state.piano_ui.mark(focus);
        if both {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        // Fader + ladder.
        let level = if l == 0 {
            state.piano_a_level
        } else {
            state.piano_b_level
        };
        let (new, resp) = w::fader(ui, r(fader), fader, level);
        if let Some(v) = new {
            logic::set_level(state, l, v, out);
        }
        resp.on_hover_text(format!(
            "Piano layer {} level (morphable)",
            name.to_uppercase()
        ));
        state.piano_ui.mark(fader);
        // Engine level after fader and Morph (UI value without an engine); Piano A/B are 2 / 3.
        let shown = state.telemetry.ladder_level(2 + l, level, active);
        w::ladder(p, r(ladder), 11, (shown * 11.0).round() as usize);
        state.piano_ui.mark(ladder);

        // Aux KB LED: the layer is played by the aux keyboard (click toggles the flag).
        let lit = aux_flag_from_telemetry(&state.telemetry, 2 + l, state.piano_ui.aux[l]);
        let tip = "AUX KB: this layer is played by the aux keyboard (click to switch)";
        if aux_led(ui, r(aux), aux, lit, tip) {
            state.piano_ui.aux[l] = !state.piano_ui.aux[l];
            out.push(PianoEdit::AuxKb {
                layer: l as u8,
                on: state.piano_ui.aux[l],
            });
        }
        state.piano_ui.mark(aux);
    }

    draw_octave_and_zone(ui, p, o, state, shift, out);
}

fn draw_octave_and_zone(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    shift: bool,
    out: &mut Vec<PianoEdit>,
) {
    use super::ui_state::slot;
    let r = |id: &str| rect_or_empty(id, o);
    let specs = [
        (
            "piano.octave_down",
            "piano.kb_zone_down",
            slot::OCT_DOWN,
            -1i8,
            "-",
        ),
        (
            "piano.octave_up",
            "piano.kb_zone_up",
            slot::OCT_UP,
            1i8,
            "+",
        ),
    ];
    for (oct_id, zone_id, tr, delta, sign) in specs {
        let rect = r(oct_id);
        let pr = w::press(ui, rect, oct_id, &mut state.piano_ui.trackers[tr]);
        if let Some(g) = pr.gesture {
            match w::trigger(g, shift, true, true) {
                Trigger::Shifted => {
                    logic::zone_step(state, delta > 0, out);
                    state.piano_ui.shift_latch = false;
                }
                _ => logic::octave_step(state, delta, out),
            }
        }
        w::face(p, rect, pr.down, false);
        w::inside(p, rect, &format!("OCT {sign}"), false);
        w::legend(p, rect, &format!("ZONE {sign}"), true);
        pr.resp.on_hover_text(format!(
            "OCTAVE SHIFT {sign} of the focused layer. Shift+tap = KB ZONE {sign} (moves the layer to another keyboard zone; zones act once a Split is on)."
        ));
        state.piano_ui.mark(oct_id);
        state.piano_ui.mark(zone_id);
    }

    let l = state.piano_ui.layer();
    let both = r("piano.octave_down").union(r("piano.octave_up"));
    w::caption(
        p,
        both,
        &format!("OCTAVE {:+}", state.piano_ui.octave[l]),
        false,
    );

    let zr = r("piano.zone_leds");
    let lits = kb_zone::lits(state.piano_ui.kb_zone[l]);
    w::led_strip(p, zr, 4, |i| lits[i], (LED_GREEN, LED_GREEN_DIM), false);
    let resp = ui.interact(
        zr.expand(2.0),
        ui.id().with("piano_zone_leds"),
        Sense::hover(),
    );
    resp.on_hover_text("KB ZONE LEDs: the keyboard zones the focused layer plays on");
    state.piano_ui.mark("piano.zone_leds");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_piano_paints_shapes() {
        let n = shapes_painted(render_bay_piano);
        assert!(n > 100, "render_bay_piano painted only {n} shapes");
    }

    #[test]
    fn every_inventory_id_of_the_piano_bay_has_a_painted_widget() {
        let mut ids = Vec::new();
        shapes_painted(|ui, p, rect, s, a| {
            render_bay_piano(ui, p, rect, s, a);
            ids = s.piano_ui.drawn.clone();
        });
        for c in nord_panel::all_controls() {
            if c.section == Section::Piano {
                assert!(ids.contains(&c.id.as_str()), "{} not drawn", c.id);
            }
        }
        let want = nord_panel::all_controls()
            .iter()
            .filter(|c| c.section == Section::Piano)
            .count();
        assert_eq!(want, 32);
        let mut uniq = ids.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), ids.len(), "an id was drawn twice");
        assert_eq!(uniq.len(), want, "drawn ids not in the inventory: {ids:?}");
    }

    #[test]
    fn every_inventory_control_has_a_visible_rect() {
        for c in nord_panel::all_controls() {
            if c.section == Section::Piano {
                let r = rect_or_empty(&c.id, egui::pos2(0.0, 0.0));
                assert!(r.width() > 0.0 && r.height() > 0.0, "{}", c.id);
            }
        }
    }

    #[test]
    fn dimmed_ids_carry_a_reason_naming_the_missing_feature() {
        let mut dimmed = Vec::new();
        shapes_painted(|ui, p, rect, s, a| {
            render_bay_piano(ui, p, rect, s, a);
            dimmed = s.piano_ui.dimmed.clone();
        });
        let ids: Vec<_> = dimmed.iter().map(|d| d.0).collect();
        // The dimmed set is exactly the inventory's remaining `Unsupported` piano ids.
        let mut want: Vec<String> = nord_panel::all_controls()
            .iter()
            .filter(|c| {
                c.section == Section::Piano
                    && matches!(c.binding, nord_panel::Binding::Unsupported(_))
            })
            .map(|c| c.id.clone())
            .collect();
        want.sort();
        let mut got: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        got.sort();
        assert_eq!(got, want);
        assert!(!got.contains(&"piano.sustped".to_string()));
        assert!(dimmed
            .iter()
            .all(|d| d.1.contains("unsupported") || d.1.contains("engine") || d.1.contains("no ")));
    }
}
