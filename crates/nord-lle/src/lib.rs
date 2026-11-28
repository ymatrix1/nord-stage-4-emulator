//! Unified multi-chip LLE engine (bean `ns4-lle5`, spec `docs/specs/lle-engine.md`).
//!
//! * [`clock`]: the virtual-time scheduler (ARM timer ticks to DSP passes, integer exact).
//! * [`arm`]: the ARM0 side behind the [`arm::ArmHost`] trait (real [`arm::Arm0`] or a test fake).
//! * [`route`]: ARM message to chip/core routing.
//! * [`smp_refill`]: SMP `.npno` page refill (descriptor detection, flash placement).
//! * [`input`]: neutral live events to ARM0 input traffic (`nord_core::lle_io`).
//! * [`engine`]: [`engine::LleEngine`] composing all of it with the audio bus.
//! * [`load`]: images from a user-provided directory at run time.
//!
//! No vendor bytes live in this crate (D20, D128); images are read at run time.

pub mod arm;
pub mod clock;
pub mod engine;
pub mod input;
pub mod load;
pub mod route;
pub mod smp_refill;

#[cfg(test)]
mod engine_tests;
