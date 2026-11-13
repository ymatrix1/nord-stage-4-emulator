//! Basic widgets: labels, LEDs, section/group frames, and the pill button.

use crate::style::*;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
};

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

/// Recessed 3D pill button with tactile bevel and depression. Returns `true` when clicked.
pub fn nord_button(ui: &mut Ui, rect: Rect, id: &str, grey: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect.expand(2.0), 3.5, BUTTON_BEZEL);
    let is_down = resp.is_pointer_button_down_on();
    let base = if grey { BUTTON_GREY } else { BUTTON_BLACK };
    let body = if is_down {
        base.gamma_multiply(0.7)
    } else {
        base
    };
    let btn_rect = if is_down {
        rect.translate(vec2(0.0, 1.0))
    } else {
        rect
    };
    p.rect_filled(btn_rect, 3.0, body);
    let pill = btn_rect.shrink2(vec2(btn_rect.width() * 0.16, btn_rect.height() * 0.22));
    let hi = if grey {
        Color32::from_gray(190)
    } else {
        Color32::from_gray(58)
    };
    p.rect_filled(pill, pill.height() * 0.5, body.gamma_multiply(0.85));
    p.rect_stroke(
        pill,
        pill.height() * 0.5,
        Stroke::new(1.2_f32, hi),
        StrokeKind::Inside,
    );
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;

    #[test]
    fn test_basic_widgets_render_and_button_is_not_clicked() {
        with_ui(|ui| {
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 40.0));
            let p = ui.painter().clone();
            label(
                &p,
                pos2(5.0, 5.0),
                "Hi",
                12.0,
                Color32::WHITE,
                Align2::LEFT_TOP,
            );
            led(&p, pos2(20.0, 20.0), true, LED_RED, Color32::BLACK);
            section_frame(&p, rect, "Sec");
            group_frame(&p, rect, "Grp");
            assert!(!nord_button(ui, rect, "btn", false));
        });
    }
}
