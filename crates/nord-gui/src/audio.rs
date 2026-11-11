//! Real-Time Audio Engine Host Thread & CPAL Output Callback.

use crossbeam_channel::{bounded, Receiver, Sender};
use nord_core::midi::MidiEvent;

/// Command sent from the GUI thread to the Real-Time Audio thread.
pub enum AudioCommand {
    Midi(MidiEvent),
    SetMasterVolume(f32),
}

/// Handle to communicate with the running audio background thread.
#[derive(Clone)]
pub struct AudioHandle {
    pub cmd_sender: Sender<AudioCommand>,
}

impl AudioHandle {
    pub fn new() -> (Self, Receiver<AudioCommand>) {
        let (tx, rx) = bounded(256);
        (Self { cmd_sender: tx }, rx)
    }

    pub fn send_midi(&self, event: MidiEvent) {
        let _ = self.cmd_sender.try_send(AudioCommand::Midi(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
