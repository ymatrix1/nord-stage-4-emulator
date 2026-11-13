//! Custom Hardware Widgets: LED ladders, drawbars, buttons, knobs, OLEDs, pitch stick & mod wheel.
//!
//! All widgets are rect-based (absolute placement) and return `true` when changed.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`
//!
//! Submodules: `chrome` (labels, LEDs, frames, OLED), `buttons`, `sliders` (drawbar, knob),
//! `selectors` (matrix selector, wheels, `cycle`). Everything is re-exported flat.

mod buttons;
mod chrome;
mod selectors;
mod sliders;
#[cfg(test)]
mod support;

pub use buttons::*;
pub use chrome::*;
pub use selectors::*;
pub use sliders::*;
