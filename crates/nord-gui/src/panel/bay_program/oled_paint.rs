//! Painter of the Program OLED text (scaled to the layout rectangle).

use super::menu_screen::DIM;
use super::oled::OledText;
use crate::panel::prelude::*;
use egui::FontId;

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
    let (f_big, f_sub, f_row) = (
        (h * 0.19).max(5.0),
        (h * 0.16).max(4.5),
        (h * 0.12).max(4.0),
    );
    let mut y = rect.top() + 1.0;
    let line = |y: &mut f32, s: &str, size: f32| {
        // A row that starts with the DIM mark is an unsupported menu item: drawn faded.
        let (s, color) = match s.strip_prefix(DIM) {
            Some(rest) => (rest, fg.gamma_multiply(0.45)),
            None => (s, fg),
        };
        painter.text(
            pos2(rect.left() + 2.0, *y),
            Align2::LEFT_TOP,
            s,
            FontId::monospace(size.max(MIN_FONT_PX)),
            color,
        );
        *y += size * 1.15;
    };
    line(&mut y, &text.big, f_big);
    line(&mut y, &text.sub, f_sub);
    painter.line_segment(
        [pos2(rect.left() + 1.0, y), pos2(rect.right() - 1.0, y)],
        Stroke::new(0.5_f32, fg),
    );
    y += 1.0;
    let soft_h = if text.soft.iter().any(|l| !l.is_empty()) {
        f_row * 1.4
    } else {
        0.0
    };
    for r in &text.rows {
        if y + f_row > rect.bottom() - soft_h {
            break;
        }
        line(&mut y, r, f_row);
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
