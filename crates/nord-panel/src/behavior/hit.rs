//! Pointer hit-testing and vertical-drag value mapping.

use crate::layout::PanelRect;
use crate::model::Normalized;

/// Index of the topmost (last) rect containing `(x, y)`, if any.
pub fn hit_test(rects: &[PanelRect], x: f32, y: f32) -> Option<usize> {
    rects.iter().rposition(|r| r.contains(x, y))
}

/// New position after dragging `dy_px` pixels upward (positive = up) across `full_travel_px`.
pub fn drag_to_value(start: Normalized, dy_px: f32, full_travel_px: f32) -> Normalized {
    if full_travel_px <= 0.0 {
        return start;
    }
    start.nudged(dy_px / full_travel_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_test_prefers_topmost_and_misses_outside() {
        let rects = [
            PanelRect::new(0.0, 0.0, 10.0, 10.0),
            PanelRect::new(5.0, 5.0, 10.0, 10.0),
        ];
        assert_eq!(hit_test(&rects, 7.0, 7.0), Some(1));
        assert_eq!(hit_test(&rects, 1.0, 1.0), Some(0));
        assert_eq!(hit_test(&rects, 50.0, 50.0), None);
    }

    #[test]
    fn test_drag_maps_travel_to_value() {
        let v = drag_to_value(Normalized::new(0.5), 50.0, 100.0);
        assert!((v.get() - 1.0).abs() < 1e-6);
        let down = drag_to_value(Normalized::new(0.5), -200.0, 100.0);
        assert_eq!(down.get(), 0.0);
        assert_eq!(drag_to_value(Normalized::new(0.3), 10.0, 0.0).get(), 0.3);
    }
}
