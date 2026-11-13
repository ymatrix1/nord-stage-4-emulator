//! Drawbars and rotary knobs.

use super::chrome::label;
use crate::style::*;
use egui::{pos2, vec2, Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

/// Organ drawbar (0 = pushed in at top, 8 = pulled out at bottom) with 3D molded handle and 8-LED ladder.
/// `rect` is ~28 x 150. Returns `true` on change.
pub fn drawbar(ui: &mut Ui, rect: Rect, id: &str, value: &mut u8, white: bool) -> bool {
    let mut changed = false;
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    let top = rect.top() + 12.0;
    let travel = rect.height() - 40.0;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let n = ((pos.y - top - 8.0) / travel).clamp(0.0, 1.0);
            let v = (n * 8.0).round() as u8;
            if v != *value {
                *value = v;
                changed = true;
            }
        }
    }
    let p = ui.painter();
    // LED ladder (left)
    for i in 0..8 {
        let y = rect.top() + 2.0 + i as f32 * 9.0;
        let r = Rect::from_min_size(pos2(rect.left(), y), vec2(7.0, 6.0));
        let lit = (i as u8) < *value;
        p.rect_filled(r, 1.0, if lit { LED_RED } else { LED_RED_DIM });
        if lit {
            p.circle_filled(r.center(), 1.0, Color32::from_rgb(255, 120, 120));
        }
    }
    // Recessed Slot
    let sx = rect.right() - 8.0;
    let slot = Rect::from_min_max(pos2(sx - 2.5, top), pos2(sx + 2.5, top + travel + 16.0));
    p.rect_filled(slot, 1.5, Color32::from_rgb(8, 10, 12));
    p.rect_stroke(
        slot,
        1.5,
        Stroke::new(1.0_f32, Color32::from_rgb(25, 28, 32)),
        StrokeKind::Inside,
    );

    // Cap (3D molded handle)
    let cy = top + 8.0 + (*value as f32 / 8.0) * travel;
    let cap = Rect::from_center_size(pos2(sx - 2.0, cy), vec2(26.0, 26.0));
    // Soft cast shadow
    p.rect_filled(
        cap.translate(vec2(0.0, 3.0)),
        3.0,
        Color32::from_black_alpha(100),
    );
    let (c, edge, hi) = if white {
        (DRAWBAR_WHITE, Color32::from_gray(140), Color32::WHITE)
    } else {
        (
            DRAWBAR_BLACK,
            Color32::from_gray(55),
            Color32::from_gray(75),
        )
    };
    p.rect_filled(cap, 3.0, c);
    // Upper bevel highlight
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 6.0)),
        3.0,
        hi,
    );
    // Center ergonomic finger grip notch
    let grip = Rect::from_center_size(cap.center(), vec2(cap.width() - 4.0, 3.0));
    p.rect_filled(grip, 1.0, c.gamma_multiply(if white { 0.85 } else { 0.6 }));
    p.rect_stroke(cap, 3.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    changed
}

/// Rotary knob with 3D knurled rim, drop shadow, and crisp pointer notch.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    id: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    name: &str,
) -> bool {
    let hit = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));
    let resp = ui.interact(hit, ui.id().with(id), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() {
        let d = -resp.drag_delta().y * 0.006 * (max - min);
        let nv = (*value + d).clamp(min, max);
        if nv != *value {
            *value = nv;
            changed = true;
        }
    }
    let p = ui.painter();
    let norm = ((*value - min) / (max - min).max(1e-6)).clamp(0.0, 1.0);
    let a0 = 0.75 * std::f32::consts::PI;
    let sweep = 1.5 * std::f32::consts::PI;

    // Ticks around knob
    for i in 0..=10 {
        let a = a0 + sweep * (i as f32 / 10.0);
        let r1 = radius + 3.0;
        let r2 = radius + 5.5;
        p.line_segment(
            [
                pos2(center.x + a.cos() * r1, center.y + a.sin() * r1),
                pos2(center.x + a.cos() * r2, center.y + a.sin() * r2),
            ],
            Stroke::new(1.0_f32, Color32::from_gray(190)),
        );
    }

    // 1. Drop shadow
    p.circle_filled(
        center + vec2(0.0, 2.0),
        radius + 1.5,
        Color32::from_black_alpha(100),
    );

    // 2. Outer knurled bezel ring
    p.circle_filled(center, radius + 0.8, Color32::from_rgb(18, 20, 24));
    p.circle_stroke(
        center,
        radius + 0.8,
        Stroke::new(1.0_f32, Color32::from_rgb(52, 56, 64)),
    );

    // 3. Dial face body
    p.circle_filled(center, radius, Color32::from_rgb(32, 35, 40));
    // Ambient specular sheen (top-left)
    p.circle_filled(
        center - vec2(0.6, 0.8),
        radius * 0.80,
        Color32::from_rgb(46, 50, 56),
    );
    // Center indented cap
    p.circle_filled(center, radius * 0.68, Color32::from_rgb(24, 26, 30));

    // 4. Pointer notch (beveled white indicator line)
    let a = a0 + sweep * norm;
    let p_start = pos2(
        center.x + a.cos() * (radius * 0.22),
        center.y + a.sin() * (radius * 0.22),
    );
    let p_end = pos2(
        center.x + a.cos() * (radius * 0.88),
        center.y + a.sin() * (radius * 0.88),
    );
    // Subtle shadow behind pointer
    p.line_segment(
        [p_start + vec2(0.5, 0.5), p_end + vec2(0.5, 0.5)],
        Stroke::new(2.8_f32, Color32::from_black_alpha(120)),
    );
    // Crisp white pointer
    p.line_segment(
        [p_start, p_end],
        Stroke::new(2.4_f32, Color32::from_rgb(245, 248, 252)),
    );

    if !name.is_empty() {
        label(
            p,
            pos2(center.x, center.y + radius + 13.0),
            name,
            9.5,
            Color32::from_gray(225),
            Align2::CENTER_CENTER,
        );
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::{rect, run_in_ui};

    #[test]
    fn test_drawbar_and_knob_idle_keep_values() {
        run_in_ui(|ui| {
            let mut v = 5_u8;
            assert!(!drawbar(ui, rect(0.0, 0.0, 20.0, 120.0), "d", &mut v, true));
            assert_eq!(v, 5);
            let mut k = 0.25_f32;
            assert!(!knob(
                ui,
                pos2(100.0, 50.0),
                15.0,
                "k",
                &mut k,
                0.0,
                1.0,
                "K"
            ));
            assert_eq!(k, 0.25);
        });
    }
}
