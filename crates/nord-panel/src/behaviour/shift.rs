//! SHIFT resolution, latch modes and selector cycling (manual p.12, p.39,
//! p.43-44).

use super::gesture::Gesture;

/// What a gesture on a control means once SHIFT is taken into account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The function printed on the control.
    Primary,
    /// The function printed below the control (Shift + control, or hold-to-shift).
    Shifted,
    /// Press-and-hold function of a control with no shifted function (Layer Off, SOLO).
    Hold,
    /// Double-tap latch (Morph Assign, Section Edit: manual p.39, p.43).
    Latch,
}

/// Resolve a gesture on a control.
///
/// * `shift_held`: a SHIFT button is down.
/// * `has_shift_fn`: the control has a function printed below it (manual p.44:
///   "Most Shift functions can also be activated by pressing down the
///   corresponding button for a short while").
pub fn resolve(gesture: Gesture, shift_held: bool, has_shift_fn: bool) -> Trigger {
    match gesture {
        Gesture::Tap if shift_held && has_shift_fn => Trigger::Shifted,
        Gesture::Tap => Trigger::Primary,
        Gesture::Hold if has_shift_fn => Trigger::Shifted,
        Gesture::Hold => Trigger::Hold,
        Gesture::DoubleTap => Trigger::Latch,
    }
}

/// A latched mode that stays on without the button held; EXIT clears it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Latch {
    active: bool,
}

impl Latch {
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Double tap enters the latch; a further double tap leaves it.
    pub fn double_tap(&mut self) {
        self.active = !self.active;
    }

    /// Shift/Exit leaves the latch.
    pub fn exit(&mut self) {
        self.active = false;
    }
}

/// Next index of a selector button with `count` LED positions (wraps).
pub fn cycle(current: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if forward {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_is_primary_unless_shift_is_held() {
        assert_eq!(resolve(Gesture::Tap, false, true), Trigger::Primary);
        assert_eq!(resolve(Gesture::Tap, true, true), Trigger::Shifted);
    }

    #[test]
    fn shift_on_a_control_without_shifted_function_stays_primary() {
        assert_eq!(resolve(Gesture::Tap, true, false), Trigger::Primary);
    }

    #[test]
    fn hold_is_hold_to_shift_when_a_shifted_function_exists() {
        assert_eq!(resolve(Gesture::Hold, false, true), Trigger::Shifted);
        assert_eq!(resolve(Gesture::Hold, false, false), Trigger::Hold);
    }

    #[test]
    fn double_tap_latches() {
        assert_eq!(resolve(Gesture::DoubleTap, false, false), Trigger::Latch);
    }

    #[test]
    fn latch_toggles_and_exit_clears() {
        let mut l = Latch::default();
        assert!(!l.is_active());
        l.double_tap();
        assert!(l.is_active());
        l.exit();
        assert!(!l.is_active());
        l.double_tap();
        l.double_tap();
        assert!(!l.is_active());
    }

    #[test]
    fn selector_wraps_both_ways() {
        assert_eq!(cycle(5, 6, true), 0);
        assert_eq!(cycle(0, 6, false), 5);
        assert_eq!(cycle(2, 6, true), 3);
        assert_eq!(cycle(3, 0, true), 0);
    }
}
