//! Shared control helpers of the Master and Organ bays: gesture buttons driven by the
//! `nord_panel::behaviour` layer, Shift legends, small LEDs and dimmed "not supported yet"
//! overlays.
//!
//! Every drawn inventory control calls [`mark`] (and [`mark_dim`] when dimmed) so the headless
//! coverage test can prove each id is painted.

use super::prelude::*;
use nord_panel::behaviour::{resolve, ButtonTracker, Gesture, Trigger};
use std::collections::HashMap;

/// Gesture state machines keyed by control id.
pub type Trackers = HashMap<String, ButtonTracker>;

/// Painted-control registry of one frame (test builds only).
#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct Painted {
    pub drawn: Vec<String>,
    pub dimmed: Vec<String>,
}

/// Record that inventory control `id` was drawn this frame.
#[cfg(test)]
pub(super) fn mark(ui: &Ui, id: &str) {
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Painted>(egui::Id::NULL)
            .drawn
            .push(id.to_string());
    });
}

/// Record that inventory control `id` was drawn dimmed (missing engine feature).
#[cfg(test)]
pub(super) fn mark_dim(ui: &Ui, id: &str) {
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Painted>(egui::Id::NULL)
            .dimmed
            .push(id.to_string());
    });
}

/// Registry contents after a frame (test builds only).
#[cfg(test)]
pub(super) fn painted(ctx: &egui::Context) -> Painted {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Painted>(egui::Id::NULL).clone())
}

#[cfg(not(test))]
pub(super) fn mark(_ui: &Ui, _id: &str) {}

#[cfg(not(test))]
pub(super) fn mark_dim(_ui: &Ui, _id: &str) {}

/// Look of one [`gesture_button`].
#[derive(Clone, Copy)]
pub(super) struct Face<'a> {
    /// Matte grey layer button instead of the black pill.
    pub grey: bool,
    /// LED above the button: `Some(lit)`.
    pub led: Option<bool>,
    /// Dimmed with this tooltip: the engine lacks the feature.
    pub dim: Option<&'a str>,
    /// Hover text for working controls (empty = none).
    pub tip: &'a str,
}

impl Face<'_> {
    pub fn black() -> Self {
        Self {
            grey: false,
            led: None,
            dim: None,
            tip: "",
        }
    }
}

/// Small LED (no bloom), `r` px radius.
pub(super) fn led_small(
    p: &Painter,
    c: egui::Pos2,
    on: bool,
    on_col: Color32,
    off_col: Color32,
    r: f32,
) {
    #[cfg(test)]
    crate::widgets::probe::led(p, c, r + 1.0);
    p.circle_filled(c, r + 1.0, Color32::from_rgb(14, 16, 19));
    p.circle_filled(c, r, if on { on_col } else { off_col });
    if on {
        p.circle_stroke(c, r + 1.2, Stroke::new(1.0_f32, on_col.gamma_multiply(0.4)));
    }
}

/// Row of LEDs spread over `rect` (one LED for a lone indicator). `dim` greys the row out with the
/// missing-feature tooltip; the row registers as inventory control `id`.
pub(super) fn led_row(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    lits: &[bool],
    cols: (Color32, Color32),
    dim: Option<&str>,
) {
    let n = lits.len().max(1) as f32;
    let r = (rect.width() / (2.0 * n))
        .min(rect.height() * 0.5)
        .clamp(1.2, 2.2);
    let p = ui.painter().clone();
    for (i, &on) in lits.iter().enumerate() {
        let c = pos2(
            rect.left() + rect.width() * (i as f32 + 0.5) / n,
            rect.center().y,
        );
        led_small(&p, c, on, cols.0, cols.1, r);
    }
    mark(ui, id);
    let resp = ui.interact(rect.expand(1.5), ui.id().with((id, "led")), Sense::hover());
    if let Some(t) = dim {
        p.rect_filled(rect.expand(2.0), 2.0, Color32::from_black_alpha(130));
        mark_dim(ui, id);
        resp.on_hover_text(t);
    }
}

/// "FX FOCUS" caption left of the focus LED at `led`: the LED sits over the level ladder of the
/// last layer with the ON button to its right, so the text goes into the free strip above the
/// faders (never onto the ladder or the button).
pub(super) fn fx_focus_caption(p: &Painter, led: egui::Pos2) {
    label(
        p,
        pos2(led.x - 8.0, led.y),
        "FX FOCUS",
        5.5,
        Color32::from_gray(235),
        Align2::RIGHT_CENTER,
    );
}

/// Tiny silkscreen text centred above `rect`, `lift` px over its top edge.
pub(super) fn tag(p: &Painter, rect: Rect, text: &str, lift: f32) {
    label(
        p,
        pos2(rect.center().x, rect.top() - lift),
        text,
        5.5,
        Color32::from_gray(235),
        Align2::CENTER_BOTTOM,
    );
}

/// Like [`tag`], but a name wider than `max_w` (the room between this control and its
/// neighbour) is broken at its space into two stacked lines instead of running into the
/// neighbour's name.
pub(super) fn tag_fit(p: &Painter, rect: Rect, text: &str, lift: f32, max_w: f32) {
    let split = text.find(' ');
    match split {
        Some(i) if text_width(p, text, 5.5) > max_w => {
            tag(p, rect, &text[i + 1..], lift);
            tag(p, rect, &text[..i], lift + TAG_LINE);
        }
        _ => tag(p, rect, text, lift),
    }
}

/// Pitch of stacked [`tag_fit`] lines (px): the minimum font plus a 2 px gap (audit rule `V`).
const TAG_LINE: f32 = 9.5;

/// Distance (px) from the start of a legend's text to the centre of its state LED.
const LEGEND_LED_GAP: f32 = 5.0;

/// Map a gesture to a trigger; a double tap is just a second primary tap on these buttons.
fn trigger_of(g: Gesture, shift_held: bool, shift_fn: bool) -> Trigger {
    match g {
        Gesture::DoubleTap => Trigger::Primary,
        g => resolve(g, shift_held, shift_fn),
    }
}

/// Feed the pointer state of one button into its tracker; returns the resolved trigger.
fn run_tracker(
    trackers: &mut Trackers,
    id: &str,
    down: bool,
    now_ms: u32,
    shift_held: bool,
    shift_fn: bool,
) -> Option<Trigger> {
    if !trackers.contains_key(id) {
        trackers.insert(id.to_string(), ButtonTracker::new());
    }
    let t = trackers.get_mut(id)?;
    let g = if down && !t.is_down() {
        t.press(now_ms)
    } else if down {
        t.tick(now_ms)
    } else if t.is_down() {
        t.release(now_ms)
    } else {
        None
    };
    g.map(|g| trigger_of(g, shift_held, shift_fn))
}

/// Pill button with tap / hold / Shift resolution (`nord_panel::behaviour`).
///
/// `shift_fn`: the control has a function printed below it (Shift + tap or hold reaches it).
pub(super) fn gesture_button(
    ui: &mut Ui,
    trackers: &mut Trackers,
    rect: Rect,
    id: &str,
    shift_fn: bool,
    face: Face<'_>,
) -> Option<Trigger> {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let now_ms = ui.input(|i| (i.time * 1000.0) as u32);
    let shift_held = ui.input(|i| i.modifiers.shift);
    let out = run_tracker(trackers, id, down, now_ms, shift_held, shift_fn);
    if down {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    let p = ui.painter();
    p.rect_filled(rect.expand(1.5), 2.5, BUTTON_BEZEL);
    let base = if face.grey { BUTTON_GREY } else { BUTTON_BLACK };
    let body = if down { base.gamma_multiply(0.7) } else { base };
    let r = if down {
        rect.translate(vec2(0.0, 0.8))
    } else {
        rect
    };
    p.rect_filled(r, 2.0, body);
    let pill = r.shrink2(vec2(r.width() * 0.16, r.height() * 0.22));
    let hi = if face.grey {
        Color32::from_gray(190)
    } else {
        Color32::from_gray(58)
    };
    p.rect_stroke(
        pill,
        pill.height() * 0.5,
        Stroke::new(1.0_f32, hi),
        StrokeKind::Inside,
    );
    if let Some(lit) = face.led {
        led_small(
            p,
            pos2(rect.center().x, rect.top() - 3.5),
            lit,
            LED_RED,
            LED_RED_DIM,
            1.8,
        );
    }
    mark(ui, id);
    let tip = match face.dim {
        Some(t) => {
            ui.painter()
                .rect_filled(rect.expand(2.0), 3.0, Color32::from_black_alpha(130));
            mark_dim(ui, id);
            t
        }
        None => face.tip,
    };
    if !tip.is_empty() {
        resp.on_hover_text(tip);
    }
    out
}

/// Shift / Hold function legend printed below `base` (the control it hangs off), with an
/// optional state LED. `dim` greys it out and shows the missing-feature tooltip.
pub(super) fn shift_legend(
    ui: &mut Ui,
    base: Rect,
    id: &str,
    text: &str,
    lit: Option<bool>,
    dim: Option<&str>,
) {
    let pos = pos2(base.center().x, base.bottom() + 1.5);
    let col = if dim.is_some() {
        Color32::from_gray(150)
    } else {
        Color32::WHITE
    };
    let p = ui.painter().clone();
    let drawn = label(&p, pos, text, 5.5, col, Align2::CENTER_TOP);
    if let Some(l) = lit {
        // State LED before the text, as printed on the instrument ("o SUSTPED"); keeps it away
        // from a group frame that ends at the right of the legend.
        let x = drawn.left() - LEGEND_LED_GAP;
        led_small(
            &p,
            pos2(x, drawn.center().y),
            l,
            LED_AMBER,
            LED_AMBER_DIM,
            1.4,
        );
    }
    let hit = Rect::from_min_size(pos2(base.center().x - 12.0, base.bottom()), vec2(24.0, 8.0));
    let resp = ui.interact(hit, ui.id().with((id, "legend")), Sense::hover());
    mark(ui, id);
    match dim {
        Some(t) => {
            mark_dim(ui, id);
            resp.on_hover_text(t);
        }
        None => {
            resp.on_hover_text(format!("Shift / hold: {text}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_tap_hold_and_double_tap_resolve() {
        let mut t = Trackers::new();
        assert_eq!(run_tracker(&mut t, "a", true, 0, false, false), None);
        assert_eq!(
            run_tracker(&mut t, "a", false, 50, false, false),
            Some(Trigger::Primary)
        );
        // Second quick press: double tap is a primary tap, not a latch.
        assert_eq!(
            run_tracker(&mut t, "a", true, 100, false, false),
            Some(Trigger::Primary)
        );
        run_tracker(&mut t, "a", false, 120, false, false);
        // Hold: shifted when the control has a shift function, plain hold otherwise.
        run_tracker(&mut t, "b", true, 1000, false, true);
        assert_eq!(
            run_tracker(&mut t, "b", true, 1600, false, true),
            Some(Trigger::Shifted)
        );
        run_tracker(&mut t, "c", true, 1000, false, false);
        assert_eq!(
            run_tracker(&mut t, "c", true, 1600, false, false),
            Some(Trigger::Hold)
        );
    }

    #[test]
    fn shift_modifier_selects_the_shifted_function_on_tap() {
        let mut t = Trackers::new();
        run_tracker(&mut t, "s", true, 0, true, true);
        assert_eq!(
            run_tracker(&mut t, "s", false, 40, true, true),
            Some(Trigger::Shifted)
        );
    }

    #[test]
    fn helpers_paint_shapes_and_register_ids() {
        let ctx = egui::Context::default();
        let mut tr = Trackers::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = Rect::from_min_size(pos2(40.0, 40.0), vec2(20.0, 10.0));
                gesture_button(ui, &mut tr, r, "x.btn", true, Face::black());
                shift_legend(ui, r, "x.shift", "LEG", Some(true), Some("tip"));
                tag(ui.painter(), r, "TAG", 4.0);
                tag_fit(ui.painter(), r, "TWO WORDS", 4.0, 5.0);
                tag_fit(ui.painter(), r, "ONE", 4.0, 5.0);
                let row = Rect::from_min_size(pos2(80.0, 40.0), vec2(24.0, 6.0));
                led_row(
                    ui,
                    row,
                    "x.row",
                    &[true, false, false],
                    (LED_GREEN, LED_GREEN_DIM),
                    Some("d"),
                );
            });
        });
        let p = painted(&ctx);
        assert!(p.drawn.contains(&"x.btn".to_string()));
        assert!(p.dimmed.contains(&"x.shift".to_string()));
        assert!(p.dimmed.contains(&"x.row".to_string()));
    }
}
