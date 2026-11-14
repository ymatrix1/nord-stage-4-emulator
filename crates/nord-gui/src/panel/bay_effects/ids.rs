//! Control ids of the Layer Effects bay, where each one is drawn, and when it is dimmed.
//!
//! Every id of `nord_panel::inventory::effects` is listed in [`HANDLED`] (a test compares the
//! list with the inventory). Shifted functions share their base control's layout rectangle, so a
//! legend under the base control is their widget; three direct controls the layout puts into one
//! rectangle (`amp.filter`, `amp.to_rotary`, `amp.res`) get a sub-rectangle next to it.

use crate::panel::layout_px::rect_or_empty;
use crate::panel::ui_state::fx::ChainUi;
use egui::{pos2, vec2, Pos2, Rect};

/// All 51 inventory ids of the Layer Effects section, drawn by this bay.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const HANDLED: [&str; 51] = [
    "fx.layer_effects_on",
    "fx.focus.organ",
    "fx.focus.piano",
    "fx.focus.synth",
    "fx.focus.group_piano",
    "fx.focus.group_synth",
    "fx.focus.leds",
    "mod1.on",
    "mod1.selector",
    "mod1.variation",
    "mod1.rate",
    "mod1.mst_clk",
    "mod1.amount",
    "mod2.on",
    "mod2.selector",
    "mod2.variation",
    "mod2.rate",
    "mod2.amount",
    "delay.on",
    "delay.global",
    "delay.tempo",
    "delay.mst_clk",
    "delay.feedback",
    "delay.dry_wet",
    "delay.effects",
    "delay.effects_variation",
    "delay.filter",
    "delay.ping_pong",
    "delay.tap_set",
    "delay.analog",
    "amp.on",
    "amp.drive",
    "amp.model",
    "amp.variation",
    "amp.bass",
    "amp.mid",
    "amp.mid_freq",
    "amp.treble",
    "amp.res",
    "amp.filter",
    "amp.to_rotary",
    "comp.on",
    "comp.global",
    "comp.amount",
    "comp.fast",
    "reverb.on",
    "reverb.global",
    "reverb.type",
    "reverb.variation",
    "reverb.dry_wet",
    "reverb.bright_dark",
];

/// Shifted functions: `(id, base control, caption line below the base)`.
const LEGENDS: [(&str, &str, usize); 15] = [
    ("fx.focus.group_piano", "fx.focus.piano", 1),
    ("fx.focus.group_synth", "fx.focus.synth", 1),
    ("mod1.variation", "mod1.selector", 0),
    ("mod1.mst_clk", "mod1.rate", 1),
    ("mod2.variation", "mod2.selector", 0),
    ("delay.global", "delay.on", 0),
    ("delay.mst_clk", "delay.tempo", 1),
    ("delay.effects_variation", "delay.effects", 0),
    ("delay.ping_pong", "delay.filter", 0),
    ("delay.analog", "delay.tap_set", 1),
    ("amp.variation", "amp.model", 0),
    ("comp.global", "comp.on", 0),
    ("comp.fast", "comp.amount", 1),
    ("reverb.global", "reverb.on", 0),
    ("reverb.variation", "reverb.type", 0),
];

/// Where control `id` is drawn, in screen pixels (`origin` is the panel's top-left corner).
pub(super) fn draw_rect(id: &str, origin: Pos2) -> Rect {
    if let Some(&(_, base, line)) = LEGENDS.iter().find(|(l, _, _)| *l == id) {
        let b = rect_or_empty(base, origin);
        let c = pos2(b.center().x, b.bottom() + 5.0 + 7.5 * line as f32);
        return Rect::from_center_size(c, vec2(b.width().max(26.0), 7.0));
    }
    match id {
        // The model button, LP24/HP24 and TO ROTARY are LED choices behind one hardware button
        // (layout `SHARED`); the two extra buttons sit above it, clear of the LED row.
        "amp.filter" | "amp.to_rotary" => {
            let m = rect_or_empty("amp.model", origin);
            let half = m.width() * 0.5;
            let x = if id == "amp.filter" {
                m.left()
            } else {
                m.left() + half
            };
            Rect::from_min_size(pos2(x, m.top() - 15.0), vec2(half - 1.0, 7.0))
        }
        // RES is printed under the FREQ knob.
        "amp.res" => {
            let f = rect_or_empty("amp.mid_freq", origin);
            Rect::from_center_size(pos2(f.center().x, f.bottom() + 8.0), vec2(10.0, 10.0))
        }
        _ => rect_or_empty(id, origin),
    }
}

/// Why Ring Mod and A-WAH cannot follow the Master Clock.
pub(super) const NO_CLOCK: &str =
    "the engine has no Master Clock for Ring Mod (a pitch) or A-WAH (envelope driven)";

/// Why control `id` cannot be honoured by the engine right now (`None`: fully functional).
///
/// Every Mod type, the Reverb VARIATION and the Amp VARIATION (a separate cabinet voicing per
/// model, `fx/amp_eq.rs`) are in the engine; what is left dimmed is TO ROTARY on the Organ slot
/// and the Mod 1 MST CLK of Ring Mod and A-WAH.
pub(super) fn dim_reason(id: &str, c: &ChainUi, slot: usize) -> Option<&'static str> {
    match id {
        "amp.to_rotary" if slot == 0 => {
            Some("Organ layers feed the Rotary Speaker directly; TO ROTARY routes Piano and Synth layers")
        }
        "mod1.mst_clk" if !c.mod1_has_clock() => Some(NO_CLOCK),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::{all_controls, Section};
    use std::collections::BTreeSet;

    #[test]
    fn handled_ids_are_exactly_the_inventory_effects_ids() {
        let inv: BTreeSet<String> = all_controls()
            .into_iter()
            .filter(|c| c.section == Section::Effects)
            .map(|c| c.id)
            .collect();
        let mine: BTreeSet<String> = HANDLED.iter().map(|s| s.to_string()).collect();
        assert_eq!(HANDLED.len(), mine.len(), "duplicate id in HANDLED");
        assert_eq!(inv, mine);
    }

    #[test]
    fn every_id_has_a_non_empty_draw_rect() {
        let o = pos2(0.0, 0.0);
        for id in HANDLED {
            let r = draw_rect(id, o);
            assert!(
                r.width() > 1.0 && r.height() > 1.0,
                "{id} has an empty rect {r:?}"
            );
            assert!(
                r.min.x >= 0.0 && r.min.y >= 0.0,
                "{id} is off the panel: {r:?}"
            );
        }
    }

    #[test]
    fn shifted_controls_have_their_own_legend_rect_below_the_base() {
        let o = pos2(0.0, 0.0);
        for (id, base, _) in LEGENDS {
            let b = rect_or_empty(base, o);
            let l = draw_rect(id, o);
            assert!(l.top() >= b.bottom(), "{id} legend overlaps {base}");
            assert!(l.height() < b.height() + 1.0);
        }
    }

    #[test]
    fn amp_companions_do_not_overlap_each_other_or_the_model_button() {
        let o = pos2(0.0, 0.0);
        let f = draw_rect("amp.filter", o);
        let r = draw_rect("amp.to_rotary", o);
        let m = draw_rect("amp.model", o);
        assert!(!f.intersects(r) && !f.intersects(m) && !r.intersects(m));
    }

    #[test]
    fn dimming_rules() {
        let mut c = ChainUi::default();
        assert!(dim_reason("amp.to_rotary", &c, 0).is_some());
        assert!(dim_reason("amp.to_rotary", &c, 1).is_none());
        for sel in [0, 1, 4] {
            c.rev_sel = sel;
            assert!(
                dim_reason("reverb.variation", &c, 1).is_none(),
                "VARIATION (left) and CHORALE (right) both reach the engine"
            );
        }
        for slot in [0, 1] {
            assert!(
                dim_reason("amp.variation", &c, slot).is_none(),
                "the Amp VARIATION has its own cabinet voicing in the engine"
            );
        }
        c.mod1_sel = 2; // RM
        assert!(dim_reason("mod1.mst_clk", &c, 1).is_some());
        c.mod1_sel = 5; // PUMP
        assert!(dim_reason("mod1.mst_clk", &c, 1).is_none());
        assert!(dim_reason("mod1.rate", &c, 1).is_none());
    }
}
