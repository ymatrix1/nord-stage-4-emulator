//! Panel layout: a rectangle in panel millimetres for every inventory control
//! (`docs/specs/control-inventory.md` §7).
//!
//! Renderers (`nord-gui`, the plugin editor) place widgets with [`rect_for`]
//! and the section frames with [`section_rect`]. No toolkit types here.
//!
//! Coordinates: origin at the top-left corner of the instrument body, `x`
//! to the right, `y` down, millimetres. The control area (red panel) spans
//! `y = 0 ..` [`PANEL_HEIGHT_MM`]; the keybed follows below it.
//!
//! ASSUMED: every rectangle was read by eye from one 2000 px product photo
//! (body 1865 px wide = 1290 mm, [`MM_PER_PX`]); the tables are authored in
//! photo pixels and converted by [`px`]. Accuracy is about +-3 px
//! (+-2 mm) for knobs and buttons, worse for the entries in [`ESTIMATED`].
//! A higher-resolution photo or a tape measure on the instrument would
//! confirm or replace them.

mod effects;
mod estimated;
mod keybed;
mod master;
mod organ;
mod piano;
mod program;
mod shared;
mod synth;

use crate::types::Section;

pub use estimated::ESTIMATED;
pub use shared::{shared_base, SHARED};

/// Instrument width in mm (88-key model).
///
/// ASSUMED: about 1290 mm per the product data; confirm with a measurement.
pub const INSTRUMENT_WIDTH_MM: f32 = 1290.0;

/// Width of the instrument body in the reference photo (px, x 67..1932).
const BODY_WIDTH_PX: f32 = 1865.0;

/// Millimetres per photo pixel.
///
/// ASSUMED: `1290 / 1865 = 0.6917`; the photo is a straight-on render, so a
/// single scale is used for both axes.
pub const MM_PER_PX: f32 = INSTRUMENT_WIDTH_MM / BODY_WIDTH_PX;

/// Photo pixel of the body's top-left corner (the layout origin).
const ORIGIN_PX: (f32, f32) = (67.0, 81.0);

/// Photo pixel row where the control area ends and the keybed begins.
const KEYBED_TOP_PX: f32 = 340.0;

/// Photo pixel row of the keybed's lower edge.
const KEYBED_BOTTOM_PX: f32 = 571.0;

/// Height of the red control area in mm.
pub const PANEL_HEIGHT_MM: f32 = (KEYBED_TOP_PX - ORIGIN_PX.1) * MM_PER_PX;

/// Axis-aligned rectangle in panel millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// `other` lies inside `self` (edges may touch; tolerance 1e-3 mm).
    pub fn contains(&self, other: &Rect) -> bool {
        const E: f32 = 1e-3;
        other.x >= self.x - E
            && other.y >= self.y - E
            && other.right() <= self.right() + E
            && other.bottom() <= self.bottom() + E
    }

    /// The interiors intersect (touching edges do not count; tolerance 1e-3 mm).
    pub fn overlaps(&self, other: &Rect) -> bool {
        const E: f32 = 1e-3;
        self.x < other.right() - E
            && other.x < self.right() - E
            && self.y < other.bottom() - E
            && other.y < self.bottom() - E
    }
}

/// Rectangle from photo-pixel corners `(x0, y0)`..`(x1, y1)`.
pub(crate) fn px(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    let (ox, oy) = ORIGIN_PX;
    Rect::new(
        (x0 - ox) * MM_PER_PX,
        (y0 - oy) * MM_PER_PX,
        (x1 - x0) * MM_PER_PX,
        (y1 - y0) * MM_PER_PX,
    )
}

/// The whole instrument: control area plus keybed.
pub fn panel_rect() -> Rect {
    px(ORIGIN_PX.0, ORIGIN_PX.1, 1932.0, KEYBED_BOTTOM_PX)
}

/// Frame of a panel section.
///
/// ASSUMED: section borders read from the photo (boundaries between the
/// printed section boxes, rounded to whole photo pixels).
pub fn section_rect(section: Section) -> Rect {
    let (x0, x1) = match section {
        Section::Master => (67.0, 415.0),
        Section::Organ => (415.0, 709.0),
        Section::Piano => (709.0, 841.0),
        Section::Program => (841.0, 1029.0),
        Section::Synth => (1029.0, 1401.0),
        Section::Effects => (1401.0, 1700.0),
        Section::Keybed => return px(67.0, KEYBED_TOP_PX, 1932.0, KEYBED_BOTTOM_PX),
    };
    px(x0, ORIGIN_PX.1, x1, KEYBED_TOP_PX)
}

/// Layout rectangle of control `id`, or `None` for an unknown id.
///
/// Shifted functions return the rectangle of the control they hang off
/// ([`SHARED`]).
pub fn rect_for(id: &str) -> Option<Rect> {
    let id = shared_base(id).unwrap_or(id);
    master::rect(id)
        .or_else(|| organ::rect(id))
        .or_else(|| piano::rect(id))
        .or_else(|| program::rect(id))
        .or_else(|| synth::rect(id))
        .or_else(|| effects::rect(id))
        .or_else(|| keybed::rect(id))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn scale_matches_the_reference_photo() {
        assert!((MM_PER_PX - 0.6917).abs() < 1e-3);
        let panel = panel_rect();
        assert!((panel.w - INSTRUMENT_WIDTH_MM).abs() < 0.5);
        assert!((section_rect(Section::Keybed).y - PANEL_HEIGHT_MM).abs() < 1e-3);
        assert!(panel.contains(&Rect::new(0.0, 0.0, 1.0, 1.0)));
    }

    #[test]
    fn rect_helpers() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let touching = Rect::new(10.0, 0.0, 5.0, 5.0);
        let over = Rect::new(9.0, 9.0, 5.0, 5.0);
        assert!(!a.overlaps(&touching));
        assert!(a.overlaps(&over));
        assert!(a.contains(&Rect::new(1.0, 1.0, 2.0, 2.0)));
        assert!(!a.contains(&over));
        assert_eq!((a.right(), a.bottom()), (10.0, 10.0));
    }
}
