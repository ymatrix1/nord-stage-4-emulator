//! Organ shared strip (manual p.18): ON/SOLO, layer buttons with focus LEDs, level faders and
//! ladders, Aux KB LEDs, OCTAVE SHIFT, KB ZONE, SUSTPED and PSTICK.
//!
//! Positions come from `layout_px::rect_or_empty`. Shift functions share their base control's
//! rect and are printed as legends below it ([`ctl::shift_legend`]).

use super::aux_flag::{aux_flag_from_telemetry, aux_led};
use super::ctl::{gesture_button, led_row, mark, shift_legend, Face};
use super::kb_zone;
use super::layout_px::rect_or_empty;
use super::organ_logic::step_octave;
use super::organ_widgets::{level_fader, level_ladder};
use super::prelude::*;
use crate::audio::OrganEdit;
use nord_panel::behaviour::Trigger;

/// Focus LED blink rate. ASSUMED: 2 Hz; the manual says "blinking" without a rate (p.18).
const BLINK_HZ: f64 = 2.0;

const LAYER_IDS: [(&str, &str, &str, &str, &str); 2] = [
    (
        "organ.layer.a",
        "organ.focus_led.a",
        "organ.fader.a",
        "organ.ladder.a",
        "organ.aux_led.a",
    ),
    (
        "organ.layer.b",
        "organ.focus_led.b",
        "organ.fader.b",
        "organ.ladder.b",
        "organ.aux_led.b",
    ),
];

/// Send level and enable state of organ layer `l` to the engine.
pub(super) fn send_layer(state: &Stage4PanelState, audio: &AudioHandle, l: usize) {
    let lvl = if state.organ_layer_active(l) {
        state.organ_layer_level(l)
    } else {
        0.0
    };
    audio.send_command(if l == 0 {
        AudioCommand::SetOrganLevel(lvl)
    } else {
        AudioCommand::SetOrganBLevel(lvl)
    });
    audio.send_command(AudioCommand::SetLayerActive(l, state.organ_layer_active(l)));
}

/// Tap on a layer button: focus it (switching it on if needed); a tap on the focused layer
/// toggles it. Returns `true` when the enable state changed.
pub(super) fn layer_tap(state: &mut Stage4PanelState, l: usize) -> bool {
    let toggle = |on: &mut bool| *on = !*on;
    if state.organ_ui.focus_layer != l {
        state.organ_ui.focus_layer = l;
        if !state.organ_layer_active(l) {
            toggle(if l == 0 {
                &mut state.organ_a_active
            } else {
                &mut state.organ_b_active
            });
            return true;
        }
        return false;
    }
    toggle(if l == 0 {
        &mut state.organ_a_active
    } else {
        &mut state.organ_b_active
    });
    true
}

fn blink(ui: &Ui) -> bool {
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(250));
    ui.input(|i| (i.time * BLINK_HZ * 2.0) as u64)
        .is_multiple_of(2)
}

pub(super) fn render_strip(
    ui: &mut Ui,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let rc = |id: &str| rect_or_empty(id, origin);
    let focus = state.organ_ui.focus_layer;

    // ON (tap) / SOLO (hold).
    let section_on = state.organ_section_on();
    let t = gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        rc("organ.on"),
        "organ.on",
        false,
        Face {
            led: Some(section_on),
            tip: "ON: tap switches every Organ layer off, or back on. Hold: SOLO",
            ..Face::black()
        },
    );
    match t {
        Some(Trigger::Primary) => {
            let changed = state.organ_on_tap();
            for (l, _) in changed.iter().enumerate().filter(|(_, c)| **c) {
                send_layer(state, audio, l);
            }
        }
        Some(Trigger::Hold) => state.organ_ui.solo = !state.organ_ui.solo,
        _ => {}
    }
    shift_legend(
        ui,
        rc("organ.on"),
        "organ.solo",
        "SOLO",
        Some(state.organ_ui.solo),
        None,
    );
    led_row(
        ui,
        rc("organ.fx_focus_led"),
        "organ.fx_focus_led",
        &[state.fx_focus == 0],
        (LED_AMBER, LED_AMBER_DIM),
        None,
    );

    // Layers A and B.
    let phase = blink(ui);
    for (l, ids) in LAYER_IDS.iter().enumerate() {
        let (layer_id, focus_id, fader_id, ladder_id, aux_id) = *ids;
        let active = state.organ_layer_active(l);
        let face = Face {
            grey: true,
            led: Some(active),
            tip: "Tap: focus / switch layer on, tap again: off. Shift or hold: SUSTPED / PSTICK",
            ..Face::black()
        };
        match gesture_button(
            ui,
            &mut state.organ_ui.trackers,
            rc(layer_id),
            layer_id,
            true,
            face,
        ) {
            Some(Trigger::Primary) => {
                if layer_tap(state, l) {
                    send_layer(state, audio, l);
                }
            }
            Some(Trigger::Shifted) => {
                let flag = if l == 0 {
                    &mut state.organ_ui.sustped[focus]
                } else {
                    &mut state.organ_ui.pstick[focus]
                };
                *flag = !*flag;
                let on = *flag;
                audio.send_command(AudioCommand::Organ(if l == 0 {
                    OrganEdit::Sustped {
                        layer: focus as u8,
                        on,
                    }
                } else {
                    OrganEdit::Pstick {
                        layer: focus as u8,
                        on,
                    }
                }));
            }
            _ => {}
        }
        let (sh_id, sh_text, sh_lit) = if l == 0 {
            ("organ.sustped", "SUSTPED", state.organ_ui.sustped[focus])
        } else {
            ("organ.pstick", "PSTICK", state.organ_ui.pstick[focus])
        };
        shift_legend(ui, rc(layer_id), sh_id, sh_text, Some(sh_lit), None);
        led_row(
            ui,
            rc(focus_id),
            focus_id,
            &[focus == l && phase],
            (LED_GREEN, LED_GREEN_DIM),
            None,
        );
        let mut lvl = state.organ_layer_level(l);
        if level_fader(ui, rc(fader_id), fader_id, &mut lvl) {
            if l == 0 {
                state.organ_a_level = lvl;
            } else {
                state.organ_b_level = lvl;
            }
            send_layer(state, audio, l);
        }
        level_ladder(
            ui,
            rc(ladder_id),
            ladder_id,
            lvl,
            state.organ_layer_active(l),
        );
        let lit = aux_flag_from_telemetry(state.organ_ui.aux[l]);
        let tip = "AUX KB: this layer is played by the aux keyboard (click to switch)";
        if aux_led(ui, rc(aux_id), aux_id, lit, tip) {
            state.organ_ui.aux[l] = !state.organ_ui.aux[l];
            audio.send_command(AudioCommand::Organ(OrganEdit::AuxKb {
                layer: l as u8,
                on: state.organ_ui.aux[l],
            }));
        }
        mark(ui, aux_id);
    }

    // OCTAVE SHIFT with KB ZONE on Shift.
    for (up, id, zone_id, text) in [
        (false, "organ.octave_down", "organ.kb_zone_down", "ZONE-"),
        (true, "organ.octave_up", "organ.kb_zone_up", "ZONE+"),
    ] {
        let tip = format!(
            "Octave shift {:+} (layer in focus)",
            state.organ_octave_of(focus)
        );
        let face = Face {
            tip: &tip,
            ..Face::black()
        };
        match gesture_button(ui, &mut state.organ_ui.trackers, rc(id), id, true, face) {
            Some(Trigger::Primary) => {
                let v = step_octave(state.organ_octave_of(focus), up);
                state.set_organ_octave(focus, v);
                audio.send_command(AudioCommand::Organ(OrganEdit::Octave {
                    layer: focus as u8,
                    shift: v,
                }));
            }
            Some(Trigger::Shifted) => {
                let z = &mut state.organ_ui.kb_zone[focus];
                *z = kb_zone::step(*z, up);
                audio.send_command(AudioCommand::Organ(OrganEdit::Zone {
                    layer: focus as u8,
                    leds: kb_zone::leds(*z),
                }));
            }
            _ => {}
        }
        shift_legend(ui, rc(id), zone_id, text, None, None);
    }
    led_row(
        ui,
        rc("organ.zone_leds"),
        "organ.zone_leds",
        &kb_zone::lits(state.organ_ui.kb_zone[focus]),
        (LED_GREEN, LED_GREEN_DIM),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn strip_paints_shapes() {
        let n = shapes_painted(|ui, _p, rect, state, audio| {
            render_strip(ui, rect.min, state, audio);
        });
        assert!(n > 60, "strip painted only {n} shapes");
    }

    #[test]
    fn layer_tap_focuses_then_toggles() {
        let mut s = Stage4PanelState::default();
        // B is off and unfocused: first tap focuses and enables it.
        assert!(layer_tap(&mut s, 1));
        assert!(s.organ_b_active && s.organ_ui.focus_layer == 1);
        // Tapping A (active, unfocused) only moves focus.
        assert!(!layer_tap(&mut s, 0));
        assert_eq!(s.organ_ui.focus_layer, 0);
        // Tapping the focused layer toggles it off.
        assert!(layer_tap(&mut s, 0));
        assert!(!s.organ_a_active);
    }

    #[test]
    fn send_layer_sends_level_and_enable() {
        let s = Stage4PanelState::default();
        let (audio, rx) = AudioHandle::new();
        send_layer(&s, &audio, 1);
        assert!(matches!(rx.try_recv(), Ok(AudioCommand::SetOrganBLevel(l)) if l == 0.0));
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::SetLayerActive(1, false))
        ));
    }
}
