//! Panel behaviour layer (Phase 6b B5, `ns4-q6p7`): gestures, SHIFT, latch
//! modes, selector cycling, pot pick-up, Live Mode, and the `PanelHost` seam.
//! Pure logic with caller-supplied time; no UI toolkit and no audio thread.

pub mod gesture;
pub mod host;
pub mod live;
pub mod pot;
pub mod shift;

pub use gesture::{ButtonTracker, Gesture, DOUBLE_TAP_MS, HOLD_MS};
pub use host::{dispatch, PanelAction, PanelHost};
pub use live::{LiveMode, LIVE_SLOTS};
pub use pot::{PotTracker, SNAP_TOLERANCE};
pub use shift::{cycle, resolve, Latch, Trigger};

#[cfg(test)]
mod tests {
    use super::*;

    /// Hold-to-shift through the whole chain: a long press on a control that
    /// has a shifted function resolves to `Shifted`.
    #[test]
    fn long_press_resolves_to_the_shifted_function() {
        let mut b = ButtonTracker::new();
        b.press(0);
        let g = b.tick(HOLD_MS).unwrap();
        assert_eq!(resolve(g, false, true), Trigger::Shifted);
    }

    /// Double tap on Morph Assign enters the latch; Exit leaves it.
    #[test]
    fn double_tap_latch_flow() {
        let mut b = ButtonTracker::new();
        let mut latch = Latch::default();
        b.press(0);
        b.release(60);
        let g = b.press(150).unwrap();
        if resolve(g, false, false) == Trigger::Latch {
            latch.double_tap();
        }
        assert!(latch.is_active());
        latch.exit();
        assert!(!latch.is_active());
    }
}
