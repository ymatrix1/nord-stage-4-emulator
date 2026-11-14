//! Graphics of the OLED parameter popups: the four beat boxes of the Master Clock and the slider
//! of Octave Shift (128 x 64 grid, scaled to the display rectangle).
//!
//! VERIFIED (ARM frames, research note s17.4): the beat boxes sit at the right edge of the label
//! row (y = 45) with the first one filled; the slider sits at the right edge of the value row
//! (y = 55), 13 glyph cells wide, the marker left of the centre for `-1`. EMPIRICAL: box size,
//! pitch and the slider ticks are read off those frames to within a pixel (the glyph bitmaps
//! themselves are not shipped); ASSUMED: the marker moves three cells per octave (range -2 to +2).

use super::oled_grid::bar;
use crate::panel::prelude::*;

/// A graphic drawn at the right edge of a popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graphic {
    /// Four beat boxes; the index (`0..4`) of the lit one.
    Beats(u8),
    /// Octave slider; the octave shift (`-2..=2`).
    Slider(i8),
}

/// Cells of the slider.
const CELLS: usize = 13;

/// Slider cell (`0..13`) of an octave shift: the centre cell is 0, three cells per octave.
pub fn slider_cell(octave: i8) -> usize {
    (6 + 3 * i32::from(octave)).clamp(0, CELLS as i32 - 1) as usize
}

/// The four beat boxes (7 px squares, 9 px pitch, right edge at x = 126) of display `rect`.
pub fn beat_boxes(rect: Rect) -> [Rect; 4] {
    let k = rect.height() / 64.0;
    let top = rect.top() + bar(rect.height()).popup_y[0];
    std::array::from_fn(|i| {
        let left = rect.left() + (92.0 + 9.0 * i as f32) * k;
        Rect::from_min_size(pos2(left, top), vec2(7.0 * k, 7.0 * k))
    })
}

/// Left edge, cell width and baseline of the slider on display `rect`.
fn slider_frame(rect: Rect) -> (f32, f32, f32) {
    let k = rect.height() / 64.0;
    let y = rect.top() + bar(rect.height()).popup_y[1] + 4.0 * k;
    (rect.left() + 74.0 * k, 4.0 * k, y)
}

/// The marker of the slider for `octave`: a 3 px wide, 7 px tall block on the cell.
pub fn slider_marker(rect: Rect, octave: i8) -> Rect {
    let (x0, w, y) = slider_frame(rect);
    let k = rect.height() / 64.0;
    let cx = x0 + (slider_cell(octave) as f32 + 0.5) * w;
    Rect::from_center_size(pos2(cx, y), vec2(3.0 * k, 7.0 * k))
}

/// Draw `g` on display `rect` in `fg`.
pub fn paint(painter: &Painter, rect: Rect, g: Graphic, fg: Color32) {
    let k = rect.height() / 64.0;
    let stroke = Stroke::new(0.7_f32.max(0.8 * k), fg);
    match g {
        Graphic::Beats(lit) => {
            for (i, r) in beat_boxes(rect).into_iter().enumerate() {
                if i == usize::from(lit) {
                    painter.rect_filled(r, 0.0, fg);
                } else {
                    painter.rect_stroke(r, 0.0, stroke, StrokeKind::Inside);
                }
            }
        }
        Graphic::Slider(octave) => {
            let (x0, w, y) = slider_frame(rect);
            painter.line_segment([pos2(x0, y), pos2(x0 + w * CELLS as f32, y)], stroke);
            for tick in (0..CELLS).step_by(3) {
                let x = x0 + (tick as f32 + 0.5) * w;
                painter.line_segment([pos2(x, y - 2.5 * k), pos2(x, y + 2.5 * k)], stroke);
            }
            painter.rect_filled(slider_marker(rect, octave), 0.0, fg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    fn disp() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(128.0, 64.0))
    }

    #[test]
    fn the_slider_marker_sits_left_of_the_centre_for_minus_one() {
        assert_eq!(slider_cell(0), 6);
        assert!(slider_cell(-1) < 6 && slider_cell(1) > 6);
        assert_eq!((slider_cell(-9), slider_cell(9)), (0, 12));
        let (m_minus, m_zero) = (slider_marker(disp(), -1), slider_marker(disp(), 0));
        assert!(m_minus.center().x < m_zero.center().x);
        assert!(
            m_zero.right() <= 126.0 && m_minus.left() >= 70.0,
            "right edge, 13 cells"
        );
    }

    #[test]
    fn the_beat_boxes_sit_in_the_label_row_at_the_right_edge() {
        let boxes = beat_boxes(disp());
        assert_eq!(boxes[0].top(), 45.0);
        assert_eq!(boxes[3].right(), 126.0);
        assert!(boxes.windows(2).all(|w| w[0].right() < w[1].left()));
    }

    #[test]
    fn every_graphic_paints_shapes_and_a_lit_box_replaces_an_outline() {
        let paint_it =
            |g| shapes_painted(move |_ui, p, _rect, _s, _a| paint(p, disp(), g, Color32::WHITE));
        assert_eq!(paint_it(Graphic::Beats(0)), paint_it(Graphic::Beats(3)));
        assert!(paint_it(Graphic::Beats(1)) >= 4);
        assert!(paint_it(Graphic::Slider(-1)) >= 4);
    }
}
