//! Maps `nord_panel::layout` millimetre rectangles to screen pixels.
//!
//! The standalone panel is drawn at [`PX_PER_MM`] (1560 px for the 1290 mm body). Bays place
//! controls with [`rect_of`] so every control sits where the real instrument has it.

use egui::{pos2, vec2, Pos2, Rect};
use nord_panel::layout;

/// Panel width in px for the 1290 mm instrument body.
pub const PANEL_WIDTH_PX: f32 = 1560.0;

/// Screen pixels per panel millimetre.
pub const PX_PER_MM: f32 = PANEL_WIDTH_PX / layout::INSTRUMENT_WIDTH_MM;

/// Convert a layout rectangle to egui pixels; `origin` is the panel's top-left corner.
pub fn to_px(r: layout::Rect, origin: Pos2) -> Rect {
    Rect::from_min_size(
        pos2(origin.x + r.x * PX_PER_MM, origin.y + r.y * PX_PER_MM),
        vec2(r.w * PX_PER_MM, r.h * PX_PER_MM),
    )
}

/// Pixel rectangle of an inventory control, or `None` for an unknown id.
pub fn rect_of(id: &str, origin: Pos2) -> Option<Rect> {
    layout::rect_for(id).map(|r| to_px(r, origin))
}

/// Pixel rectangle of an inventory control; a zero-size rect at `origin` for unknown ids
/// (never panics on the render path; the inventory completeness test guards the ids).
pub fn rect_or_empty(id: &str, origin: Pos2) -> Rect {
    rect_of(id, origin).unwrap_or_else(|| Rect::from_min_size(origin, vec2(0.0, 0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_maps_instrument_width_to_panel_width() {
        let r = layout::Rect::new(0.0, 0.0, layout::INSTRUMENT_WIDTH_MM, 10.0);
        let p = to_px(r, pos2(0.0, 0.0));
        assert!((p.width() - PANEL_WIDTH_PX).abs() < 1e-3);
    }

    #[test]
    fn origin_offsets_the_rect() {
        let r = layout::Rect::new(10.0, 20.0, 5.0, 5.0);
        let p = to_px(r, pos2(100.0, 200.0));
        assert!((p.min.x - (100.0 + 10.0 * PX_PER_MM)).abs() < 1e-3);
        assert!((p.min.y - (200.0 + 20.0 * PX_PER_MM)).abs() < 1e-3);
    }

    #[test]
    fn known_control_resolves_and_unknown_is_empty() {
        let o = pos2(0.0, 0.0);
        assert!(rect_of("master.level", o).is_some());
        assert!(rect_of("no.such.control", o).is_none());
        assert_eq!(rect_or_empty("no.such.control", o).width(), 0.0);
    }

    #[test]
    fn every_inventory_control_has_a_pixel_rect() {
        for c in nord_panel::all_controls() {
            assert!(rect_of(&c.id, pos2(0.0, 0.0)).is_some(), "{}", c.id);
        }
    }
}
