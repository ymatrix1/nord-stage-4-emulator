//! Per-control position nudges applied on top of the `nord_panel::layout` tables.
//!
//! The layout rectangles are `ASSUMED` (+-3 px estimated from one 2000 px photo), so they are
//! not edited. Where two neighbours print their text into each other at the panel scale, the
//! control is nudged here instead, by `(dx, dy)` logical px, with the reason next to the entry.
//! `layout_px::rect_of` applies the table; `legibility_tests` proves the result is collision free.

use egui::{vec2, Vec2};

/// `(control id, dx, dy)`: shift of the control's rectangle in logical px.
pub const TWEAKS: &[(&str, f32, f32)] = &[
    // The LFO RATE name collided with the "GROUP" Shift legend of the LFO waveform button above.
    ("synth.lfo.rate", 0.0, 8.0),
    ("synth.lfo.rate_mst_clk", 0.0, 8.0),
    // The FREQ name collided with the "GROUP" Shift legend of the filter TYPE button above.
    ("synth.filter.freq", 0.0, 8.0),
    // The two KB ZONE legends under OCTAVE / SHIFT printed into the zone LEDs.
    ("synth.zone_leds", 0.0, 9.0),
    // "DECAY FAST" and "HARMONIC 3RD" touched; the third button moves right.
    ("organ.perc_third", 12.0, 0.0),
];

/// Nudge of control `id` (zero when it has none).
pub fn nudge(id: &str) -> Vec2 {
    TWEAKS
        .iter()
        .find(|(i, _, _)| *i == id)
        .map_or(Vec2::ZERO, |(_, dx, dy)| vec2(*dx, *dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_have_no_nudge_and_known_ones_do() {
        assert_eq!(nudge("no.such.control"), Vec2::ZERO);
        let (id, dx, dy) = TWEAKS[0];
        assert_eq!(nudge(id), vec2(dx, dy));
    }

    #[test]
    fn every_tweaked_id_is_a_known_control() {
        for (id, _, _) in TWEAKS {
            assert!(
                nord_panel::layout::rect_for(id).is_some(),
                "{id} has no layout rect"
            );
        }
    }

    #[test]
    fn nudges_are_small() {
        for (id, dx, dy) in TWEAKS {
            assert!(dx.abs() <= 40.0 && dy.abs() <= 40.0, "{id}");
        }
    }
}
