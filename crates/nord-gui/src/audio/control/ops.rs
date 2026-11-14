//! The control operations: what each allocating `ProgramEdit` does on the control thread.
//!
//! Pattern for an edit: run it on the shadow (the clipboard and the old allocating code live
//! there), capture the shadow, and send the parts the edit changed as one [`SwapState`]. Undo and
//! Redo send the snapshot the audio thread returned earlier.

use super::swap::{Retire, SwapState};
use super::{ProgramControl, Slot};
use crate::audio::{AudioCommand, CopyItem, ProgramEdit};
use nord_core::engine::{LayerInitTarget, ProgramState, StateMask, UndoHint, UndoOp};
use nord_core::morph::LayerId;
use std::time::Duration;

/// How long a swap may wait for room in the command queue before the edit is given up.
const SEND_TIMEOUT: Duration = Duration::from_millis(50);

/// Parts of the program a LAYER INIT changes.
fn init_mask(target: LayerInitTarget) -> StateMask {
    let layer = |id| StateMask::layer(id);
    match target {
        LayerInitTarget::All => StateMask::ALL,
        LayerInitTarget::OrganAb => {
            layer(LayerId::OrganA) | layer(LayerId::OrganB) | StateMask::SCENE
        }
        LayerInitTarget::PianoA => layer(LayerId::PianoA),
        LayerInitTarget::PianoB => layer(LayerId::PianoB),
        LayerInitTarget::SynthA => layer(LayerId::SynthA),
        LayerInitTarget::SynthB => layer(LayerId::SynthB),
        LayerInitTarget::SynthC => layer(LayerId::SynthC),
    }
}

/// Parts of the program a paste of `item` changes.
fn paste_mask(item: CopyItem) -> StateMask {
    match item {
        CopyItem::Layer(id) => StateMask::layer(id),
        CopyItem::Fx(id) => StateMask::fx(id),
        CopyItem::Morph(_) => StateMask::MORPH,
        CopyItem::Program => StateMask::ALL,
    }
}

impl ProgramControl {
    /// Runs one allocating edit (see `ProgramEdit::is_control_op`).
    pub(super) fn control_op(&mut self, edit: ProgramEdit) {
        self.drain();
        match edit {
            ProgramEdit::LayerInit(t) => {
                self.shadow.layer_init(t);
                self.send_edit(UndoOp::LayerInit, init_mask(t));
            }
            ProgramEdit::Undo => self.undo_redo(true),
            ProgramEdit::Redo => self.undo_redo(false),
            ProgramEdit::Copy(item) => match item {
                CopyItem::Layer(id) => self.shadow.copy_layer(id),
                CopyItem::Fx(id) => self.shadow.copy_fx(id),
                CopyItem::Morph(s) => self.shadow.copy_morph(s),
                CopyItem::Program => self.shadow.copy_program(),
            },
            ProgramEdit::Paste(item) => {
                // A refused paste (empty or mismatching clipboard) changes nothing.
                let done = match item {
                    CopyItem::Layer(id) => self.shadow.paste_layer(id),
                    CopyItem::Fx(id) => self.shadow.paste_fx(id),
                    CopyItem::Morph(s) => self.shadow.paste_morph(s),
                    CopyItem::Program => self.shadow.paste_program(),
                };
                if done.is_ok() {
                    self.send_edit(UndoOp::CopyPaste, paste_mask(item));
                }
            }
            ProgramEdit::Swap(a, b) => {
                if self.shadow.swap_layers(a, b).is_ok() {
                    self.send_edit(UndoOp::CopyPaste, StateMask::layer(a) | StateMask::layer(b));
                }
            }
            ProgramEdit::ClearMorph(s) => {
                self.shadow.clear_morph(s);
                self.send_edit(UndoOp::ClearMorph, StateMask::MORPH);
            }
            ProgramEdit::Snapshot { ticket, reply } => {
                let _ = reply.send((ticket, self.shadow.capture_state()));
            }
            ProgramEdit::Recall(state) => {
                self.shadow.apply_state(state.clone());
                self.send_state(UndoOp::ProgramChange, StateMask::ALL, state);
            }
            // Not control operations: `dispatch` never routes them here.
            _ => {}
        }
        // The shadow's own single-level Undo is unused; free its snapshot now.
        self.shadow.discard_undo();
    }

    /// Sends the parts `mask` of the shadow (after an edit) as a new undoable operation.
    fn send_edit(&mut self, op: UndoOp, mask: StateMask) {
        let state = self.shadow.capture_state();
        self.send_state(op, mask, state);
    }

    /// New undoable operation: `state` goes in, the old parts come back as the Undo snapshot.
    fn send_state(&mut self, op: UndoOp, mask: StateMask, state: Box<ProgramState>) {
        let ticket = self.next_ticket();
        // The previous Undo and Redo snapshots (and any box still in flight for them) are dropped
        // here, on the control thread; a new operation invalidates the Redo branch.
        self.undo = Slot::Pending { ticket, op, mask };
        self.redo = Slot::Empty;
        let hint = UndoHint {
            undo: Some(op),
            redo: None,
        };
        if !self.send_swap(state, mask, Retire::Undo, ticket, hint) {
            self.undo = Slot::Empty;
        }
    }

    /// UNDO (`undo = true`) or REDO: swap the stored snapshot back in; the audio thread returns
    /// what it replaced for the opposite slot.
    fn undo_redo(&mut self, undo: bool) {
        if !self.settle(undo) {
            return;
        }
        let slot = std::mem::take(if undo { &mut self.undo } else { &mut self.redo });
        let Slot::Ready { op, mask, state } = slot else {
            return;
        };
        // Keep the shadow in step: it takes the same parts (from a copy; the box goes to audio).
        let mut copy = state.clone();
        self.shadow.swap_state(&mut copy, mask);
        drop(copy);
        let ticket = self.next_ticket();
        let (retire, hint) = if undo {
            self.redo = Slot::Pending { ticket, op, mask };
            (
                Retire::Redo,
                UndoHint {
                    undo: None,
                    redo: Some(op),
                },
            )
        } else {
            self.undo = Slot::Pending { ticket, op, mask };
            (
                Retire::Undo,
                UndoHint {
                    undo: Some(op),
                    redo: None,
                },
            )
        };
        if !self.send_swap(state, mask, retire, ticket, hint) {
            self.undo = Slot::Empty;
            self.redo = Slot::Empty;
        }
    }

    fn next_ticket(&mut self) -> u32 {
        self.ticket = self.ticket.wrapping_add(1).max(1);
        self.ticket
    }

    /// Queues the swap for the audio thread; `false` when the command queue stayed full.
    fn send_swap(
        &mut self,
        state: Box<ProgramState>,
        mask: StateMask,
        retire: Retire,
        ticket: u32,
        hint: UndoHint,
    ) -> bool {
        let swap = SwapState {
            state,
            mask,
            retire,
            ticket,
            hint,
            ret: self.ret_tx.clone(),
        };
        self.to_audio
            .send_timeout(AudioCommand::SwapState(swap), SEND_TIMEOUT)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::morph::MorphSource;

    #[test]
    fn masks_select_exactly_what_an_edit_changes() {
        let piano = init_mask(LayerInitTarget::PianoB);
        assert!(piano.contains(StateMask::panel(LayerId::PianoB)));
        assert!(!piano.contains(StateMask::panel(LayerId::PianoA)));
        assert!(!piano.contains(StateMask::SCENE));
        let organ = init_mask(LayerInitTarget::OrganAb);
        assert!(organ.contains(StateMask::SCENE) && organ.contains(StateMask::fx(LayerId::OrganB)));
        assert_eq!(init_mask(LayerInitTarget::All), StateMask::ALL);
        assert_eq!(paste_mask(CopyItem::Program), StateMask::ALL);
        assert_eq!(
            paste_mask(CopyItem::Morph(MorphSource::ModWheel)),
            StateMask::MORPH
        );
        let fx = paste_mask(CopyItem::Fx(LayerId::PianoA));
        assert_eq!(fx, StateMask::fx(LayerId::PianoA));
        assert!(!fx.contains(StateMask::panel(LayerId::PianoA)));
    }
}
