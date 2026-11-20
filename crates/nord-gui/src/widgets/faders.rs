//! Vertical faders: layer strip and organ drawbar.

use super::basic::{led, nord_button};
use crate::style::*;
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};

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

    // Baked Pyramidal Fader Cap
    let cy = fader.bottom() - 8.0 - level.clamp(0.0, 1.0) * (fader.height() - 16.0);
    let cap = Rect::from_center_size(pos2(fader.center().x, cy), vec2(26.0, 20.0));
    let assets = crate::baked::BakedPanelAssets::get_or_load(ui.ctx());
    if !assets.paint_sprite(p, "fader_cap", cap, Color32::WHITE) {
        p.rect_filled(cap, 3.0, Color32::from_gray(160));
    }

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

    // Cap (baked stepped L-profile Hammond drawbar handle)
    let cy = top + 8.0 + (*value as f32 / 8.0) * travel;
    let cap = Rect::from_center_size(pos2(sx - 2.0, cy), vec2(24.0, 56.0));
    let sprite_name = if white {
        "drawbar_white"
    } else {
        "drawbar_black"
    };
    let assets = crate::baked::BakedPanelAssets::get_or_load(ui.ctx());
    if !assets.paint_sprite(p, sprite_name, cap, Color32::WHITE) {
        let c = if white { DRAWBAR_WHITE } else { DRAWBAR_BLACK };
        p.rect_filled(cap, 3.0, c);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;
    use egui::{pos2, vec2, Rect};

    #[test]
    fn test_faders_leave_values_unchanged_without_input() {
        with_ui(|ui| {
            let mut level = 0.5_f32;
            let mut active = true;
            let r1 = Rect::from_min_size(pos2(10.0, 10.0), vec2(34.0, 150.0));
            assert!(!layer_strip(ui, r1, "ls", &mut level, &mut active));
            assert_eq!(level, 0.5);
            let mut bar = 4_u8;
            let r2 = Rect::from_min_size(pos2(60.0, 10.0), vec2(28.0, 150.0));
            assert!(!drawbar(ui, r2, "db", &mut bar, true));
            assert_eq!(bar, 4);
        });
    }
}
