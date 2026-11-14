//! Icon column of the Program home rows: our own 8 x 8 px marks drawn as shapes.
//!
//! VERIFIED (frames of `tools/arm_screens.py`, `docs/research/arm-oled-render.md` section 12): each
//! section row of the real home screen starts with an 8 px icon at x = 0 to 7 and its text at
//! x = 12; a missing sound shows a warning glyph (`0x8A` of the 8 px font) before `Piano not found`.
//! The firmware bitmaps are not copied: these are simple stand-ins (ASSUMED shapes).

use crate::panel::prelude::*;
use egui::{epaint::Shape, Pos2};

/// Organ row icon (in-band marker, like [`super::oled_home::EDITED`]).
pub const ICON_ORGAN: char = '\u{E0E2}';
/// Piano row icon.
pub const ICON_PIANO: char = '\u{E0E3}';
/// Synth row icon.
pub const ICON_SYNTH: char = '\u{E0E4}';
/// Warning glyph of a missing sound.
pub const ICON_WARN: char = '\u{E0E5}';

/// Whether `c` is one of the icon markers.
pub fn is_icon(c: char) -> bool {
    matches!(c, ICON_ORGAN | ICON_PIANO | ICON_SYNTH | ICON_WARN)
}

/// Left edge of the row text when the row starts with an icon, as a fraction of the display
/// width (12 of 128 px).
pub const TEXT_X: f32 = 12.0 / 128.0;

/// Draw `icon` in the square `cell` (side = 8 px of the real grid).
pub fn paint(painter: &Painter, cell: Rect, icon: char, color: Color32) {
    let stroke = Stroke::new(0.7_f32, color);
    let (l, t, w, h) = (cell.left(), cell.top(), cell.width(), cell.height());
    let at = |x: f32, y: f32| pos2(l + x * w, t + y * h);
    match icon {
        ICON_ORGAN => {
            // Three drawbars of different length.
            for (i, len) in [0.9_f32, 0.6, 0.3].into_iter().enumerate() {
                let x = 0.15 + 0.35 * i as f32;
                painter.line_segment([at(x, 0.05), at(x, 0.05 + len)], stroke);
            }
        }
        ICON_PIANO => {
            painter.rect_stroke(
                Rect::from_two_pos(at(0.0, 0.15), at(1.0, 0.95)),
                0.0,
                stroke,
                StrokeKind::Inside,
            );
            for x in [0.33_f32, 0.66] {
                painter.line_segment([at(x, 0.15), at(x, 0.95)], stroke);
            }
        }
        ICON_SYNTH => {
            let pts: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let u = i as f32 / 8.0;
                    at(u, 0.5 - 0.4 * (u * std::f32::consts::TAU).sin())
                })
                .collect();
            painter.add(Shape::line(pts, stroke));
        }
        ICON_WARN => {
            painter.add(Shape::closed_line(
                vec![at(0.5, 0.05), at(1.0, 0.95), at(0.0, 0.95)],
                stroke,
            ));
            painter.line_segment([at(0.5, 0.35), at(0.5, 0.65)], stroke);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn every_icon_is_a_marker_and_paints_shapes_only() {
        for icon in [ICON_ORGAN, ICON_PIANO, ICON_SYNTH, ICON_WARN] {
            assert!(is_icon(icon));
            let n = shapes_painted(|_ui, p, rect, _s, _a| {
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(8.0, 8.0)),
                    icon,
                    Color32::WHITE,
                );
            });
            let base = shapes_painted(|_ui, _p, _rect, _s, _a| {});
            assert!(n > base, "{icon:?} painted {n} over {base}");
        }
        assert!(!is_icon('A'));
        let base = shapes_painted(|_ui, _p, _rect, _s, _a| {});
        let none = shapes_painted(|_ui, p, rect, _s, _a| {
            paint(
                p,
                Rect::from_min_size(rect.min, vec2(8.0, 8.0)),
                'A',
                Color32::WHITE,
            );
        });
        assert_eq!(none, base);
    }

    #[test]
    fn the_row_text_starts_at_twelve_of_128_px() {
        assert_eq!(TEXT_X * 128.0, 12.0);
    }
}
