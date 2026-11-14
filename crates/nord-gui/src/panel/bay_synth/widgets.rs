//! Widgets of the Synth bay: pad buttons with SHIFT legends, pots with pick-up, OLED dials, faders.
//!
//! Every widget takes its rectangle from `layout_px::rect_or_empty(id, origin)` and records the
//! inventory ids it paints with `SynthUi::mark` (the headless coverage test reads them).

use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::{unsupported_reason, Btn, Knob, SynthUi};
use egui::{FontId, Pos2};
use nord_panel::behaviour::{resolve, Gesture};

/// Silkscreen legend colour (printed text).
pub(super) const LEGEND: Color32 = Color32::from_gray(205);
/// Legend of a control that has no engine support.
pub(super) const LEGEND_DIM: Color32 = Color32::from_gray(105);

/// Small silkscreen text.
pub(super) fn tiny(p: &Painter, pos: Pos2, s: &str, col: Color32, al: Align2) {
    p.text(pos, al, s, FontId::proportional(6.5), col);
}

/// Small round LED.
pub(super) fn dot(p: &Painter, c: Pos2, on: bool, col_on: Color32, col_off: Color32) {
    p.circle_filled(c, 3.0, Color32::from_rgb(14, 16, 19));
    p.circle_filled(c, 2.3, if on { col_on } else { col_off });
}

/// Monotonic time in ms for the gesture trackers.
fn now_ms(ui: &Ui) -> u32 {
    (ui.input(|i| i.time) * 1000.0) as u32
}

/// Gesture of the pointer on `resp`, via the button's `ButtonTracker`.
pub(super) fn gesture_of(
    ui: &Ui,
    sui: &mut SynthUi,
    id: &'static str,
    resp: &egui::Response,
) -> Option<Gesture> {
    let t = now_ms(ui);
    let down = resp.is_pointer_button_down_on();
    let tr = sui.tracker(id);
    let was_down = tr.is_down();
    let mut g = None;
    if down && !was_down {
        g = tr.press(t);
    }
    if down {
        g = g.or(tr.tick(t));
        ui.ctx().request_repaint();
    }
    if !down && was_down {
        g = g.or(tr.release(t));
    }
    if g.is_none() && !down && !was_down && resp.clicked() {
        // Press and release inside one frame (touch, scripted input): a plain tap.
        g = Some(Gesture::Tap);
    }
    g
}

/// A black pad button with a legend above, an optional SHIFT legend below and a status text.
pub(super) fn pad(ui: &mut Ui, p: &Painter, o: Pos2, sui: &mut SynthUi, b: Btn, label: &str) {
    let id = b.id();
    let rect = rect_or_empty(id, o);
    sui.mark(id);
    let alias = b.alias();
    if let Some((aid, _)) = alias {
        sui.mark(aid);
    }
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    if let Some(g) = gesture_of(ui, sui, id, &resp) {
        let shift = ui.input(|i| i.modifiers.shift);
        sui.press(b, resolve(g, shift, b.has_shift_fn()));
    }
    let down = resp.is_pointer_button_down_on();
    p.rect_filled(rect.expand(1.0), 2.5, BUTTON_BEZEL);
    p.rect_filled(
        rect,
        2.0,
        if down {
            BUTTON_BLACK.gamma_multiply(0.7)
        } else {
            BUTTON_BLACK
        },
    );
    if sui.page.button_id() == id {
        p.rect_stroke(
            rect.expand(2.0),
            3.0,
            Stroke::new(1.4_f32, LED_RED),
            StrokeKind::Outside,
        );
    }
    let cy = rect.center().y;
    dot(
        p,
        pos2(rect.left() + 4.5, cy),
        sui.primary_lit(b),
        LED_RED,
        LED_RED_DIM,
    );
    let status = sui.status_text(b);
    if !status.is_empty() {
        tiny(
            p,
            pos2(rect.center().x + 3.0, cy),
            &status,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }
    tiny(
        p,
        pos2(rect.center().x, rect.top() - 3.0),
        label,
        Color32::WHITE,
        Align2::CENTER_BOTTOM,
    );
    if let Some((aid, legend)) = alias {
        let why = unsupported_reason(aid);
        let col = if why.is_some() {
            LEGEND_DIM
        } else if sui.alias_lit(b) {
            LED_AMBER
        } else {
            LEGEND
        };
        tiny(
            p,
            pos2(rect.center().x, rect.bottom() + 3.0),
            legend,
            col,
            Align2::CENTER_TOP,
        );
        if let Some(why) = why {
            let _ = resp.on_hover_text(format!("{legend} (Shift): {why}"));
        }
    }
}

/// A pot with pick-up: it moves with the pointer; the layer value follows once picked up.
pub(super) fn pot(ui: &mut Ui, p: &Painter, o: Pos2, sui: &mut SynthUi, k: Knob, label: &str) {
    let rect = rect_or_empty(k.id(), o);
    sui.mark(k.id());
    let rate = matches!(k, Knob::LfoRate | Knob::ArpRate);
    if rate {
        sui.mark(if k == Knob::LfoRate {
            "synth.lfo.rate_mst_clk"
        } else {
            "synth.arp.rate_mst_clk"
        });
    }
    let radius = (rect.width().min(rect.height()) * 0.5).max(5.0);
    let center = rect.center();
    let old = sui.pots.phys(k);
    let mut v = old;
    if knob(ui, center, radius, k.id(), &mut v, 0.0, 1.0, "") {
        let shift = ui.input(|i| i.modifiers.shift);
        if rate && shift {
            sui.rate_clock_sync(k, v > old);
        } else {
            sui.knob_input(k, v);
        }
    }
    tiny(
        p,
        pos2(center.x, center.y - radius - 6.0),
        label,
        Color32::WHITE,
        Align2::CENTER_BOTTOM,
    );
    if !sui.pots.is_engaged(k) {
        // Pick-up marker: where the stored value of this layer sits on the scale.
        let a = 0.75 * std::f32::consts::PI + 1.5 * std::f32::consts::PI * sui.pots.stored(k);
        let r = radius + 4.5;
        p.circle_filled(
            pos2(center.x + a.cos() * r, center.y + a.sin() * r),
            1.8,
            LED_AMBER,
        );
    }
    if rate {
        let synced = if k == Knob::LfoRate {
            sui.layer().lfo_sync
        } else {
            sui.layer().arp_sync
        };
        tiny(
            p,
            pos2(center.x, center.y + radius + 5.0),
            "MST CLK",
            if synced { LED_AMBER } else { LEGEND },
            Align2::CENTER_TOP,
        );
    }
}

/// The three inventory ids of the OLED dials.
pub(super) const DIAL_IDS: [&str; 3] = ["synth.dial.left", "synth.dial.middle", "synth.dial.right"];

/// An endless encoder under the OLED: drag or scroll to turn; Shift+turn is the LIST / INFO view.
pub(super) fn dial(ui: &mut Ui, p: &Painter, o: Pos2, sui: &mut SynthUi, idx: usize) {
    let id = DIAL_IDS[idx];
    let rect = rect_or_empty(id, o);
    sui.mark(id);
    match idx {
        0 => sui.mark("synth.dial_info"),
        2 => sui.mark("synth.dial_list"),
        _ => {}
    }
    let radius = (rect.width().min(rect.height()) * 0.5).max(6.0);
    let c = rect.center();
    let resp = ui.interact(rect.expand(2.0), ui.id().with(id), Sense::click_and_drag());
    let mut delta = 0.0;
    if resp.dragged() {
        delta -= resp.drag_delta().y / 5.0;
    }
    if resp.hovered() {
        delta += ui.input(|i| i.raw_scroll_delta.y) / 24.0;
    }
    let ang_id = ui.id().with((id, "angle"));
    let mut ang = ui.data(|d| d.get_temp::<f32>(ang_id)).unwrap_or(0.0);
    sui.dial_acc[idx] += delta;
    let steps = sui.dial_acc[idx].trunc();
    sui.dial_acc[idx] -= steps;
    if steps != 0.0 {
        let shift = ui.input(|i| i.modifiers.shift);
        sui.turn_dial(idx, steps as i32, shift);
        ang += steps * 0.45;
        ui.data_mut(|d| d.insert_temp(ang_id, ang));
    }
    let why = sui.dial_unsupported(idx);
    let k = if why.is_some() { 0.55 } else { 1.0 };
    p.circle_filled(
        c + vec2(0.0, 1.5),
        radius + 1.0,
        Color32::from_black_alpha(110),
    );
    p.circle_filled(c, radius, Color32::from_rgb(34, 37, 42).gamma_multiply(k));
    p.circle_stroke(c, radius, Stroke::new(1.0_f32, Color32::from_gray(70)));
    let a = ang - std::f32::consts::FRAC_PI_2;
    p.line_segment(
        [
            c + vec2(a.cos(), a.sin()) * radius * 0.25,
            c + vec2(a.cos(), a.sin()) * radius * 0.85,
        ],
        Stroke::new(1.6_f32, Color32::from_gray(235).gamma_multiply(k)),
    );
    if let Some(why) = why {
        let _ = resp.on_hover_text(why);
    }
}

/// Vertical level fader; returns `true` when the level changed.
pub(super) fn fader(
    ui: &mut Ui,
    p: &Painter,
    id: &'static str,
    rect: Rect,
    level: &mut f32,
) -> bool {
    let resp = ui.interact(
        rect.expand2(vec2(4.0, 0.0)),
        ui.id().with(id),
        Sense::click_and_drag(),
    );
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let n = 1.0 - ((pos.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
            if (n - *level).abs() > f32::EPSILON {
                *level = n;
                changed = true;
            }
        }
    }
    p.rect_filled(
        Rect::from_center_size(rect.center(), vec2(2.5, rect.height())),
        1.0,
        Color32::from_rgb(10, 11, 13),
    );
    let y = rect.bottom() - rect.height() * level.clamp(0.0, 1.0);
    let cap = Rect::from_center_size(pos2(rect.center().x, y), vec2(rect.width().max(9.0), 6.0));
    p.rect_filled(cap, 1.5, Color32::from_gray(190));
    p.line_segment(
        [pos2(cap.left(), y), pos2(cap.right(), y)],
        Stroke::new(1.0_f32, Color32::from_gray(40)),
    );
    changed
}

/// LED ladder of `n` segments lit up to `level`.
pub(super) fn ladder(p: &Painter, rect: Rect, level: f32, n: usize) {
    let step = rect.height() / n as f32;
    let lit = (level.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..n {
        let y = rect.bottom() - (i as f32 + 0.5) * step;
        dot(
            p,
            pos2(rect.center().x, y),
            i < lit,
            LED_GREEN,
            LED_GREEN_DIM,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn pad_pot_dial_fader_and_ladder_paint_and_mark_their_ids() {
        let mut sui = SynthUi::new();
        let n = shapes_painted(|ui, p, rect, _s, _a| {
            sui.begin_frame();
            pad(ui, p, rect.min, &mut sui, Btn::FilterType, "TYPE");
            pot(ui, p, rect.min, &mut sui, Knob::LfoRate, "RATE");
            dial(ui, p, rect.min, &mut sui, 0);
            let mut lv = 0.5;
            fader(
                ui,
                p,
                "f",
                rect_or_empty("synth.fader.a", rect.min),
                &mut lv,
            );
            ladder(p, rect_or_empty("synth.ladder.a", rect.min), 0.5, 10);
        });
        assert!(n > 20, "painted {n} shapes");
        for id in [
            "synth.filter.type",
            "synth.filter.group",
            "synth.lfo.rate",
            "synth.lfo.rate_mst_clk",
            "synth.dial.left",
            "synth.dial_info",
        ] {
            assert!(sui.drawn().contains(&id), "{id}");
        }
    }

    #[test]
    fn a_button_click_inside_one_frame_is_a_tap() {
        let ctx = egui::Context::default();
        let mut sui = SynthUi::new();
        let mut input = egui::RawInput::default();
        let rect = rect_or_empty("synth.filter.type", pos2(0.0, 0.0));
        let c = rect.center();
        input.events.push(egui::Event::PointerMoved(c));
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let p = ui.painter().clone();
                pad(ui, &p, pos2(0.0, 0.0), &mut sui, Btn::FilterType, "TYPE");
            });
        });
        for pressed in [true, false] {
            let mut i = egui::RawInput::default();
            i.events.push(egui::Event::PointerButton {
                pos: c,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            let _ = ctx.run(i, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let p = ui.painter().clone();
                    pad(ui, &p, pos2(0.0, 0.0), &mut sui, Btn::FilterType, "TYPE");
                });
            });
        }
        assert_eq!(sui.page, crate::panel::ui_state::SynthPage::FilterType);
    }
}
