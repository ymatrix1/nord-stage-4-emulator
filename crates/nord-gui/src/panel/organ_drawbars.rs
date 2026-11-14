//! Organ drawbars (manual p.19-21): nine drawbars with their 8-LED ladders.
//!
//! The ladder shows the stored preset value and is unlit in Drawbar Live (manual p.21). The
//! sliders are the physical drawbars: in Preset mode they edit the preset, in Drawbar Live only
//! the physical positions (`OrganEdit::LiveDrawbars`).

use super::layout_px::rect_or_empty;
use super::organ_widgets::{drawbar_ladder, drawbar_slot};
use super::prelude::*;
use crate::audio::OrganEdit;

const SLOT_IDS: [&str; 9] = [
    "organ.drawbar.1",
    "organ.drawbar.2",
    "organ.drawbar.3",
    "organ.drawbar.4",
    "organ.drawbar.5",
    "organ.drawbar.6",
    "organ.drawbar.7",
    "organ.drawbar.8",
    "organ.drawbar.9",
];

const LADDER_IDS: [&str; 9] = [
    "organ.drawbar_ladder.1",
    "organ.drawbar_ladder.2",
    "organ.drawbar_ladder.3",
    "organ.drawbar_ladder.4",
    "organ.drawbar_ladder.5",
    "organ.drawbar_ladder.6",
    "organ.drawbar_ladder.7",
    "organ.drawbar_ladder.8",
    "organ.drawbar_ladder.9",
];

/// Distance (px) from the slot end to the footage number: clears the cap at the pushed-out end
/// (half cap 4.5 px plus its shadow, audit rule `F`) and leaves the glyphs a 2 px margin.
const FOOTAGE_DROP: f32 = 13.0;

/// Footages 16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1'.
const FOOTAGES: [&str; 9] = [
    "16'", "5 1/3'", "8'", "4'", "2 2/3'", "2'", "1 3/5'", "1 1/3'", "1'",
];

/// Register names printed over the ladders (manual p.19: the original register of each drawbar).
const NAMES: [&str; 9] = [
    "BASS16", "STR16", "FLUTE8", "OBOE8", "TRMP8", "STR8", "FLUTE4", "STR4", "2 2/3",
];

/// Distance (px) from the ladder top to the centre of its register name: clears the control
/// margin of the legibility audit (2.5 px) and the half height of the text.
const NAME_RISE: f32 = 7.0;
/// Distance (px) of a ladder numeral from the LED column: past the LED bezel (5.2 px at the
/// ladder pitch) and inside the ladder rectangle, so the numeral belongs to the ladder.
const NUMERAL_DX: f32 = 9.2;

/// Register name over the ladder and the numerals 1-8 beside its LEDs (photo
/// `NS4_HA88_Organ-01`: numbers beside each ladder, the name above it).
fn ladder_labels(p: &Painter, ladder: Rect, name: &str) {
    let ink = Color32::from_gray(215);
    label(
        p,
        pos2(ladder.center().x, ladder.top() - NAME_RISE),
        name,
        7.5,
        ink,
        Align2::CENTER_CENTER,
    );
    let pitch = ladder.height() / 8.0;
    for n in 1..=8 {
        label(
            p,
            pos2(
                ladder.center().x + NUMERAL_DX,
                ladder.top() + (n as f32 - 0.5) * pitch,
            ),
            &n.to_string(),
            7.5,
            ink,
            Align2::CENTER_CENTER,
        );
    }
}

/// Drawbars 3, 4, 6 and 9 (8', 4', 2', 1') have white caps, the others black.
fn is_white(i: usize) -> bool {
    matches!(i, 2 | 3 | 5 | 8)
}

/// LEDs lit on a ladder: the stored value, none in Drawbar Live.
pub(super) fn ladder_value(value: u8, live: bool) -> u8 {
    if live {
        0
    } else {
        value.min(8)
    }
}

pub(super) fn render_drawbars(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let live = state.organ_ui.drawbar_live;
    if !live {
        // Preset mode: the physical drawbars follow the stored preset (program loads, SYNC).
        state.organ_ui.live_bars = state.organ_drawbars;
    }
    let mut changed = false;
    for i in 0..9 {
        let slot = rect_or_empty(SLOT_IDS[i], origin);
        if drawbar_slot(
            ui,
            slot,
            SLOT_IDS[i],
            &mut state.organ_ui.live_bars[i],
            is_white(i),
        ) {
            changed = true;
        }
        let shown = state.telemetry.drawbar(false, i, state.organ_drawbars[i]);
        let lit = ladder_value(shown, live);
        let ladder = rect_or_empty(LADDER_IDS[i], origin);
        drawbar_ladder(ui, ladder, LADDER_IDS[i], lit);
        ladder_labels(painter, ladder, NAMES[i]);
        label(
            painter,
            pos2(slot.center().x + 4.0, slot.bottom() + FOOTAGE_DROP),
            FOOTAGES[i],
            5.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }
    if changed {
        if !live {
            state.organ_drawbars = state.organ_ui.live_bars;
            audio.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));
        }
        audio.send_command(AudioCommand::Organ(OrganEdit::LiveDrawbars(
            state.organ_ui.live_bars,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn drawbars_paint_shapes() {
        let n = shapes_painted(|ui, p, rect, state, audio| {
            render_drawbars(ui, p, rect.min, state, audio);
        });
        assert!(n > 100, "drawbars painted only {n} shapes");
    }

    #[test]
    fn ladder_is_unlit_in_drawbar_live() {
        assert_eq!(ladder_value(5, false), 5);
        assert_eq!(ladder_value(5, true), 0);
        assert_eq!(ladder_value(200, false), 8);
    }

    #[test]
    fn registers_are_named_like_the_manual_and_footages_match() {
        assert_eq!(NAMES.len(), FOOTAGES.len());
        assert_eq!(NAMES[0], "BASS16");
        assert_eq!(NAMES[8], "2 2/3");
    }

    #[test]
    fn every_ladder_prints_its_name_and_eight_numerals() {
        let (labs, _) = crate::panel::legibility_tests::labels_of(None, egui::vec2(3300.0, 900.0));
        for name in NAMES {
            assert!(labs.iter().any(|l| l.text == name), "{name}");
        }
        for n in 1..=8 {
            let count = labs.iter().filter(|l| l.text == n.to_string()).count();
            assert!(count >= 9, "numeral {n}: {count}");
        }
    }

    #[test]
    fn cap_colours_follow_the_footage() {
        let white: Vec<_> = (0..9).filter(|&i| is_white(i)).collect();
        assert_eq!(white, vec![2, 3, 5, 8]);
    }
}
