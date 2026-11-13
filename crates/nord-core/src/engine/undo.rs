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

/// Holder of the one pending undo snapshot.
#[derive(Debug, Clone, Default)]
pub struct UndoSlot(Option<(UndoOp, Box<ProgramState>)>);

impl Stage4Engine {
    /// Records the state before an undoable operation (replaces any earlier snapshot).
    /// Allocates; call it on the control thread right before applying the operation.
    pub fn begin_undoable(&mut self, op: UndoOp) {
        let state = self.capture_state();
        self.undo.0 = Some((op, state));
    }

    /// The operation Undo would revert, if any.
    pub fn undo_available(&self) -> Option<UndoOp> {
        self.undo.0.as_ref().map(|(op, _)| *op)
    }

    /// Reverts to the state before the last undoable operation; returns it. Single level, no redo.
    pub fn undo(&mut self) -> Option<UndoOp> {
        let (op, state) = self.undo.0.take()?;
        self.apply_state(state);
        Some(op)
    }

    /// Forgets the pending snapshot (the Undo prompt was cancelled).
    pub fn discard_undo(&mut self) {
        self.undo.0 = None;
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
    fn discard_clears_and_labels_exist() {
        let mut e = Stage4Engine::new(48_000.0);
        e.begin_undoable(UndoOp::LoadPreset);
        e.discard_undo();
        assert_eq!(e.undo_available(), None);
        assert_eq!(UndoOp::LayerInit.label(), "Layer Init");
    }
}
