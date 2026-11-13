//! Pitch stick and mod wheel widget.

use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;
    use egui::{pos2, vec2, Rect};

    #[test]
    fn test_wheels_unchanged_without_input() {
        with_ui(|ui| {
            let mut pitch = 0.0_f32;
            let mut modw = 0.25_f32;
            let rp = Rect::from_min_size(pos2(10.0, 10.0), vec2(30.0, 120.0));
            let rm = Rect::from_min_size(pos2(60.0, 10.0), vec2(30.0, 120.0));
            assert!(!pitch_and_mod_wheels(ui, rp, rm, &mut pitch, &mut modw));
            assert_eq!(modw, 0.25);
        });
    }
}
