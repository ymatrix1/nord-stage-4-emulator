//! Display widgets: OLED text panel and triangle matrix selector.

use super::basic::nord_button;
use crate::style::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Rect, Stroke, StrokeKind, Ui};

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

/// Dual-column red triangle matrix selector with mode cycle button.
pub fn triangle_matrix_selector(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    left_items: &[&str],
    right_items: &[&str],
    selected_idx: usize,
) -> Option<usize> {
    let p = ui.painter();
    p.rect_filled(rect, 3.0, Color32::from_rgb(20, 24, 28));
    p.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );

    let total = left_items.len() + right_items.len();
    let row_h = (rect.height() - 24.0) / left_items.len().max(1) as f32;

    for (i, &name) in left_items.iter().enumerate() {
        let is_sel = selected_idx == i;
        let y = rect.top() + 4.0 + (i as f32) * row_h;
        // triangle
        let tri_center = pos2(rect.left() + 7.0, y + row_h * 0.5);
        let col = if is_sel { LED_RED } else { LED_RED_DIM };
        p.text(
            tri_center,
            Align2::CENTER_CENTER,
            "►",
            FontId::proportional(7.5),
            col,
        );
        p.text(
            pos2(rect.left() + 14.0, y + row_h * 0.5),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(7.5),
            if is_sel {
                Color32::WHITE
            } else {
                Color32::from_gray(180)
            },
        );
    }

    for (i, &name) in right_items.iter().enumerate() {
        let idx = left_items.len() + i;
        let is_sel = selected_idx == idx;
        let y = rect.top() + 4.0 + (i as f32) * row_h;
        let tri_center = pos2(rect.right() - 7.0, y + row_h * 0.5);
        let col = if is_sel { LED_RED } else { LED_RED_DIM };
        p.text(
            tri_center,
            Align2::CENTER_CENTER,
            "◄",
            FontId::proportional(7.5),
            col,
        );
        p.text(
            pos2(rect.right() - 14.0, y + row_h * 0.5),
            Align2::RIGHT_CENTER,
            name,
            FontId::proportional(7.5),
            if is_sel {
                Color32::WHITE
            } else {
                Color32::from_gray(180)
            },
        );
    }

    // Selector button underneath
    let btn_rect = Rect::from_min_size(
        pos2(rect.left() + 4.0, rect.bottom() - 20.0),
        vec2(rect.width() - 8.0, 16.0),
    );
    if nord_button(ui, btn_rect, id, false) {
        Some((selected_idx + 1) % total)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;
    use egui::{pos2, vec2, Rect};

    #[test]
    fn test_oled_and_triangle_selector_render_without_selection() {
        with_ui(|ui| {
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(160.0, 90.0));
            let p = ui.painter().clone();
            oled(&p, rect, "Big", "sub", &["a".to_string(), "b".to_string()]);
            let sel = triangle_matrix_selector(ui, rect, "tri", &["L1", "L2"], &["R1", "R2"], 0);
            assert_eq!(sel, None);
        });
    }
}
