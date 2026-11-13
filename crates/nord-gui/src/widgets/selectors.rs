//! Triangle-matrix selector, pitch stick / mod wheel and the `cycle` helper.

use super::buttons::nord_button;
use crate::style::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui};

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

/// Authentic Nord wooden pitch stick and steel mod wheel.
pub fn pitch_and_mod_wheels(
    ui: &mut Ui,
    rect_pitch: Rect,
    rect_mod: Rect,
    pitch: &mut f32,
    mod_wheel: &mut f32,
) -> bool {
    let mut changed = false;
    let p = ui.painter();

    // 1. Pitch stick (wooden spring-loaded paddle)
    let resp_pitch = ui.interact(
        rect_pitch,
        ui.id().with("pitch_stick"),
        Sense::click_and_drag(),
    );
    if resp_pitch.dragged() {
        let d = -resp_pitch.drag_delta().x * 0.02;
        *pitch = (*pitch + d).clamp(-1.0, 1.0);
        changed = true;
    } else if !resp_pitch.is_pointer_button_down_on() && pitch.abs() > 1e-4 {
        // Return to center spring!
        *pitch *= 0.5;
        if pitch.abs() < 1e-3 {
            *pitch = 0.0;
        }
        changed = true;
    }

    // Bezel recess
    p.rect_filled(rect_pitch, 4.0, Color32::from_rgb(12, 14, 16));
    p.rect_stroke(
        rect_pitch,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(35, 40, 45)),
        StrokeKind::Inside,
    );
    // Wooden paddle
    let stick_w = 16.0;
    let stick_h = rect_pitch.height() * 0.7;
    let offset_x = *pitch * (rect_pitch.width() * 0.35);
    let stick_rect = Rect::from_center_size(
        pos2(rect_pitch.center().x + offset_x, rect_pitch.center().y),
        vec2(stick_w, stick_h),
    );
    p.rect_filled(stick_rect, 3.0, Color32::from_rgb(180, 130, 80)); // Wood beech tone
    p.rect_stroke(
        stick_rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_rgb(110, 75, 40)),
        StrokeKind::Inside,
    );

    // 2. Mod wheel (grooved metal cylinder)
    let resp_mod = ui.interact(rect_mod, ui.id().with("mod_wheel"), Sense::click_and_drag());
    if resp_mod.dragged() {
        let d = -resp_mod.drag_delta().y * 0.01;
        *mod_wheel = (*mod_wheel + d).clamp(0.0, 1.0);
        changed = true;
    }

    p.rect_filled(rect_mod, 4.0, Color32::from_rgb(12, 14, 16));
    p.rect_stroke(
        rect_mod,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(35, 40, 45)),
        StrokeKind::Inside,
    );

    let travel = rect_mod.height() * 0.7;
    let mod_y = rect_mod.bottom() - 12.0 - *mod_wheel * travel;
    let mod_cap = Rect::from_center_size(
        pos2(rect_mod.center().x, mod_y),
        vec2(rect_mod.width() * 0.75, 18.0),
    );
    p.rect_filled(mod_cap, 2.0, Color32::from_gray(180));
    p.rect_stroke(
        mod_cap,
        2.0,
        Stroke::new(1.0_f32, Color32::from_gray(240)),
        StrokeKind::Inside,
    );

    changed
}

/// Cycles to the next element of `items` after `cur` (wraps).
pub fn cycle<T: Copy + PartialEq>(items: &[T], cur: T) -> T {
    let i = items.iter().position(|x| *x == cur).unwrap_or(0);
    items[(i + 1) % items.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::{rect, run_in_ui};

    #[test]
    fn test_cycle_wraps() {
        assert_eq!(cycle(&[1, 2, 3], 3), 1);
        assert_eq!(cycle(&[1, 2, 3], 1), 2);
        assert_eq!(cycle(&[1, 2, 3], 9), 2);
    }

    #[test]
    fn test_selector_and_wheels_idle() {
        run_in_ui(|ui| {
            let sel = triangle_matrix_selector(
                ui,
                rect(0.0, 0.0, 120.0, 80.0),
                "s",
                &["A", "B"],
                &["C", "D"],
                0,
            );
            assert_eq!(sel, None);
            let (mut pitch, mut modw) = (0.0_f32, 0.0_f32);
            assert!(!pitch_and_mod_wheels(
                ui,
                rect(0.0, 100.0, 30.0, 80.0),
                rect(40.0, 100.0, 20.0, 80.0),
                &mut pitch,
                &mut modw
            ));
        });
    }
}
