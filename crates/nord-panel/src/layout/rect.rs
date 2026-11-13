//! Axis-aligned rectangle in panel millimetres, free of egui types.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PanelRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// True when `(px, py)` lies inside (left/top inclusive, right/bottom exclusive).
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// True when the interiors intersect (touching edges do not overlap).
    pub fn overlaps(&self, o: &PanelRect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// True when any two rects in `rects` overlap.
pub fn any_overlap(rects: &[PanelRect]) -> bool {
    rects
        .iter()
        .enumerate()
        .any(|(i, a)| rects[i + 1..].iter().any(|b| a.overlaps(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_is_half_open() {
        let r = PanelRect::new(0.0, 0.0, 10.0, 5.0);
        assert!(r.contains(0.0, 0.0));
        assert!(r.contains(9.9, 4.9));
        assert!(!r.contains(10.0, 2.0));
        assert!(!r.contains(5.0, 5.0));
    }

    #[test]
    fn test_touching_rects_do_not_overlap_but_intersecting_do() {
        let a = PanelRect::new(0.0, 0.0, 10.0, 10.0);
        let touching = PanelRect::new(10.0, 0.0, 10.0, 10.0);
        let inside = PanelRect::new(5.0, 5.0, 10.0, 10.0);
        assert!(!a.overlaps(&touching));
        assert!(a.overlaps(&inside));
        assert!(!any_overlap(&[a, touching]));
        assert!(any_overlap(&[a, touching, inside]));
    }
}
