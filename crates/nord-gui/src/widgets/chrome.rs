//! Static chrome: labels, LEDs, section/group frames and the OLED display.

use crate::style::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind};

/// Draws a text label.
pub fn label(p: &Painter, pos: Pos2, s: &str, size: f32, col: Color32, al: Align2) {
    p.text(pos, al, s, FontId::proportional(size), col);
}

/// Draws a round indicator LED with a dark bezel, 3D glint, and realistic bloom.
pub fn led(p: &Painter, c: Pos2, on: bool, col_on: Color32, col_off: Color32) {
    p.circle_filled(c, 4.5, Color32::from_rgb(14, 16, 19));
    p.circle_filled(c, 3.4, if on { col_on } else { col_off });
    if on {
        // Diffuse outer bloom
        p.circle_stroke(c, 5.2, Stroke::new(1.5_f32, col_on.gamma_multiply(0.40)));
        // Specular glint dot (top-left)
        p.circle_filled(c - vec2(1.1, 1.1), 0.9, Color32::from_white_alpha(200));
    } else {
        p.circle_stroke(c, 3.4, Stroke::new(1.0_f32, Color32::from_black_alpha(80)));
    }
}

/// Section frame: slate body with silver header strip and title.
pub fn section_frame(p: &Painter, rect: Rect, title: &str) {
    p.rect_filled(rect, 5.0, MODULE_SLATE);
    let hdr = Rect::from_min_size(rect.min, vec2(rect.width(), 22.0));
    p.rect_filled(hdr, 5.0, MODULE_HEADER_BAR);
    p.rect_filled(
        Rect::from_min_size(pos2(hdr.left(), hdr.bottom() - 6.0), vec2(hdr.width(), 6.0)),
        0.0,
        MODULE_HEADER_BAR,
    );
    label(
        p,
        pos2(rect.left() + 8.0, hdr.center().y),
        title,
        13.0,
        Color32::from_rgb(20, 24, 30),
        Align2::LEFT_CENTER,
    );
    p.rect_stroke(
        rect,
        5.0,
        Stroke::new(1.0_f32, MODULE_SLATE_DARK),
        StrokeKind::Inside,
    );
}

/// Silkscreen rounded group outline with a centred title that breaks the border.
pub fn group_frame(p: &Painter, rect: Rect, title: &str) {
    p.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.2_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    let w = title.len() as f32 * 6.4 + 10.0;
    let r = Rect::from_center_size(pos2(rect.center().x, rect.top()), vec2(w, 12.0));
    p.rect_filled(r, 0.0, MODULE_SLATE);
    label(
        p,
        r.center(),
        title,
        10.0,
        Color32::from_gray(235),
        Align2::CENTER_CENTER,
    );
}

/// Monochrome OLED with up to 5 text lines (first is large).
pub fn oled(p: &Painter, rect: Rect, big: &str, sub: &str, lines: &[String]) {
    p.rect_filled(rect.expand(3.0), 3.0, Color32::from_rgb(10, 11, 13));
    p.rect_filled(rect, 2.0, OLED_BG);
    let fg = Color32::from_rgb(150, 190, 230);
    p.text(
        pos2(rect.left() + 6.0, rect.top() + 5.0),
        Align2::LEFT_TOP,
        big,
        FontId::monospace(14.0),
        fg,
    );
    p.text(
        pos2(rect.left() + 6.0, rect.top() + 21.0),
        Align2::LEFT_TOP,
        sub,
        FontId::monospace(13.0),
        fg,
    );
    p.line_segment(
        [
            pos2(rect.left() + 4.0, rect.top() + 39.0),
            pos2(rect.right() - 4.0, rect.top() + 39.0),
        ],
        Stroke::new(1.0_f32, fg),
    );
    for (i, l) in lines.iter().enumerate() {
        p.text(
            pos2(rect.left() + 6.0, rect.top() + 42.0 + i as f32 * 12.0),
            Align2::LEFT_TOP,
            l,
            FontId::monospace(9.5),
            fg,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::{rect, run_in_ui};

    #[test]
    fn test_widget_color_constants() {
        assert_eq!(LED_RED.r(), 0xFF);
        assert_eq!(LED_GREEN.g(), 0xE6);
    }

    #[test]
    fn test_stroke_float_type_regression() {
        let s1 = Stroke::new(1.0_f32, Color32::BLACK);
        let s2 = Stroke::new(1.5_f32, Color32::WHITE);
        assert_eq!(s1.width, 1.0_f32);
        assert_eq!(s2.width, 1.5_f32);
    }

    #[test]
    fn test_chrome_draws_shapes() {
        let shapes = run_in_ui(|ui| {
            let p = ui.painter().clone();
            label(
                &p,
                pos2(10.0, 10.0),
                "HELLO",
                10.0,
                Color32::WHITE,
                Align2::LEFT_TOP,
            );
            led(&p, pos2(20.0, 20.0), true, LED_RED, Color32::BLACK);
            section_frame(&p, rect(0.0, 0.0, 100.0, 60.0), "SECTION");
            group_frame(&p, rect(0.0, 0.0, 80.0, 40.0), "GROUP");
            oled(
                &p,
                rect(0.0, 0.0, 120.0, 60.0),
                "Big",
                "sub",
                &["a".to_string()],
            );
        });
        assert!(shapes > 10, "expected painted shapes, got {shapes}");
    }
}
