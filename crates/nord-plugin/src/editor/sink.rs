//! `PanelSink`: where the shared front panel sends its edits.
//!
//! The panel (`nord-gui`) emits `AudioCommand`s through an `AudioHandle`. In the standalone app
//! the receiving end is the DSP thread (`apply_audio_command`). In the plugin the receiving end
//! is a [`ParamSink`] on the GUI thread: parameter edits become host parameter writes (so the
//! host records automation and `process()` reads them back), keybed MIDI goes to the
//! lock-free note queue, and anything else is reported as unmapped. The engine is never touched
//! from the editor.

use super::map::map_command;
use super::notes::NoteSender;
use super::write::{ParamWrite, ParamWriter};
use crossbeam_channel::Receiver;
use nord_core::midi::{cc, MidiEvent};
use nord_gui::audio::AudioCommand;

/// What a sink did with one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkOutcome {
    /// This many host parameters were written.
    Params(usize),
    /// A keybed / MIDI event was queued for `process()`.
    Midi,
    /// The command has no host parameter and was dropped (its control is dimmed).
    Unmapped,
}

/// Receiver of panel commands.
pub trait PanelSink {
    fn apply(&mut self, cmd: &AudioCommand) -> SinkOutcome;
}

/// Sink writing host parameters through a [`ParamWriter`] and notes into the note queue.
pub struct ParamSink<'a, W: ParamWriter> {
    pub writer: W,
    pub notes: &'a NoteSender,
    scratch: Vec<ParamWrite>,
    /// Commands dropped for lack of a host parameter since creation.
    pub unmapped: u64,
    /// Note events dropped because the queue was full.
    pub dropped_notes: u64,
}

impl<'a, W: ParamWriter> ParamSink<'a, W> {
    pub fn new(writer: W, notes: &'a NoteSender) -> Self {
        Self {
            writer,
            notes,
            scratch: Vec::with_capacity(16),
            unmapped: 0,
            dropped_notes: 0,
        }
    }
}

impl<W: ParamWriter> PanelSink for ParamSink<'_, W> {
    fn apply(&mut self, cmd: &AudioCommand) -> SinkOutcome {
        self.scratch.clear();
        let known = map_command(cmd, &mut self.scratch);
        let mut written = 0;
        for w in &self.scratch {
            if self.writer.write(w) {
                written += 1;
            }
        }
        match cmd {
            // The mod wheel is a host parameter; every other MIDI event is played.
            AudioCommand::Midi(ev) if !is_mod_wheel(ev) => {
                if !self.notes.push(*ev) {
                    self.dropped_notes += 1;
                }
                SinkOutcome::Midi
            }
            _ if known => SinkOutcome::Params(written),
            _ => {
                self.unmapped += 1;
                SinkOutcome::Unmapped
            }
        }
    }
}

fn is_mod_wheel(ev: &MidiEvent) -> bool {
    matches!(ev, MidiEvent::ControlChange { controller, .. } if *controller == cc::MOD_WHEEL)
}

/// Drain every pending command from the panel's channel into `sink`; returns how many.
pub fn pump(rx: &Receiver<AudioCommand>, sink: &mut dyn PanelSink) -> usize {
    let mut n = 0;
    while let Ok(cmd) = rx.try_recv() {
        sink.apply(&cmd);
        n += 1;
    }
    n
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;

    /// Records writes instead of calling the host.
    #[derive(Default)]
    pub struct Recorder {
        pub writes: Vec<ParamWrite>,
        pub known_ids: Option<std::collections::HashSet<String>>,
    }

    impl ParamWriter for Recorder {
        fn write(&mut self, w: &ParamWrite) -> bool {
            if let Some(ids) = &self.known_ids {
                if !ids.contains(&w.host_id()) {
                    return false;
                }
            }
            self.writes.push(*w);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::Recorder;
    use super::*;
    use crate::editor::notes::note_queue;
    use nord_gui::audio::AudioHandle;

    #[test]
    fn param_edits_are_written_and_notes_are_queued() {
        let (tx, rx) = note_queue(8);
        let mut sink = ParamSink::new(Recorder::default(), &tx);
        assert_eq!(
            sink.apply(&AudioCommand::SetMasterVolume(0.3)),
            SinkOutcome::Params(1)
        );
        let on = MidiEvent::NoteOn {
            channel: 0,
            note: 64,
            velocity: 80,
        };
        assert_eq!(sink.apply(&AudioCommand::Midi(on)), SinkOutcome::Midi);
        assert_eq!(rx.pop(), Some(on));
        assert_eq!(sink.writer.writes.len(), 1);
    }

    #[test]
    fn mod_wheel_cc_is_a_parameter_not_a_note_event() {
        let (tx, rx) = note_queue(8);
        let mut sink = ParamSink::new(Recorder::default(), &tx);
        let out = sink.apply(&AudioCommand::Midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::MOD_WHEEL,
            value: 64,
        }));
        assert_eq!(out, SinkOutcome::Params(1));
        assert!(rx.is_empty());
    }

    #[test]
    fn unmapped_commands_are_counted_and_do_nothing() {
        let (tx, _rx) = note_queue(8);
        let mut sink = ParamSink::new(Recorder::default(), &tx);
        assert_eq!(
            sink.apply(&AudioCommand::SetRotaryEnabled(true)),
            SinkOutcome::Unmapped
        );
        assert_eq!(sink.unmapped, 1);
        assert!(sink.writer.writes.is_empty());
    }

    #[test]
    fn full_note_queue_counts_dropped_notes() {
        let (tx, _rx) = note_queue(1);
        let mut sink = ParamSink::new(Recorder::default(), &tx);
        let on = AudioCommand::Midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 1,
        });
        sink.apply(&on);
        sink.apply(&on);
        assert_eq!(sink.dropped_notes, 1);
    }

    #[test]
    fn pump_drains_the_panel_channel_through_the_sink() {
        let (handle, rx) = AudioHandle::new();
        let (tx, _notes) = note_queue(8);
        handle.send_command(AudioCommand::SetMasterVolume(0.1));
        handle.send_command(AudioCommand::SetSynthCutoff(500.0));
        let mut sink = ParamSink::new(Recorder::default(), &tx);
        assert_eq!(pump(&rx, &mut sink), 2);
        assert_eq!(sink.writer.writes.len(), 2);
        assert_eq!(pump(&rx, &mut sink), 0);
    }

    #[test]
    fn writer_failures_are_not_counted_as_written() {
        let (tx, _rx) = note_queue(8);
        let rec = Recorder {
            known_ids: Some(Default::default()),
            ..Default::default()
        };
        let mut sink = ParamSink::new(rec, &tx);
        assert_eq!(
            sink.apply(&AudioCommand::SetMasterVolume(0.3)),
            SinkOutcome::Params(0)
        );
    }
}
