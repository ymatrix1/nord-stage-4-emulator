//! `ControlLink`: the shared door between every sending thread and the [`ProgramControl`].
//!
//! The UI thread and the MIDI thread both send through the `AudioHandle`; the link serialises them
//! onto the shadow engine with a `Mutex`. Both are control threads, so the lock is allowed here (it
//! is never taken on the audio thread). Until the audio thread has opened the output device and
//! knows its sample rate, the link only records (`Waiting`); `attach` builds the shadow, replays
//! the record in order and goes `Active`.

use super::ProgramControl;
use crate::audio::AudioCommand;
use crossbeam_channel::Sender;
use nord_core::midi::MidiEvent;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Commands recorded before the shadow exists (a start-up burst; a device-less session stops
/// recording at the cap instead of growing without bound).
const BACKLOG_CAP: usize = 8192;

enum State {
    Waiting(Vec<AudioCommand>),
    Active(Box<ProgramControl>),
}

/// Shared handle onto the control thread's state.
pub struct ControlLink {
    state: Mutex<State>,
}

impl Default for ControlLink {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlLink {
    /// A link that records until [`Self::attach`].
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::Waiting(Vec::new())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Called by the audio thread once the sample rate is known, before the stream starts.
    pub fn attach(&self, sample_rate: f32, to_audio: Sender<AudioCommand>) {
        let mut control = Box::new(ProgramControl::new(sample_rate, to_audio));
        let mut state = self.lock();
        if let State::Waiting(backlog) = &mut *state {
            for cmd in std::mem::take(backlog) {
                // Plain commands already reached the audio queue; control operations did not.
                control.dispatch(cmd);
            }
        }
        *state = State::Active(control);
    }

    /// `true` once the shadow engine runs.
    pub fn is_active(&self) -> bool {
        matches!(&*self.lock(), State::Active(_))
    }

    /// Sends `cmd`: control operations are executed here, everything else is mirrored onto the
    /// shadow and forwarded to the audio queue.
    pub fn send(&self, cmd: AudioCommand, to_audio: &Sender<AudioCommand>) -> bool {
        let mut state = self.lock();
        match &mut *state {
            State::Active(control) => match control.dispatch(cmd) {
                Some(forward) => to_audio.try_send(forward).is_ok(),
                None => true,
            },
            State::Waiting(backlog) => {
                let deferred =
                    matches!(&cmd, AudioCommand::Program(e) if e.is_control_op());
                if backlog.len() < BACKLOG_CAP {
                    backlog.push(cmd.clone());
                }
                deferred || to_audio.try_send(cmd).is_ok()
            }
        }
    }

    /// Mirrors a MIDI event onto the shadow (called by `AudioHandle::send_midi`).
    pub fn observe_midi(&self, event: MidiEvent) {
        if matches!(event, MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. }) {
            return;
        }
        let mut state = self.lock();
        match &mut *state {
            State::Active(control) => control.observe(&AudioCommand::Midi(event)),
            State::Waiting(backlog) if backlog.len() < BACKLOG_CAP => {
                backlog.push(AudioCommand::Midi(event));
            }
            State::Waiting(_) => {}
        }
    }

    /// Undo / Redo the control thread currently holds.
    pub fn undo_status(&self) -> (Option<nord_core::engine::UndoOp>, Option<nord_core::engine::UndoOp>) {
        match &*self.lock() {
            State::Active(c) => c.undo_status(),
            State::Waiting(_) => (None, None),
        }
    }
}
