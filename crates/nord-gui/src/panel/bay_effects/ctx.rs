//! Drawing and input helpers shared by the Layer Effects units: mm-layout placed knobs, gesture
//! buttons with hold-to-shift, LED rows with per-LED tooltips and the dimming of controls the
//! engine cannot honour.

use super::ids::draw_rect;
use crate::audio::{AudioCommand, AudioHandle, FxEdit};
use crate::style::*;
use crate::widgets::{knob, label};
use egui::{pos2, vec2, Align2, Color32, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use nord_panel::behaviour::{resolve, ButtonTracker, Gesture, Trigger};
use std::collections::HashMap;

/// Frame context of the bay: where the panel is, what time it is, whether Shift is down.
pub(super) struct Fx<'a> {
    pub ui: &'a mut Ui,
    pub p: &'a Painter,
    pub origin: Pos2,
    pub audio: &'a AudioHandle,
    pub now_ms: u32,
    pub shift: bool,
    /// Gesture trackers per button id (moved out of `FxUi` for the frame).
    pub trackers: HashMap<&'static str, ButtonTracker>,
}

/// Colour of a dimmed (unsupported) control overlay.
const DIM: Color32 = Color32::from_black_alpha(140);
/// Caption colour (silkscreen white-grey).
pub(super) const CAPTION: Color32 = Color32::from_gray(215);
/// Shifted-function legend colour, lit while the shifted function is active.
const LEGEND: Color32 = Color32::from_gray(150);
const LEGEND_ON: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x20);

/// Maps `v` in `min..=max` to `0..=1` on a log scale (rates, delay time).
pub(super) fn to_log(v: f32, min: f32, max: f32) -> f32 {
    ((v.clamp(min, max) / min).ln() / (max / min).ln()).clamp(0.0, 1.0)
}

/// Inverse of [`to_log`].
pub(super) fn from_log(n: f32, min: f32, max: f32) -> f32 {
    min * (max / min).powf(n.clamp(0.0, 1.0))
}

/// One frame of a button's gesture state machine (`nord_panel::behaviour`): returns what the
/// press means once SHIFT (held key, or hold-to-shift after half a second) is applied.
///
/// A double tap is a second primary press; a hold on a button without a shifted function is
/// treated as a plain press so a slow click is never lost.
pub(super) fn gesture_step(
    tracker: &mut ButtonTracker,
    down: bool,
    clicked: bool,
    t: u32,
    shift: bool,
    has_shift_fn: bool,
) -> Option<Trigger> {
    let was_down = tracker.is_down();
    let mut g = None;
    if down && !was_down {
        g = tracker.press(t);
    }
    if down {
        g = g.or(tracker.tick(t));
    } else if was_down {
        g = tracker.release(t);
    } else if clicked {
        g = Some(Gesture::Tap);
    }
    g.map(|g| match resolve(g, shift, has_shift_fn) {
        Trigger::Shifted => Trigger::Shifted,
        _ => Trigger::Primary,
    })
}

impl Fx<'_> {
    /// Rectangle of control `id` in screen pixels.
    pub fn rect(&self, id: &str) -> Rect {
        draw_rect(id, self.origin)
    }

    /// Queue engine edits.
    pub fn send(&self, edits: Vec<FxEdit>) {
        for e in edits {
            self.audio.send_command(AudioCommand::Fx(e));
        }
    }

    /// Hover text for an arbitrary rectangle.
    pub fn hover(&mut self, rect: Rect, key: &str, text: &str) {
        let r = self
            .ui
            .interact(rect, self.ui.id().with((key, "tip")), Sense::hover());
        r.on_hover_text(text.to_string());
    }

    /// Dim a control the engine cannot honour right now.
    pub fn dim(&self, rect: Rect) {
        self.p.rect_filled(rect.expand(2.0), 3.0, DIM);
    }

    /// Small silkscreen caption `line` rows below `rect`.
    pub fn caption(&self, rect: Rect, line: usize, text: &str, col: Color32) {
        let c = pos2(rect.center().x, rect.bottom() + 5.0 + 7.5 * line as f32);
        label(self.p, c, text, 6.5, col, Align2::CENTER_CENTER);
    }

    /// Caption of a knob or button (line 0).
    pub fn name(&self, id: &str, text: &str) {
        self.caption(self.rect(id), 0, text, CAPTION);
    }

    /// Legend of a shifted function at its own rect; `active` lights it, `dim` explains why the
    /// engine cannot honour it.
    pub fn legend(
        &mut self,
        id: &'static str,
        text: &str,
        active: bool,
        tip: &str,
        dim: Option<&str>,
    ) {
        let r = self.rect(id);
        let col = if active { LEGEND_ON } else { LEGEND };
        label(self.p, r.center(), text, 6.0, col, Align2::CENTER_CENTER);
        let full = match dim {
            Some(why) => format!("{tip}\nNot available: {why}"),
            None => tip.to_string(),
        };
        self.hover(r, id, &full);
        if dim.is_some() {
            self.dim(r);
        }
    }

    /// Replace the value text of the parameter hint of knob `id` (the knob widget only knows a
    /// plain position; steps, clock divisions and log sweeps with a per-type range say it here).
    pub fn hint(&self, id: &str, value: &str) {
        crate::panel::param_hint::touch_value(self.ui.ctx(), id, value);
    }

    /// Knob at the layout rect of `id`. A dimmed knob is drawn and explained but not edited.
    pub fn knob(
        &mut self,
        id: &'static str,
        value: &mut f32,
        range: (f32, f32),
        tip: &str,
        dim: Option<&str>,
    ) -> bool {
        let r = self.rect(id);
        let radius = (r.width().min(r.height()) * 0.5).max(5.0);
        let mut shown = *value;
        let target = if dim.is_some() { &mut shown } else { value };
        let changed = knob(
            self.ui,
            r.center(),
            radius,
            id,
            target,
            range.0,
            range.1,
            "",
        );
        let text = match dim {
            Some(why) => format!("{tip}\nNot available: {why}"),
            None => tip.to_string(),
        };
        self.hover(r, id, &text);
        if dim.is_some() {
            self.dim(r);
        }
        changed && dim.is_none()
    }

    /// Gesture button at `rect`; `lit` shows it pressed in, `has_shift_fn` makes a hold a shift
    /// press (manual p.44).
    pub fn button(
        &mut self,
        id: &'static str,
        rect: Rect,
        lit: bool,
        has_shift_fn: bool,
        tip: &str,
    ) -> Option<Trigger> {
        let resp = self
            .ui
            .interact(rect, self.ui.id().with((id, "btn")), Sense::click());
        let down = resp.is_pointer_button_down_on();
        paint_pill(self.p, rect, lit, down);
        if down {
            self.ui.ctx().request_repaint();
        }
        let clicked = resp.clicked();
        resp.on_hover_text(tip.to_string());
        let tracker = self.trackers.entry(id).or_default();
        gesture_step(
            tracker,
            down,
            clicked,
            self.now_ms,
            self.shift,
            has_shift_fn,
        )
    }

    /// Row of mini LEDs centred above `rect`: `(name, lit, missing)` per LED. An LED whose type
    /// the engine lacks stays dark amber-grey and its tooltip names what is missing.
    pub fn led_row(&mut self, id: &'static str, rect: Rect, leds: &[(&str, bool, &str)]) {
        let n = leds.len() as f32;
        let step = 5.0_f32;
        let x0 = rect.center().x - step * (n - 1.0) * 0.5;
        let y = rect.top() - 4.0;
        for (i, (name, lit, missing)) in leds.iter().enumerate() {
            let c = pos2(x0 + step * i as f32, y);
            let supported = missing.is_empty();
            let (col, off) = if supported {
                (LED_RED, LED_RED_DIM)
            } else {
                (LED_AMBER, Color32::from_gray(52))
            };
            mini_led(self.p, c, *lit && supported, col, off);
            let hit = Rect::from_center_size(c, vec2(step, 6.0));
            let text = if supported {
                (*name).to_string()
            } else {
                format!("{name}: not available, {missing}")
            };
            self.hover(hit, &format!("{id}{i}"), &text);
        }
    }

    /// Single LED just above the top edge of `rect`.
    pub fn led_above(&self, rect: Rect, on: bool, col: Color32, dim_col: Color32) {
        mini_led(
            self.p,
            pos2(rect.center().x, rect.top() - 4.0),
            on,
            col,
            dim_col,
        );
    }
}

/// Pill button body (same look as `widgets::nord_button`, but the caller owns the interaction).
pub(super) fn paint_pill(p: &Painter, rect: Rect, lit: bool, down: bool) {
    p.rect_filled(rect.expand(2.0), 3.5, BUTTON_BEZEL);
    let base = if lit { BUTTON_GREY } else { BUTTON_BLACK };
    let body = if down { base.gamma_multiply(0.7) } else { base };
    let r = if down {
        rect.translate(vec2(0.0, 1.0))
    } else {
        rect
    };
    p.rect_filled(r, 3.0, body);
    let pill = r.shrink2(vec2(r.width() * 0.16, r.height() * 0.22));
    let hi = if lit {
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
}

/// Mini indicator LED (the panel's LEDs are about 3 px; `widgets::led` is too large here).
pub(super) fn mini_led(p: &Painter, c: Pos2, on: bool, col: Color32, dim_col: Color32) {
    p.circle_filled(c, 2.6, Color32::from_rgb(14, 16, 19));
    p.circle_filled(c, 1.9, if on { col } else { dim_col });
    if on {
        p.circle_stroke(c, 2.9, Stroke::new(1.0_f32, col.gamma_multiply(0.4)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_mapping_round_trips_and_hits_the_ends() {
        assert!((to_log(0.1, 0.1, 20.0)).abs() < 1e-6);
        assert!((to_log(20.0, 0.1, 20.0) - 1.0).abs() < 1e-6);
        for v in [0.1_f32, 0.8, 4.0, 20.0] {
            let back = from_log(to_log(v, 0.1, 20.0), 0.1, 20.0);
            assert!((back - v).abs() / v < 1e-4, "{v} -> {back}");
        }
        assert_eq!(from_log(7.0, 20.0, 1500.0), 1500.0, "clamped above");
    }

    #[test]
    fn short_press_is_primary_and_fires_on_release() {
        let mut t = ButtonTracker::new();
        assert_eq!(gesture_step(&mut t, true, false, 0, false, true), None);
        assert_eq!(
            gesture_step(&mut t, false, true, 100, false, true),
            Some(Trigger::Primary)
        );
    }

    #[test]
    fn held_shift_key_makes_a_tap_shifted_only_when_a_shifted_function_exists() {
        let mut t = ButtonTracker::new();
        gesture_step(&mut t, true, false, 0, true, true);
        assert_eq!(
            gesture_step(&mut t, false, true, 80, true, true),
            Some(Trigger::Shifted)
        );
        let mut t = ButtonTracker::new();
        gesture_step(&mut t, true, false, 0, true, false);
        assert_eq!(
            gesture_step(&mut t, false, true, 80, true, false),
            Some(Trigger::Primary)
        );
    }

    #[test]
    fn hold_for_half_a_second_is_hold_to_shift() {
        let mut t = ButtonTracker::new();
        gesture_step(&mut t, true, false, 0, false, true);
        assert_eq!(gesture_step(&mut t, true, false, 499, false, true), None);
        assert_eq!(
            gesture_step(&mut t, true, false, 500, false, true),
            Some(Trigger::Shifted)
        );
        // the release after a hold does not fire a second time
        assert_eq!(gesture_step(&mut t, false, true, 900, false, true), None);
    }

    #[test]
    fn hold_on_a_plain_button_is_a_plain_press() {
        let mut t = ButtonTracker::new();
        gesture_step(&mut t, true, false, 0, false, false);
        assert_eq!(
            gesture_step(&mut t, true, false, 600, false, false),
            Some(Trigger::Primary)
        );
    }

    #[test]
    fn a_click_seen_without_a_press_frame_is_a_tap() {
        let mut t = ButtonTracker::new();
        assert_eq!(
            gesture_step(&mut t, false, true, 10, false, true),
            Some(Trigger::Primary)
        );
        assert_eq!(gesture_step(&mut t, false, false, 20, false, true), None);
    }

    #[test]
    fn second_quick_press_is_a_second_primary() {
        let mut t = ButtonTracker::new();
        gesture_step(&mut t, true, false, 0, false, true);
        gesture_step(&mut t, false, true, 50, false, true);
        assert_eq!(
            gesture_step(&mut t, true, false, 150, false, true),
            Some(Trigger::Primary)
        );
    }
}
