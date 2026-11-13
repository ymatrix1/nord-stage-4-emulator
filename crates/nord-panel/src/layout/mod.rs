//! Panel geometry in millimetres (spec `control-inventory.md` §4 geometry checks).
//!
//! Skeleton: rect type and overlap helpers; per-control rects arrive with B8.

mod rect;

pub use rect::{any_overlap, PanelRect};
