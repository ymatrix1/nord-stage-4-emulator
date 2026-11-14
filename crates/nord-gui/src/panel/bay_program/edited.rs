//! The boxed `E` of the Program display: the program was edited since it was loaded or stored.
//!
//! VERIFIED: hardware shows a boxed `E` after the location once the sound is edited and clears
//! it after STORE and on load (DSoundman course: Program Locations 5:30 vs 6:20 and 6:30, manual
//! p.13; notes in `docs/research/dsoundman-course-findings.md`).
//!
//! The engine keeps no dirty flag (Undo only tracks the last undoable operation), so the panel
//! counts the sound edits it sends (`AudioHandle::edit_count`) and compares the count with its
//! value at the last load / store.

use crate::panel::ui_state::ProgramUi;

impl ProgramUi {
    /// Called once per frame with the number of sound edits sent so far. A load or store queued
    /// by [`Self::mark_clean`] rebases the baseline here, after the commands it triggered.
    pub fn sync_edits(&mut self, count: u32) {
        self.edit_count = count;
        if self.clean_pending {
            self.edit_baseline = count;
            self.clean_pending = false;
        }
    }

    /// A program was loaded or stored: edits so far no longer count.
    pub fn mark_clean(&mut self) {
        self.clean_pending = true;
    }

    /// The program differs from what was loaded or last stored.
    pub fn is_edited(&self) -> bool {
        !self.clean_pending && self.edit_count != self.edit_baseline
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::{PendingSnapshot, ProgLoc};
    use super::*;

    #[test]
    fn edits_set_the_flag_and_a_load_clears_it() {
        let mut u = ProgramUi::default();
        u.sync_edits(0);
        assert!(!u.is_edited());
        u.sync_edits(3);
        assert!(u.is_edited());
        u.mark_clean();
        assert!(!u.is_edited(), "a load clears at once");
        u.sync_edits(3);
        assert!(!u.is_edited());
        u.sync_edits(4);
        assert!(u.is_edited(), "the next edit marks it again");
    }

    #[test]
    fn commands_sent_by_the_load_itself_do_not_count() {
        let mut u = ProgramUi::default();
        u.mark_clean();
        u.sync_edits(7); // the load sent seven setter commands before the frame ended
        assert!(!u.is_edited());
    }

    #[test]
    fn selecting_a_program_and_storing_clear_the_flag() {
        let mut u = ProgramUi::default();
        u.sync_edits(2);
        assert!(u.is_edited());
        u.select_loc(ProgLoc::parse("A:12").unwrap());
        u.sync_edits(2);
        assert!(!u.is_edited());
        u.sync_edits(5);
        assert!(u.is_edited());
        u.request_snapshot(PendingSnapshot::Bank {
            loc: ProgLoc::parse("A:12").unwrap(),
            name: "X".into(),
            category: 0,
        });
        let ticket = u.pending[0].0;
        let state = nord_core::engine::Stage4Engine::new(48_000.0).capture_state();
        u.accept_snapshot(ticket, state);
        u.sync_edits(5);
        assert!(!u.is_edited(), "STORE clears the E");
    }

    #[test]
    fn an_external_program_change_clears_the_flag() {
        let mut u = ProgramUi::default();
        u.sync_edits(1);
        assert!(u.is_edited());
        u.begin_frame(0, &[false; 128], "B:21", "Other");
        u.sync_edits(1);
        assert!(!u.is_edited());
    }
}
