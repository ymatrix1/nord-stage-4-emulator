//! Small layout-sized widgets of the Piano bay: gesture button, mini LEDs, LED ladder, fader, dial.
//!
//! The layout rectangles are only 10 to 30 px wide (`nord_panel::layout`, 1.21 px per mm), so the
//! shared widgets in `crate::widgets` (sized for the old hand-placed panel) are not used here.

use super::prelude::*;
use egui::{FontId, Pos2, Response};
use nord_panel::behaviour::{resolve, ButtonTracker, Gesture, Trigger};

/// Dimmed controls (no engine support) are drawn at this brightness.
const DIM: f32 = 0.38;

/// Result of one frame of a gesture button.
pub struct Press {
    /// Gesture completed this frame (`Tap` on release, `Hold` when the threshold passes).
    pub gesture: Option<Gesture>,
    /// The pointer is currently down on the button.
    pub down: bool,
    pub resp: Response,
}

fn now_ms(ui: &Ui) -> u32 {
    (ui.input(|i| i.time) * 1000.0) as u32
}

/// Interact with `rect` and run the button through `tracker` (tap / hold / double tap).
pub fn press(ui: &mut Ui, rect: Rect, id: &str, tracker: &mut ButtonTracker) -> Press {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let t = now_ms(ui);
    let gesture = if down && !tracker.is_down() {
        tracker.press(t)
    } else if down {
        tracker.tick(t)
    } else if tracker.is_down() {
        tracker.release(t)
    } else {
        None
    };
    if down {
        ui.ctx().request_repaint();
    }
    Press {
        gesture,
        down,
        resp,
    }
}

/// What a gesture means. `hold_shifts`: holding reaches the Shift function (manual p.44);
/// otherwise a hold is a hold (layer off, SOLO). A double tap counts as a tap.
pub fn trigger(g: Gesture, shift: bool, has_shift_fn: bool, hold_shifts: bool) -> Trigger {
    match g {
        Gesture::Hold => resolve(g, false, has_shift_fn && hold_shifts),
        Gesture::Tap => resolve(g, shift, has_shift_fn),
        Gesture::DoubleTap => Trigger::Primary,
    }
}

fn tone(c: Color32, dim: bool) -> Color32 {
    if dim {
        c.gamma_multiply(DIM)
    } else {
        c
    }
}

/// Black pill button face; `dim` greys it out.
pub fn face(p: &Painter, rect: Rect, down: bool, dim: bool) {
    let base = tone(BUTTON_BLACK, dim);
    let body = if down { base.gamma_multiply(0.7) } else { base };
    p.rect_filled(rect.expand(1.5), 3.0, tone(BUTTON_BEZEL, dim));
    p.rect_filled(rect, 2.5, body);
    let pill = rect.shrink2(vec2(rect.width() * 0.14, rect.height() * 0.2));
    p.rect_stroke(
        pill,
        pill.height() * 0.5,
        Stroke::new(1.0_f32, tone(Color32::from_gray(70), dim)),
        StrokeKind::Inside,
    );
}

/// Tiny silkscreen caption centred under `rect` (Shift legends and value read-outs).
pub fn legend(p: &Painter, rect: Rect, text: &str, dim: bool) {
    label(
        p,
        pos2(rect.center().x, rect.bottom() + 7.0),
        text,
        6.0,
        tone(Color32::from_gray(225), dim),
        Align2::CENTER_CENTER,
    );
}

/// Button label inside the face of `rect`.
pub fn inside(p: &Painter, rect: Rect, text: &str, dim: bool) {
    label(
        p,
        rect.center(),
        text,
        6.5,
        tone(Color32::from_gray(235), dim),
        Align2::CENTER_CENTER,
    );
}

/// Caption centred above `rect`.
pub fn caption(p: &Painter, rect: Rect, text: &str, dim: bool) {
    label(
        p,
        pos2(rect.center().x, rect.top() - 7.0),
        text,
        6.0,
        tone(Color32::from_gray(225), dim),
        Align2::CENTER_CENTER,
    );
}

/// Small round LED of radius `r`.
pub fn mini_led(p: &Painter, c: Pos2, on: bool, col: Color32, col_off: Color32, r: f32, dim: bool) {
    #[cfg(test)]
    crate::widgets::probe::led(p, c, r + 1.0);
    p.circle_filled(c, r + 1.0, tone(Color32::from_rgb(14, 16, 19), dim));
    p.circle_filled(c, r, tone(if on { col } else { col_off }, dim));
    if on && !dim {
        p.circle_stroke(c, r + 1.6, Stroke::new(1.0_f32, col.gamma_multiply(0.4)));
    }
}

/// `n` LEDs evenly spaced along the long side of `rect`; `lit(i)` says which are on.
pub fn led_strip(
    p: &Painter,
    rect: Rect,
    n: usize,
    lit: impl Fn(usize) -> bool,
    col: (Color32, Color32),
    dim: bool,
) {
    let vertical = rect.height() >= rect.width();
    let step = if vertical {
        rect.height()
    } else {
        rect.width()
    } / n.max(1) as f32;
    let r = (step * 0.42).clamp(1.2, 3.0);
    for i in 0..n {
        let off = (i as f32 + 0.5) * step;
        let c = if vertical {
            pos2(rect.center().x, rect.top() + off)
        } else {
            pos2(rect.left() + off, rect.center().y)
        };
        mini_led(p, c, lit(i), col.0, col.1, r, dim);
    }
}

/// Segmented level ladder (`lit` of `n` segments, green to red).
pub fn ladder(p: &Painter, rect: Rect, n: usize, lit: usize) {
    let seg_h = rect.height() / n as f32;
    for i in 0..n {
        let on = i < lit;
        let col = if i + 1 >= n {
            (LED_RED, LED_RED_DIM)
        } else if i + 3 >= n {
            (LED_AMBER, LED_AMBER_DIM)
        } else {
            (LED_GREEN, LED_GREEN_DIM)
        };
        let y = rect.bottom() - (i as f32 + 1.0) * seg_h;
        let r = Rect::from_min_size(pos2(rect.left(), y + 0.5), vec2(rect.width(), seg_h - 1.0));
        p.rect_filled(r, 0.8, if on { col.0 } else { col.1 });
    }
}

/// Vertical fader in `rect`. Returns the new level (0..=1) while dragged or clicked.
pub fn fader(ui: &mut Ui, rect: Rect, id: &str, level: f32) -> (Option<f32>, Response) {
    let resp = ui.interact(
        rect.expand2(vec2(3.0, 0.0)),
        ui.id().with(id),
        Sense::click_and_drag(),
    );
    let travel = (rect.height() - 8.0).max(1.0);
    let mut out = None;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            out = Some((1.0 - (pos.y - rect.top() - 4.0) / travel).clamp(0.0, 1.0));
        }
    }
    let shown = out.unwrap_or(level).clamp(0.0, 1.0);
    #[cfg(test)]
    crate::widgets::probe::cap_travel(
        ui.ctx(),
        Rect::from_center_size(rect.center(), vec2(rect.width() + 4.0, rect.height() + 3.5)),
        id,
    );
    let p = ui.painter();
    let slot = Rect::from_center_size(rect.center(), vec2(2.5, rect.height()));
    p.rect_filled(slot, 1.0, Color32::from_rgb(10, 12, 15));
    let cy = rect.bottom() - 4.0 - shown * travel;
    let cap = Rect::from_center_size(pos2(rect.center().x, cy), vec2(rect.width() + 4.0, 7.0));
    p.rect_filled(cap, 2.0, Color32::from_gray(165));
    p.rect_filled(
        Rect::from_min_size(cap.min, vec2(cap.width(), 2.0)),
        1.0,
        Color32::from_gray(225),
    );
    p.line_segment(
        [
            pos2(cap.left() + 1.5, cap.center().y),
            pos2(cap.right() - 1.5, cap.center().y),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(25, 28, 32)),
    );
    (out, resp)
}

/// What the PIANO SELECT dial did this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DialMove {
    /// Signed detents turned (scroll wheel or vertical drag).
    pub steps: i32,
    /// Plain click (no turn).
    pub clicked: bool,
}

/// Rotary dial: scroll or drag vertically to turn, click to step forward. `pos` is the
/// pointer position fraction 0..1 for the marker; `accum` carries partial drag between frames.
pub fn dial(ui: &mut Ui, rect: Rect, id: &str, pos: f32, accum: &mut f32) -> (DialMove, Response) {
    const DRAG_PX_PER_STEP: f32 = 10.0;
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    let mut mv = DialMove::default();
    let mut delta = if resp.dragged() {
        -resp.drag_delta().y
    } else {
        0.0
    };
    if resp.hovered() {
        delta += ui.input(|i| i.raw_scroll_delta.y) / 4.0;
    }
    *accum += delta;
    while accum.abs() >= DRAG_PX_PER_STEP {
        mv.steps += accum.signum() as i32;
        *accum -= accum.signum() * DRAG_PX_PER_STEP;
    }
    mv.clicked = resp.clicked();
    let p = ui.painter();
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    p.circle_filled(c, r + 1.0, BUTTON_BEZEL);
    p.circle_filled(c, r, Color32::from_gray(150));
    p.circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_gray(60)));
    let ang = (-135.0 + 270.0 * pos.clamp(0.0, 1.0)).to_radians();
    let tip = c + vec2(ang.sin(), -ang.cos()) * (r * 0.8);
    p.line_segment(
        [c, tip],
        Stroke::new(1.5_f32, Color32::from_rgb(25, 28, 32)),
    );
    (mv, resp)
}

/// Draw `text` with the small bay font (overlay lines).
pub fn small_text(p: &Painter, pos: Pos2, text: &str, col: Color32, al: Align2) {
    p.text(pos, al, text, FontId::monospace(8.0), col);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_ui(f: impl FnOnce(&mut Ui)) -> usize {
        let ctx = egui::Context::default();
        let mut f = Some(f);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(f) = f.take() {
                    f(ui);
                }
            });
        });
        out.shapes.len()
    }

    #[test]
    fn trigger_maps_tap_shift_and_hold() {
        assert_eq!(trigger(Gesture::Tap, false, true, true), Trigger::Primary);
        assert_eq!(trigger(Gesture::Tap, true, true, true), Trigger::Shifted);
        assert_eq!(trigger(Gesture::Tap, true, false, true), Trigger::Primary);
        assert_eq!(trigger(Gesture::Hold, false, true, true), Trigger::Shifted);
        assert_eq!(trigger(Gesture::Hold, false, true, false), Trigger::Hold);
        assert_eq!(trigger(Gesture::Hold, false, false, true), Trigger::Hold);
        assert_eq!(
            trigger(Gesture::DoubleTap, false, true, true),
            Trigger::Primary
        );
    }

    #[test]
    fn idle_press_reports_nothing_and_paints_nothing_by_itself() {
        with_ui(|ui| {
            let mut tr = ButtonTracker::new();
            let r = Rect::from_min_size(pos2(5.0, 5.0), vec2(20.0, 12.0));
            let pr = press(ui, r, "b", &mut tr);
            assert!(pr.gesture.is_none() && !pr.down);
        });
    }

    #[test]
    fn widgets_paint_shapes() {
        let n = with_ui(|ui| {
            let p = ui.painter().clone();
            let r = Rect::from_min_size(pos2(5.0, 5.0), vec2(20.0, 60.0));
            face(&p, r, false, false);
            ladder(&p, r, 11, 4);
            led_strip(&p, r, 3, |i| i == 1, (LED_GREEN, LED_GREEN_DIM), true);
            legend(&p, r, "X", false);
            caption(&p, r, "Y", false);
            let (lvl, _) = fader(ui, r, "f", 0.5);
            assert!(lvl.is_none());
            let mut acc = 0.0;
            let (mv, _) = dial(
                ui,
                Rect::from_min_size(pos2(40.0, 5.0), vec2(16.0, 16.0)),
                "d",
                0.3,
                &mut acc,
            );
            assert_eq!(mv, DialMove::default());
        });
        assert!(n > 25, "only {n} shapes");
    }

    #[test]
    fn dimming_darkens_colours() {
        assert!(tone(LED_GREEN, true).g() < LED_GREEN.g());
        assert_eq!(tone(LED_GREEN, false), LED_GREEN);
    }
}
