//! `AudioCommand` message type and the `AudioHandle` command queue (UI/MIDI to DSP thread).

use crossbeam_channel::{bounded, Receiver, Sender};
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::midi::MidiEvent;
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};
use std::sync::Arc;

/// Commands sent from the UI/MIDI to the real-time DSP thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Midi(MidiEvent),
    SetMasterVolume(f32),
    SetDrawbars([u8; 9]),
    SetOrganModel(OrganModel),
    SetScannerMode(ScannerMode),
    SetScannerOn(bool),
    SetPercussion {
        enabled: bool,
        soft: bool,
        fast: bool,
        third: bool,
    },
    SetRotarySpeed(RotarySpeed),
    SetRotaryDrive(f32),
    SetRotaryEnabled(bool),
    SetOrganLevel(f32),
    SetOrganBLevel(f32),
    SetPianoLevel(f32),
    SetPianoBLevel(f32),
    SetPianoCategory(PianoCategory),
    SetPianoTimbre(PianoTimbre),
    SetPianoTouch(VelocityTouch),
    SetPianoAcoustics {
        string_res: bool,
        pedal_noise: bool,
    },
    SetSynthLevel(f32),
    SetSynthBLevel(f32),
    SetSynthCLevel(f32),
    SetSynthOscMode(SynthOscMode),
    SetSynthWaveform(VaWaveform),
    SetSynthDetune(f32),
    SetSynthFilterModel(FilterModel),
    SetSynthCutoff(f32),
    SetSynthResonance(f32),
    SetSynthDrive(f32),
    SetSynthAttack(f32),
    SetSynthDecay(f32),
    SetSynthRelease(f32),
    SetFxMod1 {
        kind: Mod1Type,
        rate: f32,
        amount: f32,
    },
    SetFxMod2 {
        kind: Mod2Type,
        rate: f32,
        amount: f32,
    },
    SetFxEq {
        drive: f32,
        bass: f32,
        mid: f32,
        mid_freq: f32,
        treble: f32,
    },
    SetFxDelay {
        enabled: bool,
        feedback: f32,
        mix: f32,
    },
    SetFxReverb {
        model: ReverbModel,
        mix: f32,
    },
    SetMasterComp(f32),
    /// Activates/deactivates one of the 7 layers (index 0..=6) in both scenes.
    SetLayerActive(usize, bool),
    /// Loads a complete ProgramDecoded into the DSP engine.
    LoadProgram(Box<nord_core::cbin::ProgramDecoded>),
    /// Swaps the pre-decoded `.npno` or SFZ piano library on Piano A (`slot_b = false`) or B (`slot_b = true`).
    LoadPianoSound {
        slot_b: bool,
        sound: Option<Arc<nord_core::piano::NpnoSound>>,
    },
    /// Swaps the pre-decoded `.nsmp4` multisample bank on Synth A (`0`), B (`1`), or C (`2`).
    LoadSynthSample {
        layer_idx: usize,
        sound: Option<Arc<nord_core::synth::SynthSampleSound>>,
    },
}

/// Handle to communicate with the running audio background thread.
#[derive(Clone)]
pub struct AudioHandle {
    pub cmd_sender: Sender<AudioCommand>,
}

impl AudioHandle {
    pub fn new() -> (Self, Receiver<AudioCommand>) {
        Self::with_capacity(1024)
    }

    pub fn with_capacity(capacity: usize) -> (Self, Receiver<AudioCommand>) {
        let (tx, rx) = bounded(capacity);
        (Self { cmd_sender: tx }, rx)
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
