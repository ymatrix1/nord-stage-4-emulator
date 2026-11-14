//! Panorama view (B12): a strip with the whole instrument and a draggable viewport rectangle.
//!
//! The rectangle is the part of the panel the detailed view shows ([`ViewState::panorama`], in
//! logical panel px). Dragging the strip pans it, the wheel over the strip and `+` / `-` zoom it,
//! the arrow keys pan it. The strip draws section outlines only (no bay code runs twice), so its
//! cost is a handful of shapes and only while the mode is on. Pure math is separate from the
//! egui code so it is unit-tested without a context.

use super::view::{section_px, ViewState, FOCUSABLE, PANEL_SIZE};
use crate::style::{CHASSIS_DARK, CHASSIS_RED, LED_AMBER, MODULE_SLATE};
use egui::{pos2, vec2, Context, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Logical height of the strip (it scales with the zoom like the rest of the view).
pub const STRIP_H: f32 = 56.0;
/// Logical gap between the detailed view and the strip.
pub const STRIP_GAP: f32 = 4.0;
/// Width of the strip: the panel aspect at [`STRIP_H`].
pub const STRIP_W: f32 = STRIP_H * PANEL_SIZE.x / PANEL_SIZE.y;
/// Smallest viewport as a fraction of the panel (the detailed view caps at `MAX_ZOOM` anyway).
pub const MIN_SCALE: f32 = 0.25;
/// Viewport scale chosen when the mode opens from the overview (2x on the panel).
pub const DEFAULT_SCALE: f32 = 0.5;
/// Factor of one `+` / `-` key press or wheel notch.
const ZOOM_STEP: f32 = 1.25;
/// Fraction of the viewport an arrow key press pans.
const PAN_STEP: f32 = 0.15;

/// Rectangle of the strip for a detailed view occupying `visible` (below it, left-aligned).
pub fn strip_rect(visible: Rect) -> Rect {
    Rect::from_min_size(
        pos2(visible.min.x, visible.max.y + STRIP_GAP),
        vec2(STRIP_W, STRIP_H),
    )
}

/// Extra logical height the strip adds under the detailed view.
pub const fn strip_extra_h() -> f32 {
    STRIP_H + STRIP_GAP
}

fn panel_rect() -> Rect {
    Rect::from_min_size(Pos2::ZERO, PANEL_SIZE)
}

/// Size `r` to a scale in [`MIN_SCALE`], 1 of the panel (keeping the panel aspect) and shift it
/// fully inside the panel, keeping its centre where possible.
pub fn clamp_viewport(r: Rect) -> Rect {
    let scale = (r.width() / PANEL_SIZE.x).clamp(MIN_SCALE, 1.0);
    let size = PANEL_SIZE * scale;
    let max = PANEL_SIZE - size;
    let min = pos2(
        (r.center().x - size.x / 2.0).clamp(0.0, max.x),
        (r.center().y - size.y / 2.0).clamp(0.0, max.y),
    );
    Rect::from_min_size(min, size)
}

/// Viewport of the same size as `r`, centred on `c` (panel px) and clamped into the panel.
pub fn centered(r: Rect, c: Pos2) -> Rect {
    clamp_viewport(Rect::from_center_size(c, r.size()))
}

/// `r` scaled by `factor` (above 1 shows more) about its centre.
pub fn zoomed(r: Rect, factor: f32) -> Rect {
    clamp_viewport(Rect::from_center_size(r.center(), r.size() * factor))
}

/// `r` moved by `delta` panel px.
pub fn panned(r: Rect, delta: Vec2) -> Rect {
    clamp_viewport(r.translate(delta))
}

/// Panel position under `p` on a strip drawn at `strip`.
pub fn strip_to_panel(strip: Rect, p: Pos2) -> Pos2 {
    let t = (p - strip.min) / strip.size();
    pos2(t.x * PANEL_SIZE.x, t.y * PANEL_SIZE.y)
}

/// Position on a strip drawn at `strip` of panel position `p`.
pub fn panel_to_strip(strip: Rect, p: Pos2) -> Pos2 {
    strip.min + vec2(p.x / PANEL_SIZE.x, p.y / PANEL_SIZE.y) * strip.size()
}

fn rect_to_strip(strip: Rect, r: Rect) -> Rect {
    Rect::from_min_max(panel_to_strip(strip, r.min), panel_to_strip(strip, r.max))
}

/// Viewport the mode opens with: the focused section, else a centred 2x window.
pub fn initial_viewport(view: &ViewState) -> Rect {
    match view.focus {
        Some(s) => clamp_viewport(Rect::from_center_size(
            section_px(s, Pos2::ZERO).center(),
            PANEL_SIZE * DEFAULT_SCALE,
        )),
        None => centered(
            Rect::from_min_size(Pos2::ZERO, PANEL_SIZE * DEFAULT_SCALE),
            panel_rect().center(),
        ),
    }
}

impl ViewState {
    /// P: leave the panorama, or enter it from the current view.
    pub fn toggle_panorama(&mut self) {
        if self.panorama.take().is_none() {
            self.panorama = Some(initial_viewport(self));
            self.focus = None;
        }
    }

    /// Pan / zoom the viewport; no-op outside the panorama.
    pub fn adjust_viewport(&mut self, f: impl FnOnce(Rect) -> Rect) {
        if let Some(r) = self.panorama {
            self.panorama = Some(f(r));
        }
    }

    /// `+` `-` zoom and the arrow keys pan the viewport. Returns `true` when it moved.
    pub fn handle_viewport_keys(&mut self, ctx: &Context) -> bool {
        let Some(before) = self.panorama else {
            return false;
        };
        let (zoom_in, zoom_out, dir) = ctx.input(|i| {
            let k = |key| i.key_pressed(key) && !i.modifiers.command;
            let dx =
                f32::from(u8::from(k(Key::ArrowRight))) - f32::from(u8::from(k(Key::ArrowLeft)));
            let dy = f32::from(u8::from(k(Key::ArrowDown))) - f32::from(u8::from(k(Key::ArrowUp)));
            (k(Key::Plus) || k(Key::Equals), k(Key::Minus), vec2(dx, dy))
        });
        if zoom_in {
            self.adjust_viewport(|r| zoomed(r, 1.0 / ZOOM_STEP));
        }
        if zoom_out {
            self.adjust_viewport(|r| zoomed(r, ZOOM_STEP));
        }
        if dir != Vec2::ZERO {
            self.adjust_viewport(|r| panned(r, dir * r.size() * PAN_STEP));
        }
        self.panorama != Some(before)
    }
}

/// Draw the strip at `strip` and apply drag / click / wheel to the viewport of `view`.
pub fn show_strip(ui: &mut Ui, strip: Rect, view: &mut ViewState) {
    let Some(vp) = view.panorama else {
        return;
    };
    let resp = ui
        .interact(
            strip,
            ui.id().with("panorama_strip"),
            Sense::click_and_drag(),
        )
        .on_hover_text("Drag to pan, wheel to zoom (P: leave, arrows: pan, +/-: zoom)");
    if let Some(p) = resp
        .interact_pointer_pos()
        .filter(|_| resp.is_pointer_button_down_on() || resp.clicked())
    {
        view.adjust_viewport(|r| centered(r, strip_to_panel(strip, p)));
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            let step = if wheel > 0.0 {
                1.0 / ZOOM_STEP
            } else {
                ZOOM_STEP
            };
            view.adjust_viewport(|r| zoomed(r, step));
        }
    }
    let vp = view.panorama.unwrap_or(vp);
    let painter = ui.painter();
    painter.rect_filled(strip, 3.0_f32, CHASSIS_RED);
    for s in FOCUSABLE {
        let r = rect_to_strip(strip, section_px(s, Pos2::ZERO));
        painter.rect_filled(r, 1.0_f32, MODULE_SLATE);
    }
    painter.rect_stroke(
        strip,
        3.0_f32,
        Stroke::new(1.0_f32, CHASSIS_DARK),
        StrokeKind::Inside,
    );
    let v = rect_to_strip(strip, vp);
    painter.rect_filled(v, 1.0_f32, LED_AMBER.gamma_multiply(0.18));
    painter.rect_stroke(
        v,
        1.0_f32,
        Stroke::new(1.5_f32, LED_AMBER),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn clamp_keeps_the_viewport_inside_the_panel_at_a_legal_scale() {
        for r in [
            Rect::from_min_size(pos2(-50.0, -50.0), vec2(100.0, 40.0)),
            Rect::from_min_size(pos2(1500.0, 400.0), PANEL_SIZE * 0.5),
            Rect::from_min_size(Pos2::ZERO, PANEL_SIZE * 3.0),
        ] {
            let c = clamp_viewport(r);
            assert!(panel_rect().contains_rect(c), "{c:?}");
            let scale = c.width() / PANEL_SIZE.x;
            assert!((MIN_SCALE - 1e-4..=1.0 + 1e-4).contains(&scale), "{scale}");
            assert!(close(c.width() / c.height(), PANEL_SIZE.x / PANEL_SIZE.y));
        }
    }

    #[test]
    fn strip_and_panel_coordinates_round_trip() {
        let strip = Rect::from_min_size(pos2(7.0, 9.0), vec2(STRIP_W, STRIP_H));
        let p = pos2(321.0, 123.0);
        let q = strip_to_panel(strip, panel_to_strip(strip, p));
        assert!(close(p.x, q.x) && close(p.y, q.y));
        assert_eq!(strip_to_panel(strip, strip.min), Pos2::ZERO);
        let end = strip_to_panel(strip, strip.max);
        assert!(close(end.x, PANEL_SIZE.x) && close(end.y, PANEL_SIZE.y));
    }

    #[test]
    fn centering_zoom_and_pan_behave() {
        let r = clamp_viewport(Rect::from_min_size(Pos2::ZERO, PANEL_SIZE * 0.5));
        let c = centered(r, pos2(1500.0, 300.0));
        assert!(close(c.center().x, 1500.0) && close(c.center().y, 300.0));
        let top = centered(r, pos2(1500.0, 5.0));
        assert!(close(top.min.y, 0.0), "y clamped at the panel top");
        assert!(close(c.width(), r.width()));
        let out = zoomed(c, 2.0);
        assert!(close(out.width(), PANEL_SIZE.x));
        assert_eq!(zoomed(out, 2.0), out, "full panel is the zoom-out limit");
        let tiny = zoomed(c, 0.01);
        assert!(close(tiny.width(), PANEL_SIZE.x * MIN_SCALE));
        let moved = panned(r, vec2(100.0, 0.0));
        assert!(close(moved.min.x, r.min.x + 100.0));
        assert_eq!(panned(r, vec2(-1e5, -1e5)).min, Pos2::ZERO);
    }

    #[test]
    fn mode_opens_on_the_focused_section_or_the_centre() {
        let mut v = ViewState::default();
        v.toggle_panorama();
        let vp = v.panorama.expect("on");
        assert!(close(vp.center().x, PANEL_SIZE.x / 2.0));
        v.toggle_panorama();
        assert_eq!(v.panorama, None);
        v.focus = Some(egui_section_synth());
        v.toggle_panorama();
        assert_eq!(v.focus, None);
        let vp = v.panorama.expect("on");
        let s = section_px(egui_section_synth(), Pos2::ZERO);
        assert!(vp.contains(s.center()), "{vp:?} should hold the synth bay");
    }

    fn egui_section_synth() -> nord_panel::Section {
        nord_panel::Section::Synth
    }

    #[test]
    fn strip_sits_below_the_view_with_the_panel_aspect() {
        let vis = Rect::from_min_size(pos2(5.0, 6.0), vec2(800.0, 300.0));
        let s = strip_rect(vis);
        assert_eq!(s.min, pos2(5.0, 306.0 + STRIP_GAP));
        assert!(close(s.width() / s.height(), PANEL_SIZE.x / PANEL_SIZE.y));
        assert!(close(strip_extra_h(), STRIP_H + STRIP_GAP));
    }

    #[test]
    fn adjust_is_a_no_op_outside_the_mode() {
        let mut v = ViewState::default();
        v.adjust_viewport(|r| zoomed(r, 2.0));
        assert_eq!(v.panorama, None);
        assert!(!v.handle_viewport_keys(&Context::default()));
    }
}
