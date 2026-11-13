//! egui-specific drawing (the only module that names egui types).
//!
//! Skeleton: conversions only; sprite/painter rendering arrives with B6/B7/B8.

mod convert;

pub use convert::{to_egui_rect, ui_scale};
