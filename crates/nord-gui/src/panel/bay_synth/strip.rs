//! Layer strip of the Synth section: ON, layer buttons, focus / aux LEDs, faders, ladders,
//! OCTAVE SHIFT, KB ZONE LEDs (manual p.27-28).

use super::widgets::{dot, fader, gesture_of, ladder, pad, tiny, LEGEND, LEGEND_DIM};
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::{unsupported_reason, Btn, LayerAction, SynthUi};
use egui::Pos2;

const FADERS: [&str; 3] = ["synth.fader.a", "synth.fader.b", "synth.fader.c"];
const LADDERS: [&str; 3] = ["synth.ladder.a", "synth.ladder.b", "synth.ladder.c"];
const FOCUS_LEDS: [&str; 3] = [
    "synth.focus_led.a",
    "synth.focus_led.b",
    "synth.focus_led.c",
];
const AUX_LEDS: [&str; 3] = ["synth.aux_led.a", "synth.aux_led.b", "synth.aux_led.c"];
const LAYER_BTNS: [&str; 3] = ["synth.layer.a", "synth.layer.b", "synth.layer.c"];

/// Send a layer's level and on/off state to the DSP thread.
pub(super) fn send_layer(audio: &AudioHandle, i: usize, level: f32, on: bool) {
    let v = if on { level } else { 0.0 };
    audio.send_command(match i {
        0 => AudioCommand::SetSynthLevel(v),
        1 => AudioCommand::SetSynthBLevel(v),
        _ => AudioCommand::SetSynthCLevel(v),
    });
    audio.send_command(AudioCommand::SetLayerActive(4 + i, on));
}

/// A layer button: tap focuses (and turns the layer on), hold turns it off, Shift+A / Shift+B are
/// SUSTPED / PSTICK. Returns what to do with the layer's ON state.
fn layer_pad(
    ui: &mut Ui,
    p: &Painter,
    o: Pos2,
    sui: &mut SynthUi,
    i: usize,
    active: bool,
) -> LayerAction {
    let id = LAYER_BTNS[i];
    let rect = rect_or_empty(id, o);
    sui.mark(id);
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    let mut action = LayerAction::None;
    if let Some(g) = gesture_of(ui, sui, id, &resp) {
        let shift = ui.input(|i| i.modifiers.shift);
        action = sui.layer_button(i, g, shift, active);
    }
    p.rect_filled(rect.expand(1.0), 2.5, BUTTON_BEZEL);
    p.rect_filled(rect, 2.0, BUTTON_GREY);
    tiny(
        p,
        rect.center(),
        ["A", "B", "C"][i],
        Color32::BLACK,
        Align2::CENTER_CENTER,
    );
    let sub = match i {
        0 => Some(("synth.sustped", "SUSTPED")),
        1 => Some(("synth.pstick", "PSTICK")),
        _ => None,
    };
    if let Some((sid, legend)) = sub {
        sui.mark(sid);
        let why = unsupported_reason(sid);
        tiny(
            p,
            pos2(rect.center().x, rect.bottom() + 3.0),
            legend,
            if why.is_some() { LEGEND_DIM } else { LEGEND },
            Align2::CENTER_TOP,
        );
        if let Some(why) = why {
            let _ = resp.on_hover_text(format!("{legend} (Shift): {why}"));
        }
    }
    action
}

#[allow(clippy::too_many_arguments)]
/// Paint and drive the whole strip. `levels` / `active` are the three layers' fader and ON state.
pub(super) fn paint_strip(
    ui: &mut Ui,
    p: &Painter,
    o: Pos2,
    sui: &mut SynthUi,
    levels: &mut [f32; 3],
    active: &mut [bool; 3],
    fx_focus: usize,
    audio: &AudioHandle,
    tele: &crate::telemetry::PanelTelemetry,
) {
    let before_on = sui.section_on;
    pad(ui, p, o, sui, Btn::On, "ON");
    sui.mark("synth.fx_focus_led");
    let fx = rect_or_empty("synth.fx_focus_led", o).center();
    dot(p, fx, fx_focus == 2, LED_AMBER, LED_AMBER_DIM);

    let blink_phase = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
    let n_active = active.iter().filter(|a| **a).count();
    if n_active >= 2 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    for i in 0..3 {
        let on = active[i] && sui.section_on;
        let fr = rect_or_empty(FADERS[i], o);
        sui.mark(FADERS[i]);
        if fader(ui, p, FADERS[i], fr, &mut levels[i]) {
            send_layer(audio, i, levels[i], on);
        }
        sui.mark(LADDERS[i]);
        ladder(
            p,
            rect_or_empty(LADDERS[i], o),
            // Engine level after fader and Morph (Synth A/B/C are layers 4..7).
            tele.ladder_level(4 + i, levels[i], on),
            10,
        );
        sui.mark(FOCUS_LEDS[i]);
        // ASSUMED: the focused layer's LED blinks only while several layers are on.
        let focused = sui.focus == i;
        let lit = focused && (n_active < 2 || blink_phase);
        dot(
            p,
            rect_or_empty(FOCUS_LEDS[i], o).center(),
            lit,
            LED_RED,
            LED_RED_DIM,
        );
        sui.mark(AUX_LEDS[i]);
        let aux = rect_or_empty(AUX_LEDS[i], o);
        dot(p, aux.center(), tele.aux(4 + i), LED_GREEN, LED_GREEN_DIM);
        if let Some(why) = unsupported_reason(AUX_LEDS[i]).filter(|_| tele.get().is_none()) {
            let _ = ui
                .interact(aux.expand(2.0), ui.id().with(AUX_LEDS[i]), Sense::hover())
                .on_hover_text(format!("AUX KB: {why}"));
        }
        match layer_pad(ui, p, o, sui, i, active[i]) {
            LayerAction::Activate => {
                active[i] = true;
                send_layer(audio, i, levels[i], sui.section_on);
            }
            LayerAction::Deactivate => {
                active[i] = false;
                send_layer(audio, i, levels[i], false);
            }
            LayerAction::None => {}
        }
    }
    if sui.section_on != before_on {
        for i in 0..3 {
            send_layer(audio, i, levels[i], active[i] && sui.section_on);
        }
    }

    pad(ui, p, o, sui, Btn::OctaveDown, "OCTAVE");
    pad(ui, p, o, sui, Btn::OctaveUp, "SHIFT");
    sui.mark("synth.zone_leds");
    let z = rect_or_empty("synth.zone_leds", o);
    for k in 0..4 {
        let c = pos2(z.left() + (k as f32 + 0.5) * z.width() / 4.0, z.center().y);
        dot(
            p,
            c,
            usize::from(sui.layer().kb_zone) == k,
            LED_GREEN,
            LED_GREEN_DIM,
        );
    }
    if let Some(why) = unsupported_reason("synth.zone_leds") {
        let _ = ui
            .interact(z.expand(2.0), ui.id().with("zone_leds"), Sense::hover())
            .on_hover_text(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn strip_paints_and_marks_all_layer_strip_ids() {
        let mut sui = SynthUi::new();
        let mut lv = [0.7, 0.0, 0.0];
        let mut act = [true, false, false];
        let n = shapes_painted(|ui, p, rect, _s, a| {
            sui.begin_frame();
            paint_strip(
                ui,
                p,
                rect.min,
                &mut sui,
                &mut lv,
                &mut act,
                2,
                a,
                &Default::default(),
            );
        });
        assert!(n > 40, "painted {n}");
        for id in [
            "synth.on",
            "synth.solo",
            "synth.fx_focus_led",
            "synth.fader.b",
            "synth.ladder.c",
            "synth.focus_led.a",
            "synth.aux_led.c",
            "synth.layer.a",
            "synth.sustped",
            "synth.pstick",
            "synth.octave_down",
            "synth.kb_zone_up",
            "synth.zone_leds",
        ] {
            assert!(sui.drawn().contains(&id), "{id}");
        }
    }

    #[test]
    fn send_layer_zeroes_the_level_of_an_off_layer() {
        let (h, rx) = AudioHandle::new();
        send_layer(&h, 1, 0.6, false);
        send_layer(&h, 2, 0.4, true);
        let cmds: Vec<_> = rx.try_iter().collect();
        assert!(matches!(cmds[0], AudioCommand::SetSynthBLevel(v) if v == 0.0));
        assert!(matches!(cmds[1], AudioCommand::SetLayerActive(5, false)));
        assert!(matches!(cmds[2], AudioCommand::SetSynthCLevel(v) if v == 0.4));
        assert!(matches!(cmds[3], AudioCommand::SetLayerActive(6, true)));
    }
}
