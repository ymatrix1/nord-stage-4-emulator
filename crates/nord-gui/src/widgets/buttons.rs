//! Push buttons and the layer strip (ON/OFF + fader + VU ladder).

use super::chrome::led;
use crate::style::*;
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};

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

/// Layer strip: ON/OFF button, vertical fader with 3D metallic cap and 11-segment VU ladder.
/// `rect` is ~34 x 150. Returns `true` when level or active changed.
pub fn layer_strip(ui: &mut Ui, rect: Rect, id: &str, level: &mut f32, active: &mut bool) -> bool {
    let mut changed = false;

    // Fader area
    let fader = Rect::from_min_max(
        pos2(rect.left(), rect.top()),
        pos2(rect.left() + 18.0, rect.bottom() - 40.0),
    );
    let resp = ui.interact(fader, ui.id().with(id), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let n = 1.0 - ((pos.y - fader.top() - 8.0) / (fader.height() - 16.0)).clamp(0.0, 1.0);
            *level = n;
            changed = true;
        }
    }

    // ON button
    let btn = Rect::from_min_size(
        pos2(rect.left() - 2.0, rect.bottom() - 28.0),
        vec2(30.0, 18.0),
    );
    if nord_button(ui, btn, &format!("{id}_on"), true) {
        *active = !*active;
        changed = true;
    }

    let p = ui.painter();
    // Slot
    let slot = Rect::from_min_max(
        pos2(fader.center().x - 2.0, fader.top()),
        pos2(fader.center().x + 2.0, fader.bottom()),
    );
    p.rect_filled(slot, 1.5, Color32::from_rgb(10, 12, 15));
    p.rect_stroke(
        slot,
        1.5,
        Stroke::new(1.0_f32, Color32::from_rgb(28, 30, 35)),
        StrokeKind::Inside,
    );

    // 11-segment VU Ladder (Green -> Amber -> Red)
    for i in 0..11 {
        let y = fader.bottom() - 6.0 - i as f32 * ((fader.height() - 8.0) / 11.0);
        let r = Rect::from_min_size(pos2(fader.right() + 6.0, y - 5.0), vec2(9.0, 5.0));
        let lit = *active && ((*level * 11.0).round() as usize) > i;
        let col = if i >= 10 {
            if lit {
                LED_RED
            } else {
                LED_RED_DIM
            }
        } else if i >= 8 {
            if lit {
                LED_AMBER
            } else {
                LED_AMBER_DIM
            }
        } else if lit {
            LED_GREEN
        } else {
            LED_GREEN_DIM
        };
        p.rect_filled(r, 1.0, col);
        if lit {
            p.circle_filled(r.center(), 1.0, Color32::from_white_alpha(180));
        }
    }

    // 3D Metallic Fader Cap
    let cy = fader.bottom() - 8.0 - level.clamp(0.0, 1.0) * (fader.height() - 16.0);
    let cap = Rect::from_center_size(pos2(fader.center().x, cy), vec2(24.0, 16.0));
    // Soft drop shadow
    p.rect_filled(
        cap.translate(vec2(0.0, 2.5)),
        3.0,
        Color32::from_black_alpha(100),
    );
    // Cap metallic body
    p.rect_filled(cap, 3.0, Color32::from_gray(160));
    // Top bevel highlight
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 3.5)),
        3.0,
        Color32::from_gray(220),
    );
    // Bottom bevel shadow
    p.rect_filled(
        Rect::from_min_size(pos2(cap.left(), cap.bottom() - 3.5), vec2(cap.width(), 3.5)),
        3.0,
        Color32::from_gray(115),
    );
    // Center grip line
    p.line_segment(
        [
            pos2(cap.left() + 2.0, cap.center().y),
            pos2(cap.right() - 2.0, cap.center().y),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(25, 28, 32)),
    );
    p.rect_stroke(
        cap,
        3.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );

    // ON LED centered above button
    led(
        p,
        pos2(btn.center().x, btn.top() - 7.0),
        *active,
        LED_GREEN,
        LED_GREEN_DIM,
    );
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::{rect, run_in_ui};

    #[test]
    fn test_button_and_layer_strip_idle_do_not_report_change() {
        run_in_ui(|ui| {
            assert!(!nord_button(ui, rect(0.0, 0.0, 40.0, 20.0), "b", false));
            let (mut level, mut active) = (0.5_f32, true);
            assert!(!layer_strip(
                ui,
                rect(50.0, 0.0, 34.0, 150.0),
                "l",
                &mut level,
                &mut active
            ));
            assert_eq!(level, 0.5);
            assert!(active);
        });
    }
}
