//! Button gesture recognition: tap, hold and double-tap (manual p.18, p.43-44).
//!
//! Timings: `HOLD_MS` follows the manual ("roughly half a second", p.18 SOLO,
//! p.14 Layer Off); `DOUBLE_TAP_MS` is `ASSUMED` (manual p.39, p.43 describe
//! double-tapping Morph Assign and Section Edit but give no interval; confirm
//! on hardware, OQ13).

/// Hold threshold in milliseconds. VERIFIED (text): manual p.18 "roughly half a second".
pub const HOLD_MS: u32 = 500;
/// Maximum gap between a release and the next press that counts as a double tap.
/// ASSUMED: 300 ms is a common UI default; confirm on hardware.
pub const DOUBLE_TAP_MS: u32 = 300;

/// A recognised gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// Press and release before the hold threshold.
    Tap,
    /// Held down for at least [`HOLD_MS`]; fired once while still pressed.
    Hold,
    /// A second press within [`DOUBLE_TAP_MS`] of the previous tap's release.
    DoubleTap,
}

/// Per-button state machine. Times are monotonic milliseconds from the caller.
#[derive(Debug, Clone, Default)]
pub struct ButtonTracker {
    pressed_at: Option<u32>,
    last_tap_release: Option<u32>,
    hold_fired: bool,
    swallow_release: bool,
}

impl ButtonTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Button went down. Returns `DoubleTap` when it follows a recent tap.
    pub fn press(&mut self, t: u32) -> Option<Gesture> {
        self.pressed_at = Some(t);
        self.hold_fired = false;
        let double = self
            .last_tap_release
            .is_some_and(|r| t.saturating_sub(r) <= DOUBLE_TAP_MS);
        self.last_tap_release = None;
        if double {
            self.swallow_release = true;
            Some(Gesture::DoubleTap)
        } else {
            self.swallow_release = false;
            None
        }
    }

    /// Advance time while the button is held. Returns `Hold` once.
    pub fn tick(&mut self, t: u32) -> Option<Gesture> {
        let down = self.pressed_at?;
        if !self.hold_fired && !self.swallow_release && t.saturating_sub(down) >= HOLD_MS {
            self.hold_fired = true;
            return Some(Gesture::Hold);
        }
        None
    }

    /// Button went up. Returns `Tap` for a short press that was not a double tap.
    pub fn release(&mut self, t: u32) -> Option<Gesture> {
        let down = self.pressed_at.take()?;
        if self.swallow_release || self.hold_fired {
            self.swallow_release = false;
            return None;
        }
        if t.saturating_sub(down) >= HOLD_MS {
            // Released after the threshold without a tick: still a hold.
            return Some(Gesture::Hold);
        }
        self.last_tap_release = Some(t);
        Some(Gesture::Tap)
    }

    /// Whether the button is currently down.
    pub fn is_down(&self) -> bool {
        self.pressed_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_press_is_a_tap() {
        let mut b = ButtonTracker::new();
        assert_eq!(b.press(0), None);
        assert!(b.is_down());
        assert_eq!(b.release(100), Some(Gesture::Tap));
        assert!(!b.is_down());
    }

    #[test]
    fn long_press_fires_hold_once_and_no_tap() {
        let mut b = ButtonTracker::new();
        b.press(0);
        assert_eq!(b.tick(499), None);
        assert_eq!(b.tick(500), Some(Gesture::Hold));
        assert_eq!(b.tick(900), None, "hold fires once");
        assert_eq!(b.release(1000), None, "no tap after a hold");
    }

    #[test]
    fn release_after_threshold_without_tick_is_still_a_hold() {
        let mut b = ButtonTracker::new();
        b.press(0);
        assert_eq!(b.release(700), Some(Gesture::Hold));
    }

    #[test]
    fn two_quick_presses_make_a_double_tap() {
        let mut b = ButtonTracker::new();
        b.press(0);
        assert_eq!(b.release(80), Some(Gesture::Tap));
        assert_eq!(b.press(200), Some(Gesture::DoubleTap));
        assert_eq!(b.release(260), None, "double tap swallows its release");
    }

    #[test]
    fn slow_second_press_is_a_new_tap() {
        let mut b = ButtonTracker::new();
        b.press(0);
        b.release(80);
        assert_eq!(b.press(80 + DOUBLE_TAP_MS + 1), None);
        assert_eq!(b.release(500), Some(Gesture::Tap));
    }

    #[test]
    fn third_press_after_a_double_tap_starts_fresh() {
        let mut b = ButtonTracker::new();
        b.press(0);
        b.release(50);
        b.press(100);
        b.release(150);
        assert_eq!(b.press(200), None);
    }

    #[test]
    fn release_without_press_is_ignored() {
        let mut b = ButtonTracker::new();
        assert_eq!(b.release(10), None);
        assert_eq!(b.tick(10), None);
    }
}
