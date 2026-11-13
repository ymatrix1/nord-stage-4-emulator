//! Custom Hardware Widgets: LED ladders, drawbars, buttons, knobs, OLEDs, pitch stick & mod wheel.
//!
//! All widgets are rect-based (absolute placement) and return `true` when changed.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

mod basic;
mod display;
mod faders;
mod knob;
#[cfg(test)]
mod support;
mod wheels;

pub use basic::{group_frame, label, led, nord_button, section_frame};
pub use display::{oled, triangle_matrix_selector};
pub use faders::{drawbar, layer_strip};
pub use knob::knob;
pub use wheels::pitch_and_mod_wheels;

/// Cycles to the next element of `items` after `cur` (wraps).
pub fn cycle<T: Copy + PartialEq>(items: &[T], cur: T) -> T {
    let i = items.iter().position(|x| *x == cur).unwrap_or(0);
    items[(i + 1) % items.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::*;
    use egui::{Color32, Stroke};

    #[test]
    fn test_widget_color_constants() {
        assert_eq!(LED_RED.r(), 0xFF);
        assert_eq!(LED_GREEN.g(), 0xE6);
    }

    #[test]
    fn test_stroke_float_type_regression() {
        let s1 = Stroke::new(1.0_f32, Color32::BLACK);
        let s2 = Stroke::new(1.5_f32, Color32::WHITE);
        assert_eq!(s1.width, 1.0_f32);
        assert_eq!(s2.width, 1.5_f32);
    }

    #[test]
    fn test_cycle_wraps() {
        assert_eq!(cycle(&[1, 2, 3], 3), 1);
        assert_eq!(cycle(&[1, 2, 3], 1), 2);
        assert_eq!(cycle(&[1, 2, 3], 9), 2);
    }
}
