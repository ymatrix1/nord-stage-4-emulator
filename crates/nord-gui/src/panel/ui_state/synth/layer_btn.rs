//! Layer A/B/C buttons (manual p.27): focus, turn on / off, SUSTPED and PSTICK.

use super::SynthUi;
use crate::audio::SynthEdit;
use nord_core::engine::{PSTICK_MAX_RANGE, PSTICK_MIN_RANGE};
use nord_panel::behaviour::{resolve, Gesture, Trigger};

/// Scroll points that move the PSTICK/RNG list by one entry (same pitch as the dials).
const POINTS_PER_STEP: f32 = 24.0;

/// What a Layer button press asks the bay to do with the layer's ON state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerAction {
    None,
    Activate,
    Deactivate,
}

impl SynthUi {
    /// Layer A/B/C button (manual p.27). Tap focuses the layer (and turns it on, as a mouse cannot
    /// press two buttons at once); hold turns it off; Shift+A is SUSTPED, Shift+B is PSTICK.
    pub fn layer_button(
        &mut self,
        i: usize,
        g: Gesture,
        shift_held: bool,
        active: bool,
    ) -> LayerAction {
        let i = i.min(2);
        let t = if g == Gesture::Hold {
            Trigger::Hold
        } else {
            resolve(g, shift_held, i < 2)
        };
        match t {
            Trigger::Shifted if i == 0 => {
                self.sustped = !self.sustped;
                self.out.push(SynthEdit::Sustped(self.sustped));
                LayerAction::None
            }
            Trigger::Shifted => {
                self.pstick = !self.pstick;
                self.out.push(SynthEdit::Pstick(self.pstick));
                LayerAction::None
            }
            Trigger::Hold if active => LayerAction::Deactivate,
            Trigger::Hold => LayerAction::None,
            _ => {
                self.focus_layer(i);
                if active {
                    LayerAction::None
                } else {
                    LayerAction::Activate
                }
            }
        }
    }
}

impl SynthUi {
    /// PSTICK/RNG list (manual p.28): moves the bend range `steps` semitones (clamped to
    /// `1..=12`) and queues the edit when it changed. Returns whether it changed.
    pub fn pstick_range_step(&mut self, steps: i32) -> bool {
        let next = (i32::from(self.pstick_range) + steps)
            .clamp(i32::from(PSTICK_MIN_RANGE), i32::from(PSTICK_MAX_RANGE))
            as u8;
        if next == self.pstick_range {
            return false;
        }
        self.pstick_range = next;
        self.out.push(SynthEdit::PstickRange(next));
        true
    }

    /// Feeds mouse-wheel `points` over the PSTICK/RNG button (scroll up = larger range).
    pub fn pstick_range_scroll(&mut self, points: f32) -> bool {
        self.range_acc += points / POINTS_PER_STEP;
        let whole = self.range_acc.trunc();
        self.range_acc -= whole;
        whole != 0.0 && self.pstick_range_step(whole as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_steps_clamp_and_queue_one_edit_per_change() {
        let mut s = SynthUi::new();
        assert_eq!(s.pstick_range, 2);
        assert!(s.pstick_range_step(3));
        assert!(!s.pstick_range_step(0));
        assert!(s.pstick_range_step(100));
        assert_eq!(s.pstick_range, 12);
        assert!(!s.pstick_range_step(1), "already at the top");
        assert!(s.pstick_range_step(-100));
        assert_eq!(s.pstick_range, 1);
        assert_eq!(
            s.take_edits(),
            vec![
                SynthEdit::PstickRange(5),
                SynthEdit::PstickRange(12),
                SynthEdit::PstickRange(1)
            ]
        );
    }

    #[test]
    fn scroll_points_accumulate_into_whole_steps() {
        let mut s = SynthUi::new();
        assert!(!s.pstick_range_scroll(10.0));
        assert!(s.pstick_range_scroll(14.0), "24 points make one step");
        assert_eq!(s.pstick_range, 3);
        assert!(s.pstick_range_scroll(-48.0));
        assert_eq!(s.pstick_range, 1);
    }

    #[test]
    fn layer_tap_focuses_and_activates() {
        let mut s = SynthUi::new();
        let a = s.layer_button(1, Gesture::Tap, false, false);
        assert_eq!((a, s.focus), (LayerAction::Activate, 1));
        let a = s.layer_button(2, Gesture::Tap, false, true);
        assert_eq!((a, s.focus), (LayerAction::None, 2));
    }

    #[test]
    fn layer_hold_turns_off_and_shift_selects_sustped_and_pstick() {
        let mut s = SynthUi::new();
        assert_eq!(
            s.layer_button(0, Gesture::Hold, false, true),
            LayerAction::Deactivate
        );
        assert_eq!(
            s.layer_button(0, Gesture::Hold, false, false),
            LayerAction::None
        );
        s.layer_button(0, Gesture::Tap, true, true);
        assert!(s.sustped);
        s.layer_button(1, Gesture::Tap, true, true);
        assert!(s.pstick);
        assert_eq!(
            s.take_edits(),
            vec![SynthEdit::Sustped(true), SynthEdit::Pstick(true)]
        );
        s.layer_button(0, Gesture::Tap, true, true);
        assert_eq!(s.take_edits(), vec![SynthEdit::Sustped(false)]);
        assert_eq!(s.focus, 0, "shifted taps do not move the focus");
        // Layer C has no shifted function: a shifted tap is a plain tap.
        assert_eq!(
            s.layer_button(2, Gesture::Tap, true, true),
            LayerAction::None
        );
        assert_eq!(s.focus, 2);
    }
}
