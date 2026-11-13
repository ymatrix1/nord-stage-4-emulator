//! Nord Stage 4 front-panel control inventory.
//!
//! One machine-checked list of every front-panel control documented in the
//! OS v1.6x manual (ch. 3-9), shared by the standalone app and the plugin
//! editor, plus the behaviour layer (`behaviour/`: gestures, SHIFT, latch, pot
//! pick-up, Live Mode, `PanelHost`). Layout rects and renderers follow in B6-B8;
//! see `docs/specs/control-inventory.md`.

pub mod behaviour;
pub mod inventory;
pub mod types;

pub use inventory::all_controls;
pub use types::{Access, Binding, ControlEntry, ControlKind, Layer, Provenance, Section};
