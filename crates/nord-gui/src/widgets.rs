//! Custom Hardware Widgets: LED ladders, drawbars, buttons, knobs, OLEDs, pitch stick & mod wheel.
//!
//! All widgets are rect-based (absolute placement) and return `true` when changed.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use crate::style::*;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
};

/// Draws a text label.
pub fn label(p: &Painter, pos: Pos2, s: &str, size: f32, col: Color32, al: Align2) {
    p.text(pos, al, s, FontId::proportional(size), col);
}

/// Draws a round indicator LED with a dark bezel.
pub fn led(p: &Painter, c: Pos2, on: bool, col_on: Color32, col_off: Color32) {
    p.circle_filled(c, 4.5, Color32::from_rgb(20, 22, 25));
    p.circle_filled(c, 3.3, if on { col_on } else { col_off });
    if on {
        p.circle_stroke(c, 5.5, Stroke::new(1.0_f32, col_on.gamma_multiply(0.35)));
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

/// Recessed pill button with bevel. Returns `true` when clicked.
pub fn nord_button(ui: &mut Ui, rect: Rect, id: &str, grey: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect.expand(2.0), 3.0, BUTTON_BEZEL);
    let base = if grey { BUTTON_GREY } else { BUTTON_BLACK };
    let body = if resp.is_pointer_button_down_on() {
        base.gamma_multiply(0.7)
    } else {
        base
    };
    p.rect_filled(rect, 3.0, body);
    let pill = rect.shrink2(vec2(rect.width() * 0.16, rect.height() * 0.22));
    let hi = if grey {
        Color32::from_gray(190)
    } else {
        Color32::from_gray(52)
    };
    p.rect_filled(pill, pill.height() * 0.5, body.gamma_multiply(0.8));
    p.rect_stroke(
        pill,
        pill.height() * 0.5,
        Stroke::new(1.2_f32, hi),
        StrokeKind::Inside,
    );
    resp.clicked()
}

/// Layer strip: ON/OFF button, vertical fader and 11-segment green ladder.
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
    p.rect_filled(
        Rect::from_min_max(
            pos2(fader.center().x - 2.0, fader.top()),
            pos2(fader.center().x + 2.0, fader.bottom()),
        ),
        1.5,
        Color32::from_rgb(10, 12, 15),
    );
    // Ladder
    for i in 0..11 {
        let y = fader.bottom() - 6.0 - i as f32 * ((fader.height() - 8.0) / 11.0);
        let r = Rect::from_min_size(pos2(fader.right() + 6.0, y - 5.0), vec2(9.0, 5.0));
        let lit = *active && ((*level * 11.0).round() as usize) > i;
        p.rect_filled(r, 1.0, if lit { LED_GREEN } else { LED_GREEN_DIM });
    }
    // Cap
    let cy = fader.bottom() - 8.0 - level.clamp(0.0, 1.0) * (fader.height() - 16.0);
    let cap = Rect::from_center_size(pos2(fader.center().x, cy), vec2(24.0, 16.0));
    p.rect_filled(cap, 3.0, Color32::from_gray(150));
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 4.0)),
        3.0,
        Color32::from_gray(200),
    );
    p.rect_stroke(
        cap,
        3.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );
    // ON LED above button
    led(
        p,
        pos2(btn.left() + 6.0, btn.top() - 7.0),
        *active,
        LED_GREEN,
        LED_GREEN_DIM,
    );
    label(
        p,
        pos2(btn.left() + 14.0, btn.top() - 7.0),
        "ON/OFF",
        8.5,
        Color32::from_gray(225),
        Align2::LEFT_CENTER,
    );
    changed
}

/// Organ drawbar (0 = pushed in at top, 8 = pulled out at bottom) with 8-LED ladder.
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
        p.rect_filled(
            r,
            1.0,
            if (i as u8) < *value {
                LED_RED
            } else {
                LED_RED_DIM
            },
        );
    }
    // Slot
    let sx = rect.right() - 8.0;
    p.rect_filled(
        Rect::from_min_max(pos2(sx - 2.5, top), pos2(sx + 2.5, top + travel + 16.0)),
        1.5,
        Color32::from_rgb(8, 10, 12),
    );
    // Cap
    let cy = top + 8.0 + (*value as f32 / 8.0) * travel;
    let cap = Rect::from_center_size(pos2(sx - 2.0, cy), vec2(26.0, 26.0));
    let (c, edge) = if white {
        (DRAWBAR_WHITE, Color32::from_gray(150))
    } else {
        (DRAWBAR_BLACK, Color32::from_gray(70))
    };
    p.rect_filled(cap, 3.0, c);
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 6.0)),
        3.0,
        c.gamma_multiply(if white { 0.9 } else { 2.0 }),
    );
    p.rect_stroke(cap, 3.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    changed
}

/// Rotary knob. Drag vertically to change. Returns `true` on change.
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
    // Ticks
    for i in 0..=10 {
        let a = a0 + sweep * (i as f32 / 10.0);
        let r1 = radius + 3.0;
        let r2 = radius + 6.0;
        p.line_segment(
            [
                pos2(center.x + a.cos() * r1, center.y + a.sin() * r1),
                pos2(center.x + a.cos() * r2, center.y + a.sin() * r2),
            ],
            Stroke::new(1.0_f32, Color32::from_gray(200)),
        );
    }
    // Body
    p.circle_filled(
        center + vec2(0.0, 1.5),
        radius + 1.0,
        Color32::from_rgb(8, 9, 10),
    );
    p.circle_filled(center, radius, Color32::from_rgb(34, 36, 40));
    p.circle_filled(
        center - vec2(0.0, 1.0),
        radius * 0.78,
        Color32::from_rgb(46, 48, 53),
    );
    p.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 80)),
    );
    // Pointer
    let a = a0 + sweep * norm;
    p.line_segment(
        [
            pos2(
                center.x + a.cos() * radius * 0.2,
                center.y + a.sin() * radius * 0.2,
            ),
            pos2(
                center.x + a.cos() * (radius - 1.5),
                center.y + a.sin() * (radius - 1.5),
            ),
        ],
        Stroke::new(2.5_f32, Color32::WHITE),
    );
    label(
        p,
        pos2(center.x, center.y + radius + 13.0),
        name,
        9.5,
        Color32::from_gray(225),
        Align2::CENTER_CENTER,
    );
    changed
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
        let tri_center = pos2(rect.left() + 10.0, y + row_h * 0.5);
        let col = if is_sel { LED_RED } else { LED_RED_DIM };
        p.text(
            tri_center,
            Align2::CENTER_CENTER,
            "►",
            FontId::proportional(8.0),
            col,
        );
        p.text(
            pos2(rect.left() + 18.0, y + row_h * 0.5),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(8.5),
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
        let tri_center = pos2(rect.right() - 10.0, y + row_h * 0.5);
        let col = if is_sel { LED_RED } else { LED_RED_DIM };
        p.text(
            tri_center,
            Align2::CENTER_CENTER,
            "◄",
            FontId::proportional(8.0),
            col,
        );
        p.text(
            pos2(rect.right() - 18.0, y + row_h * 0.5),
            Align2::RIGHT_CENTER,
            name,
            FontId::proportional(8.5),
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
    fn test_cycle_wraps() {
        assert_eq!(cycle(&[1, 2, 3], 3), 1);
        assert_eq!(cycle(&[1, 2, 3], 1), 2);
        assert_eq!(cycle(&[1, 2, 3], 9), 2);
    }
}
