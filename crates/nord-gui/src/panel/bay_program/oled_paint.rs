//! Painter of the Program OLED text (scaled to the layout rectangle).

use super::menu_screen::DIM;
use super::oled::OledText;
use super::oled_home::EDITED;
use crate::panel::prelude::*;
use egui::{FontId, Pos2};

/// Draw the boxed `E` of an edited program with its top-left at `at`; returns the right edge.
fn boxed_e(painter: &Painter, at: Pos2, font: &FontId, color: Color32) -> f32 {
    let glyph = painter.text(
        at + vec2(1.0, 0.0),
        Align2::LEFT_TOP,
        "E",
        font.clone(),
        color,
    );
    let frame = glyph.expand2(vec2(1.0, 0.0));
    painter.rect_stroke(frame, 0.0, Stroke::new(0.7_f32, color), StrokeKind::Inside);
    frame.right() + 2.0
}

/// Row geometry of the real Program home screen on its 128 x 64 pixel grid.
///
/// VERIFIED (frames sent by the ARM emulator, `tools/arm_screens.py`, measured in
/// `docs/research/arm-oled-render.md` section 9): the location title and the name line are both
/// 13 px proportional text at the left edge, text boxes at y = 3 and y = 18 (15 px pitch); a 1 px
/// rule at y = 33 spans the full width; the section rows below use the 8 px font from y = 36 every
/// 10 px. The sizes here are those pixels divided by 64, so they scale with the rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Top of the title text box.
    pub title_y: f32,
    /// Top of the name line text box.
    pub name_y: f32,
    /// The full-width rule between the header and the rows.
    pub rule_y: f32,
    /// Top of the first row.
    pub rows_y: f32,
    /// Distance between rows.
    pub row_pitch: f32,
}

/// Real pixel rows of [`Grid`] for a display `height` (in the unit of the caller).
pub fn grid(height: f32) -> Grid {
    let k = height / 64.0;
    Grid {
        title_y: 3.0 * k,
        name_y: 18.0 * k,
        rule_y: 33.0 * k,
        rows_y: 36.0 * k,
        row_pitch: 10.0 * k,
    }
}

/// Paint the display text into `rect`, scaling the type to the rectangle.
pub fn paint(painter: &Painter, rect: Rect, text: &OledText, dim: bool) {
    let fg = if dim {
        Color32::from_rgb(80, 100, 120)
    } else {
        Color32::from_rgb(150, 190, 230)
    };
    painter.rect_filled(rect.expand(1.5), 2.0, Color32::from_rgb(10, 11, 13));
    painter.rect_filled(rect, 1.5, OLED_BG);
    let h = rect.height();
    // The name line is the same 13 px font as the title on the real unit (not a smaller one).
    let (f_big, f_sub, f_row) = (
        (h * 0.19).max(5.0),
        (h * 0.19).max(5.0),
        (h * 0.12).max(4.0),
    );
    let g = grid(h);
    let mut y = rect.top() + g.title_y;
    let line = |y: &mut f32, s: &str, size: f32, pitch: f32| {
        // A row that starts with the DIM mark is an unsupported menu item: drawn faded.
        let (s, color) = match s.strip_prefix(DIM) {
            Some(rest) => (rest, fg.gamma_multiply(0.45)),
            None => (s, fg),
        };
        let font = FontId::monospace(size.max(MIN_FONT_PX));
        let mut x = rect.left() + 2.0;
        let mut segments = s.split(EDITED);
        let first = segments.next().unwrap_or_default();
        x = painter
            .text(pos2(x, *y), Align2::LEFT_TOP, first, font.clone(), color)
            .right();
        for rest in segments {
            x = boxed_e(painter, pos2(x + 1.0, *y), &font, color);
            x = painter
                .text(pos2(x, *y), Align2::LEFT_TOP, rest, font.clone(), color)
                .right();
        }
        *y += pitch;
    };
    line(&mut y, &text.big, f_big, g.name_y - g.title_y);
    line(&mut y, &text.sub, f_sub, g.rule_y - g.name_y);
    painter.line_segment(
        [pos2(rect.left() + 1.0, y), pos2(rect.right() - 1.0, y)],
        Stroke::new(0.5_f32, fg),
    );
    y += g.rows_y - g.rule_y;
    let soft_h = if text.soft.iter().any(|l| !l.is_empty()) {
        f_row * 1.4
    } else {
        0.0
    };
    for r in &text.rows {
        if y + f_row > rect.bottom() - soft_h {
            break;
        }
        line(&mut y, r, f_row, g.row_pitch);
    }
    if soft_h > 0.0 {
        let w = rect.width() / 4.0;
        for (i, l) in text.soft.iter().enumerate() {
            if l.is_empty() {
                continue;
            }
            let r = Rect::from_min_size(
                pos2(rect.left() + w * i as f32 + 0.5, rect.bottom() - soft_h),
                vec2(w - 1.0, soft_h - 0.5),
            );
            painter.rect_filled(r, 1.0, fg.gamma_multiply(0.35));
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                *l,
                FontId::monospace(f_row.max(MIN_FONT_PX)),
                Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn grid_is_the_measured_pixel_rows_of_the_real_home_screen() {
        let g = grid(64.0);
        assert_eq!(
            (g.title_y, g.name_y, g.rule_y, g.rows_y, g.row_pitch),
            (3.0, 18.0, 33.0, 36.0, 10.0)
        );
        // The title and the name line are one 15 px pitch apart; the rule is two pitches down.
        assert_eq!(g.name_y - g.title_y, 15.0);
        assert_eq!(g.rule_y - g.title_y, 30.0);
        // It scales with the display height.
        let half = grid(32.0);
        assert_eq!((half.name_y, half.rule_y), (9.0, 16.5));
    }

    #[test]
    fn the_edited_mark_adds_a_box_and_the_e() {
        let paint_big = |big: &str| {
            shapes_painted(|_ui, p, rect, _s, _a| {
                let t = OledText {
                    big: big.into(),
                    ..Default::default()
                };
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(80.0, 40.0)),
                    &t,
                    false,
                );
            })
        };
        let plain = paint_big("A:11");
        let edited = paint_big(&format!("A:11{EDITED}"));
        assert!(edited >= plain + 2, "box and E: {plain} -> {edited}");
    }

    #[test]
    fn painter_draws_text_rows_and_only_the_labelled_soft_buttons() {
        let paint_with = |soft: [&'static str; 4]| {
            shapes_painted(|_ui, p, rect, _s, _a| {
                let t = OledText {
                    big: "A".into(),
                    sub: "b".into(),
                    rows: vec!["row".into()],
                    soft,
                };
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(80.0, 40.0)),
                    &t,
                    false,
                );
            })
        };
        let none = paint_with(["", "", "", ""]);
        let two = paint_with(["a", "", "c", ""]);
        assert!(none >= 5, "painted {none}");
        assert!(two > none, "soft buttons add shapes");
    }
}
