//! LED names the Synth panel prints above the MODE and VOICE buttons (photo `NS4-Panel-synth`):
//! SAMPLES / ANALOG over the MODE button, MONO / LEGATO over the VOICE button. The button still
//! shows its value; the LEDs lit here follow the same state, so a user reads the choice from the
//! printed names as on the instrument.

use super::widgets::{dot, tiny, LEGEND};
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::SynthUi;
use egui::Pos2;

/// Pitch (px) of the rows: the 7.5 px minimum font plus a 2 px gap (audit rule `V`).
pub(super) const ROW: f32 = 9.5;
/// Distance (px) from the top of the button to the centre of its lowest row.
const BOTTOM_ROW: f32 = 8.5;
/// Distance (px) from the left edge of the button to the LED centre.
const LED_DX: f32 = 2.5;
/// Space (px) between the LED centre and the start of its name.
const NAME_DX: f32 = 6.5;

/// One printed row: name and whether its LED is lit.
type Row = (&'static str, bool);

/// Rows of the MODE box: SAMPLES above ANALOG (the Mode button is on for samples).
pub(super) fn mode_rows(sui: &SynthUi) -> [Row; 2] {
    let smp = sui.layer().sample_mode;
    [("SAMPLES", smp), ("ANALOG", !smp)]
}

/// Rows of the VOICE box: MONO above LEGATO (both dark means POLY).
pub(super) fn voice_rows(sui: &SynthUi) -> [Row; 2] {
    let v = sui.layer().voice;
    [("MONO", v == 1), ("LEGATO", v == 2)]
}

/// Centre of the LED of row `i` of `n` above a button whose top-left corner is `top_left`.
pub(super) fn led_pos(top_left: Pos2, i: usize, n: usize) -> Pos2 {
    pos2(
        top_left.x + LED_DX,
        top_left.y - BOTTOM_ROW - ROW * (n - 1 - i) as f32,
    )
}

fn paint_rows(p: &Painter, button: Rect, rows: &[Row]) {
    for (i, (name, lit)) in rows.iter().enumerate() {
        let c = led_pos(button.min, i, rows.len());
        dot(p, c, *lit, LED_RED, LED_RED_DIM);
        let ink = if *lit { Color32::WHITE } else { LEGEND };
        tiny(
            p,
            pos2(c.x + NAME_DX, c.y),
            name,
            ink,
            Align2::LEFT_CENTER,
        );
    }
}

/// Paint the SAMPLES / ANALOG and MONO / LEGATO rows.
pub(super) fn paint_names(p: &Painter, o: Pos2, sui: &SynthUi) {
    paint_rows(p, rect_or_empty("synth.mode", o), &mode_rows(sui));
    paint_rows(p, rect_or_empty("synth.voice.mode", o), &voice_rows(sui));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn rows_follow_the_layer_state() {
        let mut s = SynthUi::new();
        assert_eq!(mode_rows(&s), [("SAMPLES", false), ("ANALOG", true)]);
        assert_eq!(voice_rows(&s), [("MONO", false), ("LEGATO", false)]);
        s.layer_mut().sample_mode = true;
        s.layer_mut().voice = 2;
        assert_eq!(mode_rows(&s), [("SAMPLES", true), ("ANALOG", false)]);
        assert_eq!(voice_rows(&s), [("MONO", false), ("LEGATO", true)]);
        s.layer_mut().voice = 1;
        assert!(voice_rows(&s)[0].1 && !voice_rows(&s)[1].1);
    }

    #[test]
    fn rows_stack_upwards_from_the_button() {
        let tl = pos2(50.0, 100.0);
        let (a, b) = (led_pos(tl, 0, 2), led_pos(tl, 1, 2));
        assert!((b.y - (100.0 - BOTTOM_ROW)).abs() < 1e-4);
        assert!((b.y - a.y - ROW).abs() < 1e-4);
        assert!(a.x > tl.x);
    }

    #[test]
    fn names_paint_four_leds_and_four_texts() {
        let sui = SynthUi::new();
        let n = shapes_painted(|_ui, p, rect, _s, _a| paint_names(p, rect.min, &sui));
        // Two shapes per LED plus one text each.
        assert_eq!(n, 4 * 3);
    }
}
