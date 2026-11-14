//! Pots with pick-up and the OLED dials of the Synth bay (split from `widgets.rs`).
//!
//! A pot prints its name below the knob, in the open gap of the tick ring, as the panel does, so
//! the numerals around the ring (`widgets::scale`) stay clear.

use super::widgets::{tiny, LEGEND};
use crate::panel::layout_px::rect_or_empty;
use crate::panel::param_hint::{self};
use crate::panel::prelude::*;
use crate::panel::ui_state::{Knob, SynthUi, DIVISIONS};
use egui::Pos2;

/// Parameter hint of pot `k`: the value stored for the focused layer (the pot itself may not have
/// picked it up yet), or the Master Clock division while the rate is synced.
pub(super) fn pot_hint(ctx: &egui::Context, sui: &SynthUi, k: Knob) {
    let l = sui.layer();
    let div = match k {
        Knob::LfoRate if l.lfo_sync => Some(l.lfo_div),
        Knob::ArpRate if l.arp_sync => Some(l.arp_div),
        _ => None,
    };
    if let Some(d) = div {
        param_hint::touch_value(ctx, k.id(), DIVISIONS[usize::from(d) % DIVISIONS.len()]);
    } else if k == Knob::ArpRange && l.arp_kind == 2 {
        param_hint::touch_value(ctx, k.id(), &format!("Gate {:.0} %", l.gate_env * 100.0));
    } else {
        param_hint::touch(ctx, k.id(), sui.pots.stored(k));
    }
}

/// Gap (px) between the knob body and its name, printed in the open bottom of the ring below the
/// numerals `0` and `10` (rule `L`).
const NAME_DROP: f32 = 7.5;
/// Pitch (px) of the name and the MST CLK line under it (rule `V` keeps stacked labels 2 px apart).
const NAME_LINE: f32 = 9.5;

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
        pot_hint(ui.ctx(), sui, k);
    }
    tiny(
        p,
        pos2(center.x, center.y + radius + NAME_DROP),
        label,
        Color32::WHITE,
        Align2::CENTER_TOP,
    );
    if !sui.pots.is_engaged(k) {
        // Pick-up marker: where the stored value of this layer sits on the scale.
        let a = 0.75 * std::f32::consts::PI + 1.5 * std::f32::consts::PI * sui.pots.stored(k);
        let r = crate::widgets::scale::body_radius(k.id(), radius) + 4.5;
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
            pos2(center.x, center.y + radius + NAME_DROP + NAME_LINE),
            "MST CLK",
            if synced { LED_AMBER } else { LEGEND },
            Align2::CENTER_TOP,
        );
    }
}

/// The three inventory ids of the OLED dials.
pub(in crate::panel::bay_synth) const DIAL_IDS: [&str; 3] =
    ["synth.dial.left", "synth.dial.middle", "synth.dial.right"];

/// Names printed under the three OLED dials, left to right.
pub(super) const DIAL_NAMES: [&str; 3] = ["INFO", "LIST", "LIST"];

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
    // Printed under the knob: INFO, LIST, LIST (photo `NS4-Panel-synth`).
    tiny(
        p,
        pos2(c.x, c.y + radius + 4.0),
        DIAL_NAMES[idx],
        LEGEND,
        Align2::CENTER_TOP,
    );
    if let Some(why) = why {
        let _ = resp.on_hover_text(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn a_pot_prints_its_name_below_the_knob_and_marks_its_ids() {
        let mut sui = SynthUi::new();
        let n = shapes_painted(|ui, p, rect, _s, _a| {
            sui.begin_frame();
            pot(ui, p, rect.min, &mut sui, Knob::LfoRate, "RATE");
            dial(ui, p, rect.min, &mut sui, 1);
        });
        assert!(n > 10, "painted {n} shapes");
        for id in [
            "synth.lfo.rate",
            "synth.lfo.rate_mst_clk",
            "synth.dial.middle",
        ] {
            assert!(sui.drawn().contains(&id), "{id}");
        }
    }

    #[test]
    fn the_three_dials_have_ids_and_names() {
        assert_eq!(DIAL_IDS.len(), DIAL_NAMES.len());
        assert_eq!(DIAL_NAMES, ["INFO", "LIST", "LIST"]);
    }
}
