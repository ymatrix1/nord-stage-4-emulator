//! Piano bay: the printed names next to the PIANO SELECT type LEDs and the TIMBRE LED lists.
//!
//! The real panel prints the six type names in two columns beside a 2 x 3 LED block, and lists
//! the timbres BRIGHT, MID, SOFT from top to bottom with DYNO 1 / DYNO 2 in a second column
//! (photo `NS4_HA88_Piano-01_241008.jpg`). Both blocks are drawn here, so a user can find the
//! type or timbre without hovering.

use super::layout_px::rect_or_empty;
use super::piano_logic as logic;
use super::piano_widgets as w;
use super::prelude::*;
use super::ui_state::slot;
use crate::audio::PianoEdit;

/// Type names (printed order) per LED row: `[left, right]` indices into
/// [`super::bay_piano_select::TYPE_NAMES`]. EMPIRICAL: read from the photo, left column
/// ELECTRIC, UPRIGHT, GRAND, right column CLAV, DIGITAL, MISC (top to bottom); checked by
/// `type_grid_covers_every_type_once` and the legibility audit (every name printed and unclipped).
pub(super) const TYPE_GRID: [[usize; 2]; 3] = [[2, 3], [1, 4], [0, 5]];

/// Row pitch (px) of the type names and timbre lists: the 7.5 px font plus the 2 px of rule `V`.
const ROW: f32 = 9.5;
/// Half the distance (px) between the two LED columns of the type block.
const COL_HALF: f32 = 6.0;
/// Silkscreen text colour.
const INK: u8 = 225;
/// Grey of names that do not apply to the current type (dimmed, still above 4.5:1 contrast).
const INK_OFF: u8 = 170;

/// 2 x 3 LED block with the type names beside it; `lit` is the selected type (index into
/// `TYPE_NAMES`). `rect` is the layout rectangle of the type LEDs; the block is centred on it.
pub(super) fn type_grid(p: &Painter, rect: Rect, lit: usize) {
    let c = rect.center();
    for (row, pair) in TYPE_GRID.iter().enumerate() {
        let y = c.y + (row as f32 - 1.0) * ROW;
        for (col, &ty) in pair.iter().enumerate() {
            let sign = if col == 0 { -1.0 } else { 1.0 };
            let x = c.x + sign * COL_HALF;
            w::mini_led(p, pos2(x, y), ty == lit, LED_RED, LED_RED_DIM, 2.2, false);
            label(
                p,
                pos2(x + sign * 6.5, y),
                &super::bay_piano_select::TYPE_NAMES[ty].to_uppercase(),
                5.5,
                Color32::from_gray(INK),
                if col == 0 {
                    Align2::RIGHT_CENTER
                } else {
                    Align2::LEFT_CENTER
                },
            );
        }
    }
}

/// Names and lit mask of the TIMBRE LEDs, top to bottom: `(names, lit bits by name index)`.
/// Acoustic and electric types print BRIGHT, MID, SOFT (bit 0 = Soft); the Clav prints its three
/// pick-up EQ names in the same direction (ASSUMED: the photo only shows the non-Clav list).
pub(super) fn timbre_rows(clav: bool, idx: usize, clav_bits: u8) -> ([&'static str; 3], u8) {
    if clav {
        (["BRIL", "TREBLE", "SOFT"], clav_bits)
    } else {
        (
            ["BRIGHT", "MID", "SOFT"],
            if idx == 0 { 0 } else { 1 << (idx - 1) },
        )
    }
}

/// Index of the name printed in `row` (0 = top) in a list whose bit 0 is the last row.
fn bit_of_row(row: usize) -> usize {
    2 - row
}

pub(super) fn draw_timbre(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    out: &mut Vec<PianoEdit>,
) {
    let clav = state.piano_cat == PianoCategory::Clav;
    let rect = rect_or_empty("piano.timbre", o);
    let pr = w::press(
        ui,
        rect,
        "piano_timbre",
        &mut state.piano_ui.trackers[slot::TIMBRE],
    );
    if pr.gesture.is_some() {
        logic::timbre_cycle(state, out);
    }
    w::face(p, rect, pr.down, clav);
    w::inside(p, rect, "TIMB", clav);

    let electric = state.piano_cat == PianoCategory::Electric;
    let (names, lit) = timbre_rows(clav, logic::timbre_index(state), state.piano_ui.clav_timbre);
    let top = rect.center().y - ROW * 1.0;
    let x0 = rect.right() + 7.0;
    let mut col2_x = x0;
    for (row, name) in names.iter().enumerate() {
        let c = pos2(x0, top + row as f32 * ROW);
        let on = lit & (1 << bit_of_row(row)) != 0;
        w::mini_led(p, c, on, LED_RED, LED_RED_DIM, 2.2, clav);
        let shown = label(
            p,
            c + vec2(5.0, 0.0),
            name,
            5.5,
            Color32::from_gray(if clav { INK_OFF } else { INK }),
            Align2::LEFT_CENTER,
        );
        col2_x = col2_x.max(shown.right() + 5.0);
    }
    // DYNO 1 / DYNO 2: Electric only, printed (dimmed) on every type like the panel.
    let dyno_lit = if electric {
        logic::timbre_index(state)
    } else {
        0
    };
    for (row, name) in ["DYNO1", "DYNO2"].iter().enumerate() {
        let c = pos2(col2_x, top + row as f32 * ROW);
        let on = electric && dyno_lit == 4 + row;
        w::mini_led(p, c, on, LED_RED, LED_RED_DIM, 2.2, !electric);
        label(
            p,
            c + vec2(5.0, 0.0),
            name,
            5.5,
            Color32::from_gray(if electric { INK } else { INK_OFF }),
            Align2::LEFT_CENTER,
        );
    }
    if clav {
        pr.resp.on_hover_text(
            "TIMBRE on Clav: the seven pick-up EQ combinations (Soft, Treble, Brilliant and their mixes, manual p.26) are kept in the UI; the engine has no Clav timbre yet.",
        );
        state.piano_ui.mark_dim(
            "piano.timbre",
            "TIMBRE (Clav): engine has no Clav pick-up EQ timbres",
        );
    } else {
        pr.resp.on_hover_text(
            "TIMBRE: off, Soft, Mid, Bright; Dyno 1 and 2 on the Electric type only (applies to both layers).",
        );
        state.piano_ui.mark("piano.timbre");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::bay_piano_select::TYPE_NAMES;
    use crate::panel::support::shapes_painted;

    #[test]
    fn type_grid_covers_every_type_once() {
        let mut all: Vec<usize> = TYPE_GRID.iter().flatten().copied().collect();
        all.sort_unstable();
        assert_eq!(all, (0..TYPE_NAMES.len()).collect::<Vec<_>>());
        // Printed order: ELECTRIC / CLAV on top, GRAND / MISC at the bottom.
        assert_eq!(TYPE_NAMES[TYPE_GRID[0][0]], "Electric");
        assert_eq!(TYPE_NAMES[TYPE_GRID[2][1]], "Misc");
    }

    #[test]
    fn timbres_list_bright_mid_soft_top_to_bottom() {
        let (names, lit) = timbre_rows(false, 1, 0);
        assert_eq!(names, ["BRIGHT", "MID", "SOFT"]);
        // Timbre index 1 is Soft: bit 0, the bottom row.
        assert_eq!(lit, 1);
        assert_eq!(bit_of_row(2), 0);
        assert_eq!(bit_of_row(0), 2);
        assert_eq!(timbre_rows(false, 0, 0).1, 0);
        assert_eq!(timbre_rows(true, 0, 0b101).1, 0b101);
    }

    #[test]
    fn type_grid_and_timbre_paint_their_names() {
        let n = shapes_painted(|ui, p, rect, s, _a| {
            let mut out = vec![];
            s.piano_ui.begin_frame();
            type_grid(
                p,
                Rect::from_min_size(rect.min + vec2(50.0, 50.0), vec2(20.0, 30.0)),
                2,
            );
            draw_timbre(ui, p, rect.min, s, &mut out);
        });
        assert!(n > 30, "{n}");
    }
}
