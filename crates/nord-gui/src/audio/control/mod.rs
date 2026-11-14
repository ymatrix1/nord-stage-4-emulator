//! Control-thread side of the standalone engine (TD-12, spec `docs/specs/program-handoff.md`).
//!
//! The audio callback owns the live `Stage4Engine`; everything that allocates (layer init,
//! copy / paste / swap, Undo snapshots, store, recall) runs here instead. [`ProgramControl`] keeps
//! a **shadow** engine in step with the live one by replaying every panel command (and every
//! non-note MIDI event) onto it. A control operation edits the shadow, captures the result as a
//! `Box<ProgramState>` and sends it to the audio thread as one [`SwapState`] command; the audio
//! thread swaps the selected parts in and returns the old ones, which become the Undo / Redo
//! snapshot here. The audio thread therefore only moves memory.

mod link;
mod ops;
mod swap;

pub use link::ControlLink;
pub use swap::{Retire, Retired, SwapState};

use super::{apply_audio_command, standalone_engine, AudioCommand};
use crossbeam_channel::{bounded, Receiver, Sender};
use nord_core::engine::{ProgramState, Stage4Engine, StateMask, UndoOp};
use nord_core::midi::MidiEvent;
use std::time::{Duration, Instant};

/// Capacity of the return channel (a button press returns one box).
const RETURN_CAPACITY: usize = 16;
/// How long Undo / Redo waits for the audio thread to hand a snapshot back.
const SETTLE_TIMEOUT: Duration = Duration::from_millis(250);

/// One Undo / Redo slot of the control thread.
#[derive(Debug, Default)]
enum Slot {
    #[default]
    Empty,
    /// The swap was sent; the old state is still on its way back from the audio thread.
    Pending { ticket: u32, op: UndoOp, mask: StateMask },
    Ready {
        op: UndoOp,
        mask: StateMask,
        state: Box<ProgramState>,
    },
}

impl Slot {
    fn op(&self) -> Option<UndoOp> {
        match self {
            Self::Empty => None,
            Self::Pending { op, .. } | Self::Ready { op, .. } => Some(*op),
        }
    }
}

/// The shadow engine, the Undo / Redo snapshots and the channel to the audio thread.
pub struct ProgramControl {
    pub(super) shadow: Stage4Engine,
    master_vol: f32,
    undo: Slot,
    redo: Slot,
    ticket: u32,
    ret_tx: Sender<Retired>,
    ret_rx: Receiver<Retired>,
    to_audio: Sender<AudioCommand>,
}

impl ProgramControl {
    /// Shadow of an engine running at `sample_rate`, sending to `to_audio`.
    pub fn new(sample_rate: f32, to_audio: Sender<AudioCommand>) -> Self {
        let (ret_tx, ret_rx) = bounded(RETURN_CAPACITY);
        Self {
            shadow: standalone_engine(sample_rate),
            master_vol: 0.85,
            undo: Slot::Empty,
            redo: Slot::Empty,
            ticket: 0,
            ret_tx,
            ret_rx,
            to_audio,
        }
    }

    /// The shadow engine (tests compare it with the live engine).
    pub fn shadow(&self) -> &Stage4Engine {
        &self.shadow
    }

    /// Operation UNDO would revert / REDO would restore.
    pub fn undo_status(&self) -> (Option<UndoOp>, Option<UndoOp>) {
        (self.undo.op(), self.redo.op())
    }

    /// Handles `cmd`: control operations run here and are consumed (`None`); every other command
    /// is applied to the shadow and returned for the caller to forward to the audio thread.
    pub fn dispatch(&mut self, cmd: AudioCommand) -> Option<AudioCommand> {
        match cmd {
            AudioCommand::Program(edit) if edit.is_control_op() => {
                self.control_op(edit);
                None
            }
            other => {
                self.observe(&other);
                Some(other)
            }
        }
    }

    /// Replays a command the audio thread also receives onto the shadow.
    pub fn observe(&mut self, cmd: &AudioCommand) {
        match cmd {
            // Held keys never change panel state; skipping them keeps shadow voices silent.
            AudioCommand::Midi(MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. }) => {}
            AudioCommand::Midi(ev) => self.shadow.handle_midi(*ev),
            AudioCommand::SwapState(_) => {}
            other => apply_audio_command(other.clone(), &mut self.shadow, &mut self.master_vol),
        }
    }

    /// Moves boxes the audio thread returned into their Undo / Redo slots (never blocks).
    pub fn drain(&mut self) {
        while let Ok(r) = self.ret_rx.try_recv() {
            let slot = match r.to {
                Retire::Undo => &mut self.undo,
                Retire::Redo => &mut self.redo,
            };
            if let Slot::Pending { ticket, op, mask } = *slot {
                if ticket == r.ticket {
                    let mut state = r.state;
                    // The old state may carry sounding voices; a stored snapshot must not.
                    state.release_voices(mask);
                    *slot = Slot::Ready { op, mask, state };
                }
            }
            // A stale answer (its slot was replaced meanwhile) drops here, on this thread.
        }
    }

    /// Waits (polling, never blocking on a channel) until `slot` holds its snapshot.
    fn settle(&mut self, undo: bool) -> bool {
        let t0 = Instant::now();
        loop {
            self.drain();
            let slot = if undo { &self.undo } else { &self.redo };
            match slot {
                Slot::Ready { .. } => return true,
                Slot::Empty => return false,
                Slot::Pending { .. } if t0.elapsed() > SETTLE_TIMEOUT => return false,
                Slot::Pending { .. } => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
