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
    ("synth.lfo.rate", 0.0, 12.0),
    ("synth.lfo.rate_mst_clk", 0.0, 12.0),
    // The FREQ name collided with the "GROUP" Shift legend of the filter TYPE button above.
    ("synth.filter.freq", 0.0, 8.0),
    // The two KB ZONE legends under OCTAVE / SHIFT printed into the zone LEDs.
    ("synth.zone_leds", 0.0, 9.0),
    // The OSC CTRL name printed into the ENV TO PITCH legend of the PITCH/SMP button above.
    ("synth.osc.ctrl", 0.0, 5.0),
    // "ENV AMT" / "RES" touched the VELOCITY legend of the ENVELOPE button above.
    ("synth.osc.env_amt", 0.0, 4.0),
    ("synth.filter.res", 0.0, 4.0),
    // The PROG VIEW name printed into the PANIC legend of the TRANSP button above.
    ("program.prog_view", 0.0, 5.0),
    ("program.preset_name", 0.0, 5.0),
    // Program buttons 5-8: their numbers touched the legends (System, Sound, ...) of buttons 1-4.
    ("program.button.5", 0.0, 3.0),
    ("program.button.6", 0.0, 3.0),
    ("program.button.7", 0.0, 3.0),
    ("program.button.8", 0.0, 3.0),
    ("program.menu.output", 0.0, 3.0),
    ("program.menu.pedal", 0.0, 3.0),
    ("program.menu.midi", 0.0, 3.0),
    ("program.menu.extern", 0.0, 3.0),
    // The MON/COPY name sat 1.5 px under the LAYER INIT legend of SECT EDIT.
    ("program.mon_copy", 0.0, 3.0),
    ("program.paste", 0.0, 3.0),
    // The piano ZONE -/+ legends (now clear of the OCT bezels) printed into the zone LEDs.
    ("piano.zone_leds", 0.0, 3.0),
    // The SPLIT name touched the L M H LED strip above its button.
    ("split.leds", 0.0, -2.0),
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
