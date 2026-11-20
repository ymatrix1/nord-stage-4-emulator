//! Pitch stick and mod wheel widget.

use egui::{vec2, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

/// Layout bounding boxes (mm) the shapes are scaled from: `master.pitch_stick` / `master.mod_wheel`.
const PITCH_BBOX_MM: (f32, f32) = (32.1, 19.4);
const MOD_BBOX_MM: (f32, f32) = (19.1, 49.0);
/// Slot and paddle sizes (length, width) in mm.
///
/// VERIFIED: measured on the HA88 top-down render (`data/photo_anchors_wheels.csv`, 0.1195 mm/px):
/// pitch slot 255 x 110 px, paddle 200 x 80 px, tilt -12 deg; wheel slot 395 x 90 px, tilt -14 deg
/// (error about +-1.5 mm, the slot edges are shaded).
const PITCH_SLOT_MM: (f32, f32) = (30.5, 13.0);
const PITCH_PADDLE_MM: (f32, f32) = (24.0, 9.6);
/// Wheel slot (width, length).
const MOD_SLOT_MM: (f32, f32) = (11.0, 48.0);
/// Rotation (radians, clockwise on screen) of the pitch slot and of the wheel slot's long axis.
const PITCH_TILT: f32 = -0.21;
const MOD_TILT: f32 = -0.245;

/// Filled rectangle `size` (along its own x, y) centred on `c`, rotated by `angle` radians.
fn fill_rotated(p: &Painter, c: Pos2, size: Vec2, angle: f32, fill: Color32, stroke: Stroke) {
    let (sin, cos) = angle.sin_cos();
    let pts = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| {
            let (px, py) = (x * size.x * 0.5, y * size.y * 0.5);
            c + vec2(px * cos - py * sin, px * sin + py * cos)
        })
        .to_vec();
    p.add(Shape::convex_polygon(pts, fill, stroke));
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
        // Dragging right bends up: positive values draw the paddle to the right.
        let d = resp_pitch.drag_delta().x * 0.02;
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

    // Slot (30.5 x 13 mm) and wooden paddle (24 x 9.6 mm), both tilted -12 degrees: the HA88
    // top-down render shows the right end higher. `rect_pitch` is the 32.1 x 19.4 mm bounding box.
    let s = rect_pitch.width() / PITCH_BBOX_MM.0;
    let c = rect_pitch.center();
    let axis = vec2(PITCH_TILT.cos(), PITCH_TILT.sin());
    fill_rotated(
        p,
        c,
        vec2(PITCH_SLOT_MM.0, PITCH_SLOT_MM.1) * s,
        PITCH_TILT,
        Color32::from_rgb(10, 11, 13),
        Stroke::new(1.0_f32, Color32::from_rgb(30, 34, 38)),
    );
    // The paddle travels along the slot: +1 is the right (upper) end.
    let assets = crate::baked::BakedPanelAssets::get_or_load(ui.ctx());
    let travel = (PITCH_SLOT_MM.0 - PITCH_PADDLE_MM.0) * 0.5 * s;
    let paddle_center = c + axis * (*pitch * travel);
    let paddle_size = vec2(PITCH_PADDLE_MM.0, PITCH_PADDLE_MM.1) * s;
    if !assets.paint_sprite_rotated(
        p,
        "pitch_stick",
        paddle_center,
        paddle_size,
        PITCH_TILT,
        Color32::WHITE,
    ) {
        fill_rotated(
            p,
            paddle_center,
            vec2(PITCH_PADDLE_MM.0, PITCH_PADDLE_MM.1) * s,
            PITCH_TILT,
            Color32::from_rgb(196, 140, 84),
            Stroke::new(1.0_f32, Color32::from_rgb(120, 80, 44)),
        );
    }

    // 2. Mod wheel (grooved metal cylinder)
    let resp_mod = ui.interact(rect_mod, ui.id().with("mod_wheel"), Sense::click_and_drag());
    if resp_mod.dragged() {
        let d = -resp_mod.drag_delta().y * 0.01;
        *mod_wheel = (*mod_wheel + d).clamp(0.0, 1.0);
        changed = true;
    }

    // Slot 48 x 11 mm tilted 14 degrees (bottom end to the right), silver wheel rim inside it with
    // a grip groove that runs along the slot as the wheel turns (0 = bottom, 1 = top).
    let s = rect_mod.height() / MOD_BBOX_MM.1;
    let c = rect_mod.center();
    let axis = vec2(-MOD_TILT.sin(), MOD_TILT.cos());
    let (w_slot, h_slot) = (MOD_SLOT_MM.0 * s, MOD_SLOT_MM.1 * s);
    fill_rotated(
        p,
        c,
        vec2(w_slot, h_slot),
        MOD_TILT,
        Color32::from_rgb(10, 11, 13),
        Stroke::new(1.0_f32, Color32::from_rgb(30, 34, 38)),
    );
    if !assets.paint_sprite_rotated(
        p,
        "mod_wheel_drum",
        c,
        vec2(w_slot * 0.92, h_slot * 1.02),
        MOD_TILT,
        Color32::WHITE,
    ) {
        fill_rotated(
            p,
            c,
            vec2(w_slot * 0.78, h_slot * 0.97),
            MOD_TILT,
            Color32::from_gray(176),
            Stroke::new(1.0_f32, Color32::from_gray(225)),
        );
    }
    let along = (0.5 - *mod_wheel) * h_slot * 0.68;
    fill_rotated(
        p,
        c + axis * along,
        vec2(w_slot * 0.62, 5.0),
        MOD_TILT,
        Color32::from_black_alpha(130),
        Stroke::new(0.8_f32, Color32::from_white_alpha(110)),
    );

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;
    use egui::{pos2, vec2, Rect};

    /// Pitch slot rises to the right, the wheel slot's bottom end leans right (HA88 render).
    #[test]
    fn tilts_follow_the_render() {
        const { assert!(PITCH_TILT < 0.0 && MOD_TILT < 0.0) };
        let (sin, cos) = PITCH_TILT.sin_cos();
        assert!(cos > 0.9 && sin < 0.0, "right end is higher on screen");
        assert!(PITCH_PADDLE_MM.0 < PITCH_SLOT_MM.0 && PITCH_PADDLE_MM.1 < PITCH_SLOT_MM.1);
    }

    #[test]
    fn rotated_rect_paints_one_polygon() {
        with_ui(|ui| {
            let n = ui.ctx().layer_painter(ui.layer_id());
            fill_rotated(
                &n,
                pos2(50.0, 50.0),
                vec2(40.0, 10.0),
                0.2,
                Color32::WHITE,
                Stroke::NONE,
            );
        });
    }

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

    /// Dragging the stick to the right must bend up (positive) and draw it right of centre.
    #[test]
    fn dragging_right_moves_the_pitch_stick_right() {
        use egui::{Event, Modifiers, PointerButton, RawInput};
        let ctx = egui::Context::default();
        let mut pitch = 0.0_f32;
        let mut modw = 0.0_f32;
        let rp = Rect::from_min_size(pos2(10.0, 10.0), vec2(40.0, 40.0));
        let rm = Rect::from_min_size(pos2(80.0, 10.0), vec2(30.0, 120.0));
        let c = rp.center();
        let mut run = |events: Vec<Event>| {
            let raw = RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    pitch_and_mod_wheels(ui, rp, rm, &mut pitch, &mut modw);
                });
            });
        };
        run(vec![Event::PointerMoved(c)]);
        run(vec![Event::PointerButton {
            pos: c,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        run(vec![Event::PointerMoved(pos2(c.x + 12.0, c.y))]);
        run(vec![Event::PointerMoved(pos2(c.x + 24.0, c.y))]);
        assert!(pitch > 0.1, "dragging right gave pitch {pitch}");
    }
}
