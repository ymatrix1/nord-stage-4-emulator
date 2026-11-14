//! `AudioHandle`: UI-side sender for audio commands and MIDI events.

use super::AudioCommand;
use crate::telemetry::TelemetryHandle;
use crossbeam_channel::{bounded, Receiver, Sender};
use nord_core::midi::MidiEvent;

/// Handle to communicate with the running audio background thread.
#[derive(Clone)]
pub struct AudioHandle {
    pub cmd_sender: Sender<AudioCommand>,
    /// Lock-free engine to GUI telemetry; the audio thread publishes into it.
    pub telemetry: TelemetryHandle,
}

impl AudioHandle {
    pub fn new() -> (Self, Receiver<AudioCommand>) {
        Self::with_capacity(1024)
    }

    pub fn with_capacity(capacity: usize) -> (Self, Receiver<AudioCommand>) {
        let (tx, rx) = bounded(capacity);
        (
            Self {
                cmd_sender: tx,
                telemetry: TelemetryHandle::new(),
            },
            rx,
        )
    }

    /// Dispatches a MIDI event to the real-time audio thread.
    ///
    /// For non-critical events (e.g. continuous ModWheel, CC, NoteOn), if the event queue is full,
    /// the event is dropped to prevent unbounded latency. For critical note lifecycle events
    /// (`NoteOff`, `AllNotesOff`, `SustainPedal(0)`), this retries with a short timeout
    /// to ensure key releases are not silently dropped.
    pub fn send_midi(&self, event: MidiEvent) -> bool {
        let is_critical = match event {
            MidiEvent::NoteOff { .. } => true,
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                controller == nord_core::midi::cc::ALL_NOTES_OFF
                    || controller == nord_core::midi::cc::ALL_SOUND_OFF
                    || (controller == nord_core::midi::cc::SUSTAIN_PEDAL && value == 0)
            }
            _ => false,
        };
        let cmd = AudioCommand::Midi(event);
        match self.cmd_sender.try_send(cmd) {
            Ok(()) => true,
            Err(crossbeam_channel::TrySendError::Full(returned_cmd)) => {
                if is_critical {
                    self.cmd_sender
                        .send_timeout(returned_cmd, std::time::Duration::from_millis(25))
                        .is_ok()
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub fn send_command(&self, cmd: AudioCommand) -> bool {
        self.cmd_sender.try_send(cmd).is_ok()
    }

    /// The telemetry reader the panel polls once per frame.
    pub fn telemetry(&self) -> &TelemetryHandle {
        &self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_share_one_telemetry_cell() {
        let (handle, _rx) = AudioHandle::new();
        let copy = handle.clone();
        let mut e = nord_core::engine::Stage4Engine::new(48_000.0);
        e.attach_telemetry(handle.telemetry().cell());
        e.publish_telemetry();
        assert!(copy.telemetry().latest().get().is_some());
    }

    #[test]
    fn test_audio_handle_channel() {
        let (handle, rx) = AudioHandle::new();
        handle.send_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
                assert_eq!(note, 60);
                assert_eq!(velocity, 100);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_midi_queue_overflow_policy() {
        let (handle, rx) = AudioHandle::with_capacity(2);

        let mod_wheel = MidiEvent::ControlChange {
            channel: 0,
            controller: nord_core::midi::cc::MOD_WHEEL,
            value: 64,
        };

        // Fill queue to capacity with non-critical events
        assert!(handle.send_midi(mod_wheel));
        assert!(handle.send_midi(mod_wheel));

        // Non-critical event drops immediately when full
        assert!(!handle.send_midi(mod_wheel));

        // Spawn a background consumer synchronized before the 25 ms send_timeout window
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let barrier_bg = std::sync::Arc::clone(&barrier);
        let drain_thread = std::thread::spawn(move || {
            barrier_bg.wait();
            std::thread::sleep(std::time::Duration::from_millis(2));
            let _ = rx.recv();
            let _ = rx.recv();
            rx
        });

        // Critical lifecycle event (NoteOff) waits for drainage and succeeds!
        barrier.wait();
        let note_off = MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        };
        assert!(handle.send_midi(note_off));

        let _rx = drain_thread.join().unwrap();
    }
}
