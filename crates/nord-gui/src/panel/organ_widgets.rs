//! Layout-sized widgets of the Organ strip: level fader, LED ladders and the slim drawbar.
//!
//! Sizes come from `nord_panel::layout` rectangles (about 12 x 57 px fader, 6.7 x 64 px drawbar
//! slot), so these differ from the larger standalone widgets in `crate::widgets`.

use super::ctl::{led_small, mark};
use super::prelude::*;

/// Vertical level fader in a slot filling `rect`. Returns `true` when the level changed.
pub(super) fn level_fader(ui: &mut Ui, rect: Rect, id: &str, level: &mut f32) -> bool {
    let hit = rect.expand2(vec2(3.0, 3.0));
    let resp = ui.interact(hit, ui.id().with(id), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let n = 1.0 - ((pos.y - rect.top() - 4.0) / (rect.height() - 8.0)).clamp(0.0, 1.0);
            *level = n;
            changed = true;
        }
    }
    #[cfg(test)]
    crate::widgets::probe::cap_travel(
        ui.ctx(),
        Rect::from_min_max(
            pos2(rect.center().x - rect.width() * 0.5 - 1.0, rect.top()),
            pos2(
                rect.center().x + rect.width() * 0.5 + 1.0,
                rect.bottom() + 1.5,
            ),
        ),
        id,
    );
    let p = ui.painter();
    let slot = Rect::from_center_size(rect.center(), vec2(3.0, rect.height()));
    p.rect_filled(slot, 1.5, Color32::from_rgb(10, 12, 15));
    let cy = rect.bottom() - 4.0 - level.clamp(0.0, 1.0) * (rect.height() - 8.0);
    let cap = Rect::from_center_size(pos2(rect.center().x, cy), vec2(rect.width() + 2.0, 8.0));
    p.rect_filled(
        cap.translate(vec2(0.0, 1.5)),
        2.0,
        Color32::from_black_alpha(100),
    );
    p.rect_filled(cap, 2.0, Color32::from_gray(160));
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 2.5)),
        2.0,
        Color32::from_gray(220),
    );
    p.line_segment(
        [
            pos2(cap.left() + 1.5, cap.center().y),
            pos2(cap.right() - 1.5, cap.center().y),
        ],
        Stroke::new(1.2_f32, Color32::from_rgb(25, 28, 32)),
    );
    p.rect_stroke(
        cap,
        2.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );
    mark(ui, id);
    changed
}

/// Number of lit segments of an 11-segment level ladder (`level` in `0..=1`).
pub(super) fn ladder_lit(level: f32, active: bool) -> usize {
    if active {
        (level.clamp(0.0, 1.0) * 11.0).round() as usize
    } else {
        0
    }
}

/// 11-segment level ladder (green, amber, red) filling `rect`, lit from the bottom.
pub(super) fn level_ladder(ui: &Ui, rect: Rect, id: &str, level: f32, active: bool) {
    let p = ui.painter();
    let lit = ladder_lit(level, active);
    let h = rect.height() / 11.0;
    for i in 0..11 {
        let r = Rect::from_min_size(
            pos2(rect.left(), rect.bottom() - (i as f32 + 1.0) * h + 0.5),
            vec2(rect.width(), h - 1.0),
        );
        let on = i < lit;
        let col = match (i, on) {
            (10, true) => LED_RED,
            (10, false) => LED_RED_DIM,
            (8 | 9, true) => LED_AMBER,
            (8 | 9, false) => LED_AMBER_DIM,
            (_, true) => LED_GREEN,
            (_, false) => LED_GREEN_DIM,
        };
        p.rect_filled(r, 1.0, col);
    }
    mark(ui, id);
}

/// 8-LED drawbar ladder in `rect`; `lit` LEDs from the top (none in Drawbar Live).
pub(super) fn drawbar_ladder(ui: &Ui, rect: Rect, id: &str, lit: u8) {
    let p = ui.painter();
    let pitch = rect.height() / 8.0;
    let r = (rect.width() * 0.5).min(pitch * 0.5) - 1.0;
    for i in 0..8 {
        let c = pos2(rect.center().x, rect.top() + (i as f32 + 0.5) * pitch);
        led_small(p, c, (i as u8) < lit, LED_RED, LED_RED_DIM, r.max(1.2));
    }
    mark(ui, id);
}

/// Drawbar position from a pointer `y` over the slot (`0` pushed in at the top, `8` pulled out).
pub(super) fn drawbar_value_at(slot: Rect, y: f32) -> u8 {
    let n = ((y - slot.top()) / slot.height()).clamp(0.0, 1.0);
    (n * 8.0).round() as u8
}

/// Slim drawbar: slot, molded handle (`white` = white cap). Returns `true` on change.
pub(super) fn drawbar_slot(ui: &mut Ui, slot: Rect, id: &str, value: &mut u8, white: bool) -> bool {
    let hit = slot.expand2(vec2(7.0, 6.0));
    let resp = ui.interact(hit, ui.id().with(id), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let v = drawbar_value_at(slot, pos.y);
            if v != *value {
                *value = v;
                changed = true;
            }
        }
    }
    #[cfg(test)]
    crate::widgets::probe::cap_travel(
        ui.ctx(),
        Rect::from_min_max(
            pos2(slot.left() - 3.5, slot.top() - 4.5),
            pos2(slot.right() + 3.5, slot.bottom() + 6.0),
        ),
        id,
    );
    let p = ui.painter();
    p.rect_filled(slot, 1.5, Color32::from_rgb(8, 10, 12));
    p.rect_stroke(
        slot,
        1.5,
        Stroke::new(1.0_f32, Color32::from_rgb(25, 28, 32)),
        StrokeKind::Inside,
    );
    let cy = slot.top() + f32::from(*value).min(8.0) / 8.0 * slot.height();
    let cap = Rect::from_center_size(pos2(slot.center().x, cy), vec2(slot.width() + 7.0, 9.0));
    let (c, edge, hi) = if white {
        (DRAWBAR_WHITE, Color32::from_gray(140), Color32::WHITE)
    } else {
        (
            DRAWBAR_BLACK,
            Color32::from_gray(55),
            Color32::from_gray(75),
        )
    };
    p.rect_filled(
        cap.translate(vec2(0.0, 1.5)),
        2.0,
        Color32::from_black_alpha(100),
    );
    p.rect_filled(cap, 2.0, c);
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 2.5)),
        2.0,
        hi,
    );
    p.rect_stroke(cap, 2.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    mark(ui, id);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_lit_counts_segments() {
        assert_eq!(ladder_lit(0.0, true), 0);
        assert_eq!(ladder_lit(1.0, true), 11);
        assert_eq!(ladder_lit(0.5, true), 6);
        assert_eq!(ladder_lit(1.0, false), 0);
    }

    #[test]
    fn drawbar_value_spans_zero_to_eight() {
        let slot = Rect::from_min_size(pos2(0.0, 100.0), vec2(6.0, 64.0));
        assert_eq!(drawbar_value_at(slot, 0.0), 0);
        assert_eq!(drawbar_value_at(slot, 132.0), 4);
        assert_eq!(drawbar_value_at(slot, 400.0), 8);
    }

    #[test]
    fn widgets_paint_without_input() {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut lvl = 0.5;
                let mut v = 3u8;
                let r = Rect::from_min_size(pos2(20.0, 20.0), vec2(12.0, 57.0));
                assert!(!level_fader(ui, r, "f", &mut lvl));
                assert!(!drawbar_slot(
                    ui,
                    r.translate(vec2(30.0, 0.0)),
                    "d",
                    &mut v,
                    true
                ));
                level_ladder(ui, r.translate(vec2(60.0, 0.0)), "l", 0.5, true);
                drawbar_ladder(ui, r.translate(vec2(80.0, 0.0)), "dl", 3);
                assert_eq!((lvl, v), (0.5, 3));
            });
        });
        assert!(out.shapes.len() > 30);
    }
}
