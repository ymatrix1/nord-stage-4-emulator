//! Shared Nord Stage 4 front panel (Phase 6b, decision D15).
//!
//! Only `render` touches egui; `inventory`, `layout`, `model` and `behavior` are toolkit-free so
//! control state and hit-testing can be unit-tested without a GUI context.
//!
//! Specs: `docs/specs/control-inventory.md`, `docs/specs/visual-style-and-panel.md`

pub mod behavior;
pub mod inventory;
pub mod layout;
pub mod model;
pub mod render;
