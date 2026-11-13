//! Live Mode storage model (manual p.13, p.44): eight Live program slots whose edits are stored
//! automatically. `crates/nord-panel` `LiveMode` decides when to save (it returns the slot to
//! save); this module holds the data and the engine methods that save and recall it.

use super::*;

/// Number of Live program slots (equals `nord_panel::behaviour::live::LIVE_SLOTS`).
pub const LIVE_SLOT_COUNT: usize = 8;

/// The eight Live program slots. Empty slots recall nothing.
#[derive(Debug, Clone, Default)]
pub struct LiveSlots {
    slots: [Option<Box<ProgramState>>; LIVE_SLOT_COUNT],
}

impl LiveSlots {
    /// Stored state of `slot`.
    pub fn get(&self, slot: usize) -> Option<&ProgramState> {
        self.slots.get(slot)?.as_deref()
    }

    /// Stores `state` into `slot`. Out-of-range slots are ignored.
    pub fn set(&mut self, slot: usize, state: Box<ProgramState>) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = Some(state);
        }
    }

    /// Empties `slot`.
    pub fn clear(&mut self, slot: usize) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = None;
        }
    }

    /// SWAP two Live programs (Organize, manual p.45).
    pub fn swap(&mut self, a: usize, b: usize) {
        if a < LIVE_SLOT_COUNT && b < LIVE_SLOT_COUNT {
            self.slots.swap(a, b);
        }
    }

    /// COPY the program in `slot` for the clipboard.
    pub fn copy(&self, slot: usize) -> Option<Clip> {
        self.slots
            .get(slot)?
            .as_ref()
            .map(|s| Clip::Program(s.clone()))
    }

    /// PASTE a copied program into `slot`.
    pub fn paste(&mut self, slot: usize, clip: &Clip) -> Result<(), ClipboardError> {
        match clip {
            Clip::Program(state) if slot < LIVE_SLOT_COUNT => {
                self.slots[slot] = Some(state.clone());
                Ok(())
            }
            _ => Err(ClipboardError::Mismatch),
        }
    }
}

impl Stage4Engine {
    /// Auto-stores the current program into Live `slot` (leaving the slot or powering off).
    pub fn live_auto_store(&self, slots: &mut LiveSlots, slot: usize) {
        slots.set(slot, self.capture_state());
    }

    /// Recalls Live `slot`; returns `false` when it is empty. Undoable like any program change.
    pub fn live_recall(&mut self, slots: &LiveSlots, slot: usize) -> bool {
        let Some(state) = slots.get(slot) else {
            return false;
        };
        let state = Box::new(state.clone());
        self.begin_undoable(UndoOp::ProgramChange);
        self.apply_state(state);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_store_and_recall_round_trip() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut live = LiveSlots::default();
        assert!(!e.live_recall(&live, 2));
        e.set_transpose(3);
        e.layer_levels[0].base = 0.2;
        e.live_auto_store(&mut live, 2);
        e.set_transpose(0);
        e.layer_levels[0].base = 0.9;
        assert!(e.live_recall(&live, 2));
        assert_eq!(e.transpose(), 3);
        assert_eq!(e.layer_levels[0].base, 0.2);
        assert_eq!(e.undo(), Some(UndoOp::ProgramChange));
        assert_eq!(e.layer_levels[0].base, 0.9);
    }

    #[test]
    fn copy_paste_swap_and_bounds() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut live = LiveSlots::default();
        e.set_transpose(1);
        e.live_auto_store(&mut live, 0);
        e.set_transpose(-2);
        e.live_auto_store(&mut live, 7);
        live.swap(0, 7);
        let mut probe = Stage4Engine::new(48_000.0);
        probe.live_recall(&live, 0);
        assert_eq!(probe.transpose(), -2);
        let clip = live.copy(7).unwrap();
        live.paste(3, &clip).unwrap();
        assert!(live.get(3).is_some());
        assert!(live.paste(8, &clip).is_err());
        assert!(live
            .paste(1, &Clip::Fx(FxParams::defaults(48_000.0)))
            .is_err());
        live.clear(3);
        assert!(live.get(3).is_none());
        live.set(9, e.capture_state());
        assert!(live.get(9).is_none());
    }
}
