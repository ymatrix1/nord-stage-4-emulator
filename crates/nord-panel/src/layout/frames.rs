//! Printed group frames and section titles of the front panel (functional identification).
//!
//! The real panel prints a frame with a title around related controls (`MOD 1`, `OSCILLATORS`,
//! `PIANO SELECT`, ...) and a few bare titles over a button (`ACOUSTICS`, `KB TOUCH`). A frame is
//! not a free rectangle: it is the union of the layout rectangles of its `members`, grown by `pad`
//! (photo px, left / top / right / bottom) so that the legends printed around those controls stay
//! inside. A renderer that nudges a member therefore moves the frame with it.
//!
//! ASSUMED: membership and padding follow the printed boxes in the reference photos
//! (`.reference/crops`: `NS4-Panel-synth`, `NS4_Panel-crop-right`, `NS4_HA88_Piano-01`,
//! `NS4_HA88_Organ-01`); the pads were tuned until every label of the members fits (the legibility
//! audit of `nord-gui`, rule `G`). A higher-resolution photo or a measurement would confirm them.

use super::{rect_for, Rect, MM_PER_PX};

pub use super::frames_table::FRAMES;
use crate::types::Section;

/// How a frame is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Outline with the title breaking its top border.
    Box,
    /// Bare title centred over the members (no outline).
    Title,
}

/// One printed frame or title.
#[derive(Debug, Clone, Copy)]
pub struct FrameDef {
    pub id: &'static str,
    pub section: Section,
    pub title: &'static str,
    pub kind: FrameKind,
    /// Inventory ids whose rectangles the frame encloses.
    pub members: &'static [&'static str],
    /// Growth `[left, top, right, bottom]` in photo px.
    pub pad: [f32; 4],
    /// Horizontal place of the title along the top edge (0 = left corner, 0.5 = centred).
    pub title_x: f32,
}

impl FrameDef {
    /// The same frame with its title at `x` along the top edge.
    pub(super) const fn at(mut self, x: f32) -> Self {
        self.title_x = x;
        self
    }
}

/// Frames of one section.
pub fn frames_of(section: Section) -> impl Iterator<Item = &'static FrameDef> {
    FRAMES.iter().filter(move |f| f.section == section)
}

/// Definition of frame `id`.
pub fn frame_def(id: &str) -> Option<&'static FrameDef> {
    FRAMES.iter().find(|f| f.id == id)
}

/// Union of the member rectangles of `def`, grown by its pad (mm).
pub fn frame_rect(def: &FrameDef) -> Option<Rect> {
    let mut it = def.members.iter().map(|m| rect_for(m));
    let first = it.next()??;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.right(), first.bottom());
    for r in it {
        let r = r?;
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    }
    let [l, t, r, b] = def.pad.map(|v| v * MM_PER_PX);
    Some(Rect::new(x0 - l, y0 - t, x1 - x0 + l + r, y1 - y0 + t + b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::section_rect;
    use crate::types::Section::Synth;

    #[test]
    fn every_member_has_a_layout_rect_and_every_frame_a_title() {
        for f in FRAMES {
            assert!(!f.title.is_empty() && !f.members.is_empty(), "{}", f.id);
            assert!(frame_rect(f).is_some(), "{}: member without a rect", f.id);
        }
    }

    #[test]
    fn ids_are_unique_and_the_manufacturer_is_never_printed() {
        for (i, f) in FRAMES.iter().enumerate() {
            assert!(
                FRAMES[..i].iter().all(|g| g.id != f.id),
                "duplicate {}",
                f.id
            );
            assert!(!f.title.to_lowercase().contains("clavia"), "{}", f.id);
            assert_eq!(frame_def(f.id).map(|d| d.title), Some(f.title));
        }
        assert!(frame_def("nope").is_none());
    }

    #[test]
    fn frames_stay_inside_their_section() {
        for f in FRAMES {
            let r = frame_rect(f).unwrap();
            assert!(section_rect(f.section).contains(&r), "{}: {r:?}", f.id);
        }
    }

    #[test]
    fn frames_of_filters_by_section() {
        assert!(frames_of(Synth).count() >= 9);
        assert!(frames_of(Synth).all(|f| f.section == Synth));
        assert_eq!(frames_of(crate::types::Section::Keybed).count(), 0);
    }

    #[test]
    fn pad_grows_the_member_union() {
        let f = frame_def("synth.frame.amp").unwrap();
        let m = rect_for("synth.amp.envelope").unwrap();
        let r = frame_rect(f).unwrap();
        assert!(r.contains(&m) && r.w > m.w && r.h > m.h);
    }
}
