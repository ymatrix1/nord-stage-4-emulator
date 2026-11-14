//! Aux KB LED of the Organ and Piano strips (manual p.43-45).
//!
//! The LED shows whether a layer is played by the aux keyboard. The panel keeps the flag in its UI
//! state and the LED doubles as a click target that sends the per-layer Aux KB edit; the real
//! read-back (which layers the engine currently routes to the aux keyboard) arrives with the
//! telemetry stream, which is why every reader goes through [`aux_flag_from_telemetry`].

use super::ctl::led_small;
use super::prelude::*;

/// The flag to display for one layer. Today the UI value; the telemetry PR replaces the body with
/// the engine's `AuxKb::is_aux_layer` read-back.
pub(super) fn aux_flag_from_telemetry(ui_value: bool) -> bool {
    ui_value
}

/// Draw one Aux KB LED in `rect`; returns `true` when it was clicked.
pub(super) fn aux_led(ui: &mut Ui, rect: Rect, id: &str, lit: bool, tip: &str) -> bool {
    let r = (rect.width().min(rect.height()) * 0.5).clamp(1.2, 2.4);
    led_small(
        &ui.painter().clone(),
        rect.center(),
        lit,
        LED_GREEN,
        LED_GREEN_DIM,
        r,
    );
    let resp = ui.interact(rect.expand(2.0), ui.id().with((id, "aux")), Sense::click());
    let clicked = resp.clicked();
    resp.on_hover_text(tip);
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn the_flag_is_the_ui_value_until_telemetry_exists() {
        assert!(aux_flag_from_telemetry(true));
        assert!(!aux_flag_from_telemetry(false));
    }

    #[test]
    fn a_lit_and_an_unlit_led_both_paint() {
        let n = shapes_painted(|ui, _p, rect, _s, _a| {
            let r = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(6.0, 6.0));
            assert!(!aux_led(ui, r, "x.aux", true, "tip"));
            assert!(!aux_led(
                ui,
                r.translate(vec2(12.0, 0.0)),
                "y.aux",
                false,
                "tip"
            ));
        });
        assert!(n >= 4, "painted only {n} shapes");
    }
}
