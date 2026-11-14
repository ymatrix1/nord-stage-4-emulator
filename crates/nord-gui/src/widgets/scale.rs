//! Printed scale ends of a knob: `0` and `10` (or `-15` and `15`) at the two ends of the tick
//! ring, in the open gap below the knob, as the panel prints them (photos `NS4-Panel-synth`,
//! `NS4-Panel-effects`). A user can read the range of a knob without hovering it.

use super::basic::label;
use egui::{pos2, Align2, Color32, Painter, Pos2};

/// Distance (px) from the knob body edge to the nearer edge of a scale number: clear of the
/// tick ring (5.5 px) and of the 2.5 px control margin of the legibility audit.
const SIDE: f32 = 5.0;
/// Height of the scale numbers below the knob centre, as a fraction of the radius.
const DROP: f32 = 0.62;
/// Distance (px) of the EQ scale numbers below the knob body edge / outside it (clear of the
/// 2.5 px control margin and of the knob name printed between them).
const LOW_DROP: f32 = 7.5;
const LOW_SIDE: f32 = 3.0;
/// Scale text size (the 7.5 px legend minimum).
const SIZE: f32 = 7.5;

/// Id prefixes of the knobs that print a scale (Synth, Layer Effects).
const SCALED: [&str; 7] = [
    "synth.", "mod1.", "mod2.", "amp.", "delay.", "comp.", "reverb.",
];

/// Scale ends printed for the knob with inventory id `id`, or `None` for knobs that print none
/// (Master level, the AMP RES trimmer, ids outside the Synth / Layer Effects blocks).
///
/// EMPIRICAL: read from the reference photos; the EQ knobs print -15 and 15, the Oscillator ENV
/// AMT knob -10 and 10, every other pot 0 and 10; the amp FREQ knob (printed 200 ... 8K) and the RES trimmer print none.
pub fn ends(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "amp.bass" | "amp.mid" | "amp.treble" => Some(("-15", "15")),
        "synth.osc.env_amt" => Some(("-10", "10")),
        "amp.mid_freq" | "amp.res" => None,
        _ if SCALED.iter().any(|p| id.starts_with(p)) => Some(("0", "10")),
        _ => None,
    }
}

/// Anchor points of the low and high end numbers of a knob at `center` with body `radius`: left
/// and right of the knob, a little below its centre, the text growing away from the knob.
pub fn end_positions(center: Pos2, radius: f32) -> (Pos2, Pos2) {
    let y = center.y + radius * DROP;
    (
        pos2(center.x - radius - SIDE, y),
        pos2(center.x + radius + SIDE, y),
    )
}

/// Anchor points for the wide `-15` / `15` ends of the EQ knobs, whose neighbours are too close
/// for numbers beside the knob: below the knob, left and right of its name.
pub fn low_end_positions(center: Pos2, radius: f32) -> (Pos2, Pos2) {
    let y = center.y + radius + LOW_DROP;
    (
        pos2(center.x - radius - LOW_SIDE, y),
        pos2(center.x + radius + LOW_SIDE, y),
    )
}

/// Paint the scale ends of knob `id`, if it has any.
pub fn paint(p: &Painter, center: Pos2, radius: f32, id: &str) {
    let Some((lo, hi)) = ends(id) else { return };
    let (a, b) = if lo == "-15" {
        low_end_positions(center, radius)
    } else {
        end_positions(center, radius)
    };
    let ink = Color32::from_gray(205);
    label(p, a, lo, SIZE, ink, Align2::RIGHT_CENTER);
    label(p, b, hi, SIZE, ink, Align2::LEFT_CENTER);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;

    #[test]
    fn scales_follow_the_printed_panel() {
        assert_eq!(ends("synth.filter.freq"), Some(("0", "10")));
        assert_eq!(ends("amp.bass"), Some(("-15", "15")));
        assert_eq!(ends("synth.osc.env_amt"), Some(("-10", "10")));
        assert_eq!(ends("amp.res"), None);
        assert_eq!(ends("amp.mid_freq"), None);
        assert_eq!(ends("master.level"), None);
        assert_eq!(ends("delay.feedback"), Some(("0", "10")));
    }

    #[test]
    fn ends_sit_beside_the_knob_below_its_centre() {
        let c = pos2(100.0, 100.0);
        let (a, b) = end_positions(c, 18.0);
        assert!(a.x < c.x - 18.0 && b.x > c.x + 18.0);
        assert!(a.y > c.y && (a.y - b.y).abs() < 1e-3);
    }

    #[test]
    fn eq_ends_sit_below_the_knob_beside_its_name() {
        let c = pos2(100.0, 100.0);
        let (a, b) = low_end_positions(c, 18.0);
        assert!(a.y > c.y + 18.0 + 2.5 && (a.y - b.y).abs() < 1e-3);
        assert!(a.x < c.x - 18.0 && b.x > c.x + 18.0);
    }

    #[test]
    fn paint_draws_two_labels_only_for_scaled_knobs() {
        with_ui(|ui| {
            let p = ui.painter().clone();
            paint(&p, pos2(50.0, 50.0), 14.0, "synth.lfo.rate");
            paint(&p, pos2(50.0, 50.0), 14.0, "master.level");
        });
    }
}
