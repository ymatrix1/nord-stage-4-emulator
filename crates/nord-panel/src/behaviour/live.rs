//! Live Mode (manual p.13, p.44): eight Live programs that auto-store edits.

/// Number of Live program slots.
pub const LIVE_SLOTS: u8 = 8;

/// Live Mode state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveMode {
    slot: Option<u8>,
    dirty: bool,
}

impl LiveMode {
    /// A Live slot is selected.
    pub fn is_active(&self) -> bool {
        self.slot.is_some()
    }

    /// Selected slot `0..LIVE_SLOTS`.
    pub fn slot(&self) -> Option<u8> {
        self.slot
    }

    /// Select Live slot from Program button `1..=8`. Leaving a dirty slot for
    /// another returns the slot whose edits must be saved first.
    pub fn select(&mut self, button: u8) -> Option<u8> {
        if !(1..=LIVE_SLOTS).contains(&button) {
            return None;
        }
        let save = self.take_dirty();
        self.slot = Some(button - 1);
        save
    }

    /// An edit happened; Live programs auto-store (no manual Store needed).
    pub fn edited(&mut self) {
        if self.slot.is_some() {
            self.dirty = true;
        }
    }

    /// Leave Live Mode. Returns the slot to save when it has unsaved edits.
    pub fn leave(&mut self) -> Option<u8> {
        let save = self.take_dirty();
        self.slot = None;
        save
    }

    fn take_dirty(&mut self) -> Option<u8> {
        if self.dirty {
            self.dirty = false;
            self.slot
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_maps_buttons_one_to_eight_to_slots() {
        let mut l = LiveMode::default();
        assert!(!l.is_active());
        assert_eq!(l.select(1), None);
        assert_eq!(l.slot(), Some(0));
        l.select(8);
        assert_eq!(l.slot(), Some(7));
        assert_eq!(l.select(0), None);
        assert_eq!(l.select(9), None);
        assert_eq!(l.slot(), Some(7), "invalid button keeps the slot");
    }

    #[test]
    fn edits_are_saved_when_switching_slot() {
        let mut l = LiveMode::default();
        l.select(2);
        l.edited();
        assert_eq!(l.select(3), Some(1));
        assert_eq!(l.select(4), None, "clean slot needs no save");
    }

    #[test]
    fn leaving_saves_dirty_slot_and_deactivates() {
        let mut l = LiveMode::default();
        l.select(5);
        l.edited();
        assert_eq!(l.leave(), Some(4));
        assert!(!l.is_active());
        assert_eq!(l.leave(), None);
    }

    #[test]
    fn edits_outside_live_mode_do_not_mark_dirty() {
        let mut l = LiveMode::default();
        l.edited();
        l.select(1);
        assert_eq!(l.leave(), None);
    }
}
