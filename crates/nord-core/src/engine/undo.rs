//! Single-level Undo (manual p.43): reverts the program to the state before the last undoable
//! operation. Snapshots are taken on the control thread; the audio thread never touches them.

use super::*;

/// Undoable operations listed on manual p.43.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoOp {
    LoadPreset,
    SynthGroup,
    FxFocusGroup,
    GlobalFx,
    AllFxOff,
    ClearMorph,
    CopyPaste,
    SynthInit,
    LayerInit,
    ProgramChange,
}

impl UndoOp {
    /// Text for the display ("Undo ...").
    pub fn label(self) -> &'static str {
        match self {
            Self::LoadPreset => "Load Preset",
            Self::SynthGroup => "Synth Group",
            Self::FxFocusGroup => "FX Group",
            Self::GlobalFx => "Global FX",
            Self::AllFxOff => "All FX Off",
            Self::ClearMorph => "Clear Morph",
            Self::CopyPaste => "Paste",
            Self::SynthInit => "Synth Init",
            Self::LayerInit => "Layer Init",
            Self::ProgramChange => "Program Change",
        }
    }
}

type Snapshot = Option<(UndoOp, Box<ProgramState>)>;

/// Holder of the one pending undo snapshot and, after an Undo, the matching redo snapshot.
#[derive(Debug, Clone, Default)]
pub struct UndoSlot(Snapshot, Snapshot);

impl Stage4Engine {
    /// Records the state before an undoable operation (replaces any earlier snapshot).
    /// Allocates; call it on the control thread right before applying the operation.
    pub fn begin_undoable(&mut self, op: UndoOp) {
        let state = self.capture_state();
        self.undo.0 = Some((op, state));
        self.undo.1 = None; // a new operation invalidates the redo branch
    }

    /// The operation Undo would revert, if any.
    pub fn undo_available(&self) -> Option<UndoOp> {
        self.undo.0.as_ref().map(|(op, _)| *op)
    }

    /// Reverts to the state before the last undoable operation; returns it. Single level; the
    /// state it replaced is kept for [`Self::redo`]. Allocates (control thread only).
    pub fn undo(&mut self) -> Option<UndoOp> {
        let (op, state) = self.undo.0.take()?;
        let current = self.capture_state();
        self.apply_state(state);
        self.undo.1 = Some((op, current));
        Some(op)
    }

    /// The operation Redo would re-apply, if an Undo just happened.
    pub fn redo_available(&self) -> Option<UndoOp> {
        self.undo.1.as_ref().map(|(op, _)| *op)
    }

    /// Re-applies the state an Undo replaced and makes the operation undoable again.
    pub fn redo(&mut self) -> Option<UndoOp> {
        let (op, state) = self.undo.1.take()?;
        let before = self.capture_state();
        self.apply_state(state);
        self.undo.0 = Some((op, before));
        Some(op)
    }

    /// Forgets the pending snapshots (the Undo prompt was cancelled).
    pub fn discard_undo(&mut self) {
        self.undo.0 = None;
        self.undo.1 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_reverts_to_state_before_operation_and_is_single_level() {
        let mut e = Stage4Engine::new(48_000.0);
        assert_eq!(e.undo(), None);
        e.layer_levels[0].base = 0.5;
        e.begin_undoable(UndoOp::AllFxOff);
        assert_eq!(e.undo_available(), Some(UndoOp::AllFxOff));
        e.layer_levels[0].base = 0.1;
        e.fx_chains[0].reverb_mix = 0.77;
        // Edits made after the operation are undone as well (manual p.43).
        assert_eq!(e.undo(), Some(UndoOp::AllFxOff));
        assert_eq!(e.layer_levels[0].base, 0.5);
        assert_ne!(e.fx_chains[0].reverb_mix, 0.77);
        assert_eq!(e.undo(), None);
    }

    #[test]
    fn redo_restores_the_undone_state_and_is_cleared_by_a_new_operation() {
        let mut e = Stage4Engine::new(48_000.0);
        assert_eq!(e.redo(), None);
        e.layer_levels[0].base = 0.5;
        e.begin_undoable(UndoOp::LoadPreset);
        e.layer_levels[0].base = 0.1;
        assert_eq!(e.redo_available(), None);
        e.undo();
        assert_eq!(e.layer_levels[0].base, 0.5);
        assert_eq!(e.redo_available(), Some(UndoOp::LoadPreset));
        assert_eq!(e.redo(), Some(UndoOp::LoadPreset));
        assert_eq!(e.layer_levels[0].base, 0.1);
        // Redo is itself undoable again.
        assert_eq!(e.undo_available(), Some(UndoOp::LoadPreset));
        e.undo();
        assert_eq!(e.layer_levels[0].base, 0.5);
        e.begin_undoable(UndoOp::AllFxOff);
        assert_eq!(e.redo_available(), None);
        e.undo();
        e.discard_undo();
        assert_eq!(e.redo_available(), None);
    }

    #[test]
    fn discard_clears_and_labels_exist() {
        let mut e = Stage4Engine::new(48_000.0);
        e.begin_undoable(UndoOp::LoadPreset);
        e.discard_undo();
        assert_eq!(e.undo_available(), None);
        assert_eq!(UndoOp::LayerInit.label(), "Layer Init");
    }
}
