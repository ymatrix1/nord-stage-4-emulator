//! Toolkit-free input behaviour: hit-testing and drag mapping.
//!
//! Skeleton: B5 (`ns4-q6p7`) adds `PanelHost`, `PanelAction`, Shift / hold / double-tap and pot snap.

mod hit;

pub use hit::{drag_to_value, hit_test};
