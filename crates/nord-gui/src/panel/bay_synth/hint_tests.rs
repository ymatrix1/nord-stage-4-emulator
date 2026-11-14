//! Tests of the parameter hint wiring of the Synth pots and the LFO DEST pad.

use super::widgets::pot_hint;
use crate::panel::param_hint::{self, table::LFO_DEST_HINT};
use crate::panel::ui_state::{Knob, SynthUi};

fn hint_after(f: impl FnOnce(&egui::Context)) -> Option<String> {
    let ctx = egui::Context::default();
    let mut out = None;
    let mut f = Some(f);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        if let Some(f) = f.take() {
            f(ctx);
            out = param_hint::active(ctx);
        }
    });
    out
}

#[test]
fn pot_hint_shows_the_stored_layer_value_in_panel_units() {
    let mut sui = SynthUi::new();
    sui.knob_input(Knob::LfoModAmt, 0.0);
    sui.knob_input(Knob::LfoModAmt, 0.71);
    let h = hint_after(|ctx| pot_hint(ctx, &sui, Knob::LfoModAmt));
    assert_eq!(h.as_deref(), Some("LFO Amount 7.1"));
}

#[test]
fn synced_rate_pot_hint_names_the_clock_division() {
    let mut sui = SynthUi::new();
    sui.rate_clock_sync(Knob::LfoRate, true);
    let h = hint_after(|ctx| pot_hint(ctx, &sui, Knob::LfoRate)).expect("hint");
    assert!(h.starts_with("LFO Rate 1/"), "{h}");
}

#[test]
fn free_lfo_rate_pot_hint_is_in_hz() {
    let sui = SynthUi::new();
    let h = hint_after(|ctx| pot_hint(ctx, &sui, Knob::LfoRate)).expect("hint");
    assert!(h.starts_with("LFO Rate ") && h.ends_with(" Hz"), "{h}");
}

#[test]
fn lfo_dest_hint_words_follow_the_destination_table() {
    assert_eq!(LFO_DEST_HINT.len(), crate::panel::ui_state::LFO_DESTS.len());
    let h = hint_after(|ctx| param_hint::touch_value(ctx, "synth.lfo.dest", LFO_DEST_HINT[1]));
    assert_eq!(h.as_deref(), Some("LFO Destination Osc Pitch"));
}
