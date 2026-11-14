//! The one command that moves program state to the audio thread, and the box that comes back.
//!
//! The control thread builds a `Box<ProgramState>` and sends it in [`SwapState`]. The audio thread
//! exchanges the selected parts with `Stage4Engine::swap_state` (moves only) and returns the box,
//! now holding the old parts, through a bounded channel. Nothing is allocated or freed on the audio
//! thread; the old state is dropped (or kept as Undo / Redo) on the control thread.

use crossbeam_channel::Sender;
use nord_core::engine::{ProgramState, Stage4Engine, StateMask, UndoHint};

/// Which control-thread slot the returned box belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retire {
    /// The old state becomes the Undo snapshot.
    Undo,
    /// The old state becomes the Redo snapshot.
    Redo,
}

/// What the audio thread hands back.
#[derive(Debug)]
pub struct Retired {
    pub to: Retire,
    /// Matches the [`SwapState::ticket`] it answers; stale answers are dropped.
    pub ticket: u32,
    pub state: Box<ProgramState>,
}

/// Swap `state` into the engine for the parts `mask` selects.
#[derive(Debug, Clone)]
pub struct SwapState {
    pub state: Box<ProgramState>,
    pub mask: StateMask,
    pub retire: Retire,
    pub ticket: u32,
    /// Undo / Redo availability after this swap (published to the panel).
    pub hint: UndoHint,
    /// Bounded return channel; the control thread only ever polls it (`try_recv`).
    pub ret: Sender<Retired>,
}

impl SwapState {
    /// Runs on the audio thread: swap, set the hint, return the old state. No allocation, no free,
    /// no lock (`try_send` on a bounded array channel nobody blocks on).
    pub fn apply(self, engine: &mut Stage4Engine) {
        let Self {
            mut state,
            mask,
            retire,
            ticket,
            hint,
            ret,
        } = self;
        engine.swap_state(&mut state, mask);
        engine.set_undo_hint(hint);
        let answer = Retired {
            to: retire,
            ticket,
            state,
        };
        if let Err(full) = ret.try_send(answer) {
            // Never free on the audio thread: leak the box and count it (should stay 0).
            engine.note_retire_overflow();
            std::mem::forget(full.into_inner());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_swaps_returns_the_old_state_and_sets_the_hint() {
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[2].base = 0.3;
        let incoming = e.capture_state();
        e.layer_levels[2].base = 0.9;
        let (tx, rx) = crossbeam_channel::bounded(4);
        let hint = UndoHint {
            undo: Some(nord_core::engine::UndoOp::LayerInit),
            redo: None,
        };
        SwapState {
            state: incoming,
            mask: StateMask::ALL,
            retire: Retire::Undo,
            ticket: 7,
            hint,
            ret: tx,
        }
        .apply(&mut e);
        assert_eq!(e.layer_levels[2].base, 0.3);
        assert_eq!(e.undo_hint(), hint);
        let back = rx.try_recv().expect("old state returned");
        assert_eq!((back.to, back.ticket), (Retire::Undo, 7));
        // The returned box holds what the engine had (0.9): swapping it back restores it.
        let mut old = back.state;
        e.swap_state(&mut old, StateMask::ALL);
        assert_eq!(e.layer_levels[2].base, 0.9);
    }

    #[test]
    fn a_full_return_queue_leaks_instead_of_freeing_and_counts() {
        let mut e = Stage4Engine::new(48_000.0);
        let (tx, rx) = crossbeam_channel::bounded(1);
        for t in 0..2 {
            SwapState {
                state: e.capture_state(),
                mask: StateMask::NONE,
                retire: Retire::Redo,
                ticket: t,
                hint: UndoHint::default(),
                ret: tx.clone(),
            }
            .apply(&mut e);
        }
        assert_eq!(rx.len(), 1);
        assert_eq!(e.retire_overflow(), 1);
    }
}
