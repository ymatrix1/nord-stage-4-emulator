//! Custom Hardware Widgets: 11-LED Ladders, Drawbars, Triangles, Knobs, OLEDs.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use crate::style::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui};

/// Drawbar widget with authentic cap coloring and vertical slot.
pub fn drawbar(
    ui: &mut Ui,
    value: &mut u8,
    label_top: &str,
    label_bot: &str,
    is_white: bool,
) -> Response {
    let size = vec2(28.0, 110.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let norm = ((pos.y - (rect.top() + 15.0)) / (rect.height() - 30.0)).clamp(0.0, 1.0);
            *value = (norm * 8.0).round() as u8;
            response.mark_changed();
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // 1. Labels
        painter.text(
            pos2(rect.center().x, rect.top() + 4.0),
            Align2::CENTER_CENTER,
            label_top,
            FontId::proportional(9.0),
            Color32::from_gray(210),
        );
        painter.text(
            pos2(rect.center().x, rect.bottom() - 4.0),
            Align2::CENTER_CENTER,
            label_bot,
            FontId::proportional(9.5),
            Color32::from_gray(240),
        );

        // 2. Vertical track slot
        let track_rect = Rect::from_min_max(
            pos2(rect.center().x - 2.5, rect.top() + 15.0),
            pos2(rect.center().x + 2.5, rect.bottom() - 15.0),
        );
        painter.rect_filled(track_rect, 1.5, Color32::from_rgb(18, 22, 26));

        // 3. 8-Segment Red LED Ladder (left of track)
        for i in 0..8 {
            let led_y = rect.top() + 18.0 + (i as f32) * 9.5;
            let led_rect = Rect::from_min_max(
                pos2(rect.left() + 1.0, led_y),
                pos2(rect.left() + 5.0, led_y + 6.0),
            );
            let active = i < *value as usize;
            painter.rect_filled(led_rect, 1.0, if active { LED_RED } else { LED_RED_DIM });
        }

        // 4. Physical Drawbar Cap
        let cap_y = rect.top() + 15.0 + (*value as f32 / 8.0) * (rect.height() - 36.0);
        let cap_rect = Rect::from_min_max(
            pos2(rect.center().x - 9.0, cap_y),
            pos2(rect.center().x + 9.0, cap_y + 14.0),
        );
        let cap_color = if is_white {
            DRAWBAR_WHITE
        } else {
            DRAWBAR_BLACK
        };
        painter.rect_filled(cap_rect, 2.0, cap_color);
        painter.rect_stroke(
            cap_rect,
            2.0,
            Stroke::new(1.0_f32, Color32::from_gray(80)),
            StrokeKind::Inside,
        );

        // Center notch line on cap
        painter.line_segment(
            [
                pos2(cap_rect.left() + 2.0, cap_rect.center().y),
                pos2(cap_rect.right() - 2.0, cap_rect.center().y),
            ],
            Stroke::new(
                1.5_f32,
                if is_white {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                },
            ),
        );
    }

    response
}

/// 11-Segment Green Level Ladder with adjacent vertical Fader.
pub fn layer_fader_with_ladder(
    ui: &mut Ui,
    level: &mut f32,
    active: &mut bool,
    label: &str,
) -> Response {
    let size = vec2(42.0, 130.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let norm =
                1.0 - ((pos.y - (rect.top() + 25.0)) / (rect.height() - 45.0)).clamp(0.0, 1.0);
            *level = norm;
            response.mark_changed();
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Layer ON button pill at top
        let btn_rect = Rect::from_min_max(
            pos2(rect.center().x - 14.0, rect.top() + 2.0),
            pos2(rect.center().x + 14.0, rect.top() + 18.0),
        );
        let fill = if *active { BUTTON_GREY } else { BUTTON_BLACK };
        painter.rect_filled(btn_rect, 3.0, fill);
        painter.rect_stroke(
            btn_rect,
            3.0,
            Stroke::new(1.0, GROUP_BORDER),
            StrokeKind::Inside,
        );
        painter.text(
            btn_rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            Color32::WHITE,
        );

        // Green ON LED dot
        let dot_pos = pos2(btn_rect.right() - 4.0, btn_rect.top() + 4.0);
        painter.circle_filled(
            dot_pos,
            2.0,
            if *active { LED_GREEN } else { LED_GREEN_DIM },
        );

        // Fader track
        let track_rect = Rect::from_min_max(
            pos2(rect.left() + 10.0, rect.top() + 25.0),
            pos2(rect.left() + 14.0, rect.bottom() - 15.0),
        );
        painter.rect_filled(track_rect, 1.5, Color32::from_rgb(18, 22, 26));

        // 11-segment Green LED Ladder (right of track)
        for i in 0..11 {
            let led_y = rect.bottom() - 22.0 - (i as f32) * 7.5;
            let led_rect = Rect::from_min_max(
                pos2(rect.left() + 22.0, led_y),
                pos2(rect.left() + 30.0, led_y + 5.0),
            );
            let is_lit = *active && (*level * 11.0).round() as usize > i;
            painter.rect_filled(
                led_rect,
                1.0,
                if is_lit { LED_GREEN } else { LED_GREEN_DIM },
            );
        }

        // Fader Cap
        let cap_y = rect.bottom() - 25.0 - (*level).clamp(0.0, 1.0) * (rect.height() - 52.0);
        let cap_rect = Rect::from_min_max(
            pos2(rect.left() + 3.0, cap_y),
            pos2(rect.left() + 21.0, cap_y + 12.0),
        );
        painter.rect_filled(cap_rect, 2.0, BUTTON_GREY);
        painter.rect_stroke(
            cap_rect,
            2.0,
            Stroke::new(1.0, Color32::from_gray(50)),
            StrokeKind::Inside,
        );
        painter.line_segment(
            [
                pos2(cap_rect.left(), cap_rect.center().y),
                pos2(cap_rect.right(), cap_rect.center().y),
            ],
            Stroke::new(1.5, Color32::WHITE),
        );
    }

    response
}

/// Rotary control knob with radial indicator and label.
pub fn nord_knob(ui: &mut Ui, value: &mut f32, min: f32, max: f32, label: &str) -> Response {
    let size = vec2(42.0, 52.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    if response.dragged() {
        let delta = response.drag_delta().y * -0.005 * (max - min);
        *value = (*value + delta).clamp(min, max);
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let center = pos2(rect.center().x, rect.top() + 20.0);
        let radius = 15.0;

        // Knob body
        painter.circle_filled(center, radius, Color32::from_rgb(26, 28, 32));
        painter.circle_stroke(center, radius, Stroke::new(1.5, Color32::from_gray(90)));

        // Radial pointer line
        let norm = (*value - min) / (max - min).max(1e-5);
        let angle = 0.75 * std::f32::consts::PI + norm * 1.5 * std::f32::consts::PI;
        let p_end = pos2(
            center.x + angle.cos() * (radius - 2.0),
            center.y + angle.sin() * (radius - 2.0),
        );
        let p_start = pos2(center.x + angle.cos() * 4.0, center.y + angle.sin() * 4.0);
        painter.line_segment([p_start, p_end], Stroke::new(2.0, Color32::WHITE));

        // Knob label
        painter.text(
            pos2(rect.center().x, rect.bottom() - 6.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(9.5),
            Color32::from_gray(220),
        );
    }

    response
}

/// Monochrome Program OLED Display Widget.
pub fn program_oled(
    ui: &mut Ui,
    prog_id: &str,
    title: &str,
    organ_info: &str,
    piano_info: &str,
    synth_info: &str,
) {
    let size = vec2(150.0, 70.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // Screen bezel & glass
        painter.rect_filled(rect, 4.0, OLED_BG);
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.5, Color32::from_rgb(40, 45, 50)),
            StrokeKind::Inside,
        );

        // Content
        painter.text(
            pos2(rect.left() + 8.0, rect.top() + 10.0),
            Align2::LEFT_CENTER,
            prog_id,
            FontId::monospace(13.0),
            OLED_FG,
        );
        painter.text(
            pos2(rect.left() + 45.0, rect.top() + 10.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(12.0),
            OLED_FG,
        );

        painter.line_segment(
            [
                pos2(rect.left() + 6.0, rect.top() + 22.0),
                pos2(rect.right() - 6.0, rect.top() + 22.0),
            ],
            Stroke::new(1.0, Color32::from_gray(60)),
        );

        painter.text(
            pos2(rect.left() + 8.0, rect.top() + 32.0),
            Align2::LEFT_CENTER,
            format!("Org: {}", organ_info),
            FontId::monospace(9.5),
            Color32::from_gray(180),
        );
        painter.text(
            pos2(rect.left() + 8.0, rect.top() + 46.0),
            Align2::LEFT_CENTER,
            format!("Pno: {}", piano_info),
            FontId::monospace(9.5),
            Color32::from_gray(180),
        );
        painter.text(
            pos2(rect.left() + 8.0, rect.top() + 60.0),
            Align2::LEFT_CENTER,
            format!("Syn: {}", synth_info),
            FontId::monospace(9.5),
            Color32::from_gray(180),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_color_constants() {
        assert_eq!(LED_RED.r(), 0xFF);
        assert_eq!(LED_GREEN.g(), 0xE6);
    }

    #[test]
    fn test_stroke_float_type_regression() {
        // Enforce f32 compatibility for egui Stroke constructions to prevent
        // Windows/Linux compiler warnings & clippy errors on trait bounds (From<f64>).
        let s1 = Stroke::new(1.0_f32, Color32::BLACK);
        let s2 = Stroke::new(1.5_f32, Color32::WHITE);
        assert_eq!(s1.width, 1.0_f32);
        assert_eq!(s2.width, 1.5_f32);
    }
}
