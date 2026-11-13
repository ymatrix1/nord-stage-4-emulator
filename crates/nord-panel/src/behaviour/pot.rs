//! Pot snap: how an on-screen knob takes over after a program load.
//!
//! After a program changes a stored value, the on-screen knob position and the
//! stored value differ. The control must not jump: it ignores input until the
//! pointer crosses or comes within `SNAP_TOLERANCE` of the stored value.
//!
//! ASSUMED: the hardware pots are physical and behave this way ("pick-up");
//! the manual does not describe it. Confirm on hardware (OQ13).

/// Normalized distance within which a knob picks up the stored value.
/// ASSUMED: 2 % of the travel.
pub const SNAP_TOLERANCE: f32 = 0.02;

/// Pick-up tracker for one knob (all values normalized `0..=1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PotTracker {
    stored: f32,
    engaged: bool,
    last_input: Option<f32>,
}

impl PotTracker {
    /// New tracker already engaged (the knob matches the stored value).
    pub fn new(stored: f32) -> Self {
        Self {
            stored: stored.clamp(0.0, 1.0),
            engaged: true,
            last_input: None,
        }
    }

    pub fn stored(&self) -> f32 {
        self.stored
    }

    pub fn is_engaged(&self) -> bool {
        self.engaged
    }

    /// A program load or morph changed the stored value: disengage.
    pub fn stored_changed(&mut self, value: f32) {
        self.stored = value.clamp(0.0, 1.0);
        self.engaged = false;
        self.last_input = None;
    }

    /// Pointer input. Returns the new stored value once engaged.
    pub fn input(&mut self, pos: f32) -> Option<f32> {
        let pos = pos.clamp(0.0, 1.0);
        if !self.engaged {
            let near = (pos - self.stored).abs() <= SNAP_TOLERANCE;
            let crossed = self
                .last_input
                .is_some_and(|l| (l - self.stored) * (pos - self.stored) <= 0.0);
            self.last_input = Some(pos);
            if !(near || crossed) {
                return None;
            }
            self.engaged = true;
        }
        self.stored = pos;
        Some(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engaged_knob_follows_input() {
        let mut p = PotTracker::new(0.5);
        assert_eq!(p.input(0.7), Some(0.7));
        assert_eq!(p.stored(), 0.7);
    }

    #[test]
    fn after_a_change_far_input_is_ignored() {
        let mut p = PotTracker::new(0.5);
        p.stored_changed(0.2);
        assert!(!p.is_engaged());
        assert_eq!(p.input(0.8), None);
        assert_eq!(p.stored(), 0.2);
    }

    #[test]
    fn input_within_tolerance_picks_up() {
        let mut p = PotTracker::new(0.5);
        p.stored_changed(0.2);
        assert_eq!(p.input(0.21), Some(0.21));
        assert!(p.is_engaged());
    }

    #[test]
    fn crossing_the_stored_value_picks_up() {
        let mut p = PotTracker::new(0.5);
        p.stored_changed(0.2);
        assert_eq!(p.input(0.6), None);
        assert_eq!(p.input(0.1), Some(0.1), "crossed 0.2 going down");
    }

    #[test]
    fn values_are_clamped() {
        let mut p = PotTracker::new(2.0);
        assert_eq!(p.stored(), 1.0);
        assert_eq!(p.input(-1.0), Some(0.0));
    }
}
