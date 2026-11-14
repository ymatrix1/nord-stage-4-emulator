//! Focus / zoom view (B12 "focused rack view") and scale-to-fit.
//!
//! The bays always draw the same way at the same logical size ([`PANEL_SIZE`]). The view only
//! decides (a) which part of that panel is visible and (b) the egui zoom factor, so text stays
//! crisp at any zoom. Nothing here duplicates bay code.
//!
//! * Overview: the whole panel, zoom = window fit.
//! * Focus on one section: that section fills the window (zoom up to [`MAX_ZOOM`]).
//! * Click a section header or press Z to toggle focus, Esc to leave, 1-5 jump to a section.
//! * Panorama (P, see `panorama.rs`): the detailed view shows a pannable / zoomable rectangle
//!   of the panel ([`ViewState::panorama`]) with a strip of the whole instrument below it.

use super::layout_px::{to_px, PANEL_HEIGHT_PX, PANEL_WIDTH_PX};
use super::panorama;
use egui::{vec2, Context, Pos2, Rect, Vec2};
use nord_panel::Section;

/// Logical size of the drawn panel (control area plus keybed).
pub const PANEL_SIZE: Vec2 = Vec2::new(PANEL_WIDTH_PX, PANEL_HEIGHT_PX);
/// Height of a section header strip (`widgets::section_frame`).
pub const HEADER_H: f32 = 22.0;
/// Logical px kept around a focused section.
const FOCUS_MARGIN: f32 = 6.0;
/// Logical height of the two bars above the panel in overview (they scale with the zoom).
const BAR_H: f32 = 70.0;
/// Largest automatic zoom.
pub const MAX_ZOOM: f32 = 4.0;
/// Smallest automatic zoom.
pub const MIN_ZOOM: f32 = 0.5;

/// Sections reachable with keys 1..=5 and by their header.
pub const FOCUSABLE: [Section; 5] = [
    Section::Organ,
    Section::Piano,
    Section::Program,
    Section::Synth,
    Section::Effects,
];

/// Which part of the panel is shown, and an optional fixed zoom (screenshots).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewState {
    /// Section filling the window, `None` for the whole panel.
    pub focus: Option<Section>,
    /// Section Z jumps to when nothing is focused (the last one left).
    pub last: Option<Section>,
    /// Fixed zoom in physical px per logical px (screenshots); `None` fits the window.
    pub fixed_zoom: Option<f32>,
    /// Panorama mode: the visible rectangle in logical panel px (`None` = mode off).
    pub panorama: Option<Rect>,
}

impl ViewState {
    /// Toggle focus on `s`: focus it, or return to the overview when it already has focus.
    pub fn toggle(&mut self, s: Section) {
        self.panorama = None;
        if self.focus == Some(s) {
            self.exit();
        } else {
            self.focus = Some(s);
        }
    }

    /// Return to the overview, remembering the section left.
    pub fn exit(&mut self) {
        if self.focus.is_some() {
            self.last = self.focus;
        }
        self.focus = None;
    }

    /// Z: leave focus, or focus the section under the pointer (else the last / Organ).
    pub fn toggle_z(&mut self, hovered: Option<Section>) {
        match self.focus {
            Some(_) => self.exit(),
            None => {
                self.panorama = None;
                self.focus = hovered.or(self.last).or(Some(Section::Organ));
            }
        }
    }

    /// Part of the panel the view shows in logical panel px; `None` is the whole panel.
    pub fn region(&self) -> Option<Rect> {
        self.panorama.or_else(|| focus_region(self.focus))
    }

    /// Top-left of the panel for a view whose visible area starts at `top_left`.
    pub fn origin(&self, top_left: Pos2) -> Pos2 {
        region_origin(self.region(), top_left)
    }

    /// Logical height the view reserves on top of the panel (bars, panorama strip).
    fn extra_h(&self) -> f32 {
        match (self.panorama, self.focus) {
            (Some(_), _) => BAR_H + panorama::strip_extra_h(),
            (None, Some(_)) => 0.0,
            (None, None) => BAR_H,
        }
    }

    /// Apply P, Z, Esc, 1-5 and (in the panorama) arrows and +/- from this frame's input. Returns `true` when the view changed.
    pub fn handle_keys(&mut self, ctx: &Context, origin: Pos2) -> bool {
        if ctx.wants_keyboard_input() {
            return false;
        }
        let before = *self;
        let moved = self.handle_viewport_keys(ctx);
        let (p, z, esc, digit, hover) = ctx.input(|i| {
            let digit =
                (0..FOCUSABLE.len()).find(|n| i.key_pressed(digit_key(*n)) && !i.modifiers.any());
            (
                i.key_pressed(egui::Key::P) && !i.modifiers.any(),
                i.key_pressed(egui::Key::Z) && !i.modifiers.any(),
                i.key_pressed(egui::Key::Escape),
                digit,
                i.pointer.hover_pos(),
            )
        });
        if p {
            self.toggle_panorama();
        } else if z {
            self.toggle_z(hover.and_then(|p| section_at(p, origin)));
        } else if esc {
            if self.panorama.take().is_none() {
                self.exit();
            }
        } else if let Some(n) = digit {
            self.panorama = None;
            self.focus = Some(FOCUSABLE[n]);
        }
        moved || *self != before
    }
}

fn digit_key(n: usize) -> egui::Key {
    [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
    ][n]
}

/// Pixel rectangle of `section` for a panel whose top-left is `origin`.
pub fn section_px(section: Section, origin: Pos2) -> Rect {
    to_px(nord_panel::layout::section_rect(section), origin)
}

/// Section of [`FOCUSABLE`] under `pos`, for a panel whose top-left is `origin`.
pub fn section_at(pos: Pos2, origin: Pos2) -> Option<Section> {
    FOCUSABLE
        .into_iter()
        .find(|s| section_px(*s, origin).contains(pos))
}

/// Header strip of `section`.
pub fn header_px(section: Section, origin: Pos2) -> Rect {
    let r = section_px(section, origin);
    Rect::from_min_size(r.min, vec2(r.width(), HEADER_H))
}

/// Region of the panel a focused section shows (the section plus [`FOCUS_MARGIN`]).
pub fn focus_region(focus: Option<Section>) -> Option<Rect> {
    focus.map(|s| section_px(s, Pos2::ZERO).expand(FOCUS_MARGIN))
}

/// Logical size of `region` (`None` is the whole panel).
pub fn region_size(region: Option<Rect>) -> Vec2 {
    region.map_or(PANEL_SIZE, |r| r.size())
}

/// Logical size of what a focused (or whole-panel) view shows.
pub fn content_size(focus: Option<Section>) -> Vec2 {
    region_size(focus_region(focus))
}

/// Top-left of the panel when `region` of it starts at `top_left`.
pub fn region_origin(region: Option<Rect>, top_left: Pos2) -> Pos2 {
    region.map_or(top_left, |r| top_left - r.min.to_vec2())
}

/// Zoom (egui zoom factor relative to the native scale) that fits `size` plus `extra_h` logical
/// px into a window of `window_phys` physical px on a display with `native_ppp` px per point.
fn fit(size: Vec2, extra_h: f32, window_phys: Vec2, native_ppp: f32) -> f32 {
    let w = window_phys / native_ppp.max(0.1);
    (w.x / size.x)
        .min(w.y / (size.y + extra_h))
        .clamp(MIN_ZOOM, MAX_ZOOM)
}

/// Zoom that fits a focused (or whole-panel) view into the window.
pub fn fit_zoom(focus: Option<Section>, window_phys: Vec2, native_ppp: f32) -> f32 {
    let bars = if focus.is_some() { 0.0 } else { BAR_H };
    fit(content_size(focus), bars, window_phys, native_ppp)
}

/// Zoom that fits whatever `view` shows (focus, panorama viewport or the whole panel).
pub fn fit_zoom_view(view: &ViewState, window_phys: Vec2, native_ppp: f32) -> f32 {
    fit(
        region_size(view.region()),
        view.extra_h(),
        window_phys,
        native_ppp,
    )
}

/// Apply the zoom of `view` to `ctx` (takes effect on the next frame).
pub fn apply_zoom(ctx: &Context, view: &ViewState) {
    let native = ctx.native_pixels_per_point().unwrap_or(1.0);
    let z = match view.fixed_zoom {
        Some(px_per_logical) => px_per_logical / native,
        None => {
            let phys = ctx.input(|i| i.screen_rect().size()) * ctx.pixels_per_point();
            fit_zoom_view(view, phys, native)
        }
    };
    let z = (z * 100.0).round() / 100.0;
    if (ctx.zoom_factor() - z).abs() > 0.004 {
        ctx.set_zoom_factor(z);
    }
}

/// Top-left of the panel for a focused view whose visible area starts at `top_left`.
pub fn panel_origin(focus: Option<Section>, top_left: Pos2) -> Pos2 {
    region_origin(focus_region(focus), top_left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn toggle_enters_and_leaves_and_remembers() {
        let mut v = ViewState::default();
        v.toggle(Section::Synth);
        assert_eq!(v.focus, Some(Section::Synth));
        v.toggle(Section::Synth);
        assert_eq!(v.focus, None);
        assert_eq!(v.last, Some(Section::Synth));
        v.toggle_z(None);
        assert_eq!(v.focus, Some(Section::Synth));
        v.toggle_z(None);
        assert_eq!(v.focus, None);
        v.toggle_z(Some(Section::Piano));
        assert_eq!(v.focus, Some(Section::Piano));
    }

    #[test]
    fn z_with_no_history_focuses_organ() {
        let mut v = ViewState::default();
        v.toggle_z(None);
        assert_eq!(v.focus, Some(Section::Organ));
    }

    #[test]
    fn headers_and_sections_are_found_by_position() {
        let o = pos2(10.0, 10.0);
        for s in FOCUSABLE {
            let r = section_px(s, o);
            assert_eq!(section_at(r.center(), o), Some(s));
            assert!(r.contains_rect(header_px(s, o)));
        }
        assert_eq!(section_at(pos2(-5.0, 0.0), o), None);
    }

    #[test]
    fn focused_sections_zoom_in_more_than_the_overview() {
        let win = vec2(1580.0, 900.0);
        let overview = fit_zoom(None, win, 1.0);
        for s in FOCUSABLE {
            assert!(fit_zoom(Some(s), win, 1.0) > overview * 1.5, "{s:?}");
        }
        assert!(fit_zoom(Some(Section::Effects), vec2(4000.0, 4000.0), 1.0) <= MAX_ZOOM);
        assert!(fit_zoom(None, vec2(10.0, 10.0), 1.0) >= MIN_ZOOM);
    }

    #[test]
    fn focus_origin_puts_the_section_at_the_top_left() {
        let o = panel_origin(Some(Section::Piano), pos2(0.0, 0.0));
        let r = section_px(Section::Piano, o);
        assert!((r.min.x - FOCUS_MARGIN).abs() < 1e-3);
        assert!((r.min.y - FOCUS_MARGIN).abs() < 1e-3);
        assert_eq!(panel_origin(None, pos2(3.0, 4.0)), pos2(3.0, 4.0));
    }

    #[test]
    fn keys_switch_the_view() {
        let ctx = Context::default();
        let mut v = ViewState::default();
        let press = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        for (n, s) in FOCUSABLE.iter().enumerate() {
            let _ = ctx.run(press(digit_key(n)), |ctx| {
                assert!(v.handle_keys(ctx, Pos2::ZERO));
            });
            assert_eq!(v.focus, Some(*s));
        }
        let _ = ctx.run(press(egui::Key::Escape), |ctx| {
            v.handle_keys(ctx, Pos2::ZERO);
        });
        assert_eq!(v.focus, None);
        let _ = ctx.run(press(egui::Key::Z), |ctx| {
            v.handle_keys(ctx, Pos2::ZERO);
        });
        assert_eq!(v.focus, Some(Section::Effects));
    }
}
