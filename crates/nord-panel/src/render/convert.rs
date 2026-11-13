//! `PanelRect` (millimetres) to egui screen rectangles.

use crate::layout::PanelRect;
use egui::{pos2, vec2, Rect};

/// Points per millimetre for a panel `panel_width_mm` wide shown in `available_width_pt` points.
pub fn ui_scale(available_width_pt: f32, panel_width_mm: f32) -> f32 {
    if panel_width_mm <= 0.0 {
        return 0.0;
    }
    available_width_pt / panel_width_mm
}

/// Converts `r` to screen space: `origin` is the top-left of the panel in points, `scale` is
/// points per millimetre.
pub fn to_egui_rect(r: &PanelRect, origin: egui::Pos2, scale: f32) -> Rect {
    Rect::from_min_size(
        pos2(origin.x + r.x * scale, origin.y + r.y * scale),
        vec2(r.w * scale, r.h * scale),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_egui_rect_applies_origin_and_scale() {
        let r = PanelRect::new(10.0, 20.0, 30.0, 40.0);
        let e = to_egui_rect(&r, pos2(100.0, 200.0), 2.0);
        assert_eq!(e.min, pos2(120.0, 240.0));
        assert_eq!(e.size(), vec2(60.0, 80.0));
    }

    #[test]
    fn test_ui_scale_guards_zero_width() {
        assert_eq!(ui_scale(800.0, 400.0), 2.0);
        assert_eq!(ui_scale(800.0, 0.0), 0.0);
    }
}
