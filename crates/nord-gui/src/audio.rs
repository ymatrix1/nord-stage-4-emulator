//! Real-Time Audio Engine Host Thread & CPAL Output Callback & MIDIR Input.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use midir::{Ignore, MidiInput};
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Commands sent from the UI/MIDI to the real-time DSP thread.
pub enum AudioCommand {
    Midi(MidiEvent),
    SetMasterVolume(f32),
    SetDrawbars([u8; 9]),
    SetOrganLevel(f32),
    SetPianoLevel(f32),
    SetSynthLevel(f32),
    SetSynthCutoff(f32),
    SetSynthResonance(f32),
    SetReverbMix(f32),
    SetRotarySpeed(nord_core::organ::RotarySpeed),
}

/// Handle to communicate with the running audio background thread.
#[derive(Clone)]
pub struct AudioHandle {
    pub cmd_sender: Sender<AudioCommand>,
}

impl AudioHandle {
    pub fn new() -> (Self, Receiver<AudioCommand>) {
        let (tx, rx) = bounded(1024);
        (Self { cmd_sender: tx }, rx)
    }

    pub fn send_midi(&self, event: MidiEvent) {
        let _ = self.cmd_sender.try_send(AudioCommand::Midi(event));
    }

    pub fn send_command(&self, cmd: AudioCommand) {
        let _ = self.cmd_sender.try_send(cmd);
    }
}

/// Spawns the audio output stream (CPAL) and MIDI input listener (MIDIR).
pub fn start_audio_and_midi(active_keys: Arc<[AtomicBool; 128]>) -> AudioHandle {
    let (handle, rx) = AudioHandle::new();

    // 1. Spawn Audio Output Thread
    let audio_rx = rx;
    std::thread::spawn(move || {
        run_cpal_audio(audio_rx);
    });

    // 2. Spawn MIDI In Listener
    let midi_handle = handle.clone();
    let midi_keys = Arc::clone(&active_keys);
    std::thread::spawn(move || {
        run_midi_input(midi_handle, midi_keys);
    });

    handle
}

fn run_cpal_audio(rx: Receiver<AudioCommand>) {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            eprintln!("[Audio] No default audio output device found.");
            return;
        }
    };

    let config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Audio] Failed to get default output config: {}", e);
            return;
        }
    };

    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    let mut engine = Stage4Engine::new(sample_rate);
    let mut master_vol: f32 = 0.85;

    let err_fn = |err| eprintln!("[Audio] Stream error: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Drain queued commands without blocking
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        AudioCommand::Midi(ev) => engine.handle_midi(ev),
                        AudioCommand::SetMasterVolume(v) => master_vol = v.clamp(0.0, 1.0),
                        AudioCommand::SetDrawbars(d) => engine.organ_a.drawbars = d,
                        AudioCommand::SetOrganLevel(lvl) => engine.layer_levels[0].base = lvl,
                        AudioCommand::SetPianoLevel(lvl) => engine.layer_levels[2].base = lvl,
                        AudioCommand::SetSynthLevel(lvl) => engine.layer_levels[4].base = lvl,
                        AudioCommand::SetSynthCutoff(co) => engine.synth_a.cutoff_hz = co,
                        AudioCommand::SetSynthResonance(res) => engine.synth_a.resonance = res,
                        AudioCommand::SetReverbMix(rev) => engine.fx_chains[0].reverb_mix = rev,
                        AudioCommand::SetRotarySpeed(sp) => engine.organ_a.rotary_speed = sp,
                    }
                }

                for frame in data.chunks_mut(channels) {
                    let [l, r] = engine.render_frame();
                    let out_l = l * master_vol;
                    let out_r = r * master_vol;
                    if channels >= 2 {
                        frame[0] = out_l;
                        frame[1] = out_r;
                    } else if channels == 1 {
                        frame[0] = (out_l + out_r) * 0.5;
                    }
                }
            },
            err_fn,
            None,
        ),
        _ => {
            eprintln!("[Audio] Unsupported sample format");
            return;
        }
    };

    if let Ok(s) = stream {
        if let Err(e) = s.play() {
            eprintln!("[Audio] Playback failed: {}", e);
        } else {
            // Keep stream active
            loop {
                std::thread::park();
            }
        }
    }
}

fn run_midi_input(handle: AudioHandle, active_keys: Arc<[AtomicBool; 128]>) {
    let mut midi_in = match MidiInput::new("Nord Stage 4 MIDI In") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[MIDI] Failed to initialize MidiInput: {}", e);
            return;
        }
    };
    midi_in.ignore(Ignore::None);

    let ports = midi_in.ports();
    if ports.is_empty() {
        println!("[MIDI] No MIDI input devices found. Plug in a USB/MIDI keyboard!");
        return;
    }

    println!("[MIDI] Found {} MIDI input port(s):", ports.len());
    for (i, p) in ports.iter().enumerate() {
        if let Ok(name) = midi_in.port_name(p) {
            println!("  [{}] {}", i, name);
        }
    }

    // Connect to the first port
    let in_port = &ports[0];
    let port_name = midi_in
        .port_name(in_port)
        .unwrap_or_else(|_| "Unknown".to_string());
    println!("[MIDI] Connecting to port: {}", port_name);

    let _conn = midi_in.connect(
        in_port,
        "nord-stage-4-input",
        move |_stamp, message, _| {
            handle_midi_bytes(message, &handle, &active_keys);
        },
        (),
    );

    match _conn {
        Ok(_connection) => loop {
            std::thread::park();
        },
        Err(e) => eprintln!("[MIDI] Failed to connect to port: {}", e),
    }
}

/// Parses raw MIDI bytes and updates the atomic key states and audio engine.
pub fn handle_midi_bytes(message: &[u8], handle: &AudioHandle, active_keys: &[AtomicBool; 128]) {
    if let Some(event) = MidiEvent::from_bytes(message) {
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                if (note as usize) < active_keys.len() {
                    active_keys[note as usize].store(velocity > 0, Ordering::Relaxed);
                }
            }
            MidiEvent::NoteOff { note, .. } => {
                if (note as usize) < active_keys.len() {
                    active_keys[note as usize].store(false, Ordering::Relaxed);
                }
            }
            _ => {}
        }
        handle.send_midi(event);
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
    fn test_handle_midi_bytes_note_on_off() {
        let (handle, rx) = AudioHandle::new();
        let active_keys: Arc<[AtomicBool; 128]> =
            Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));

        // Send Note On (Middle C, vel 90)
        let note_on = [0x90, 60, 90];
        handle_midi_bytes(&note_on, &handle, &active_keys);
        assert!(active_keys[60].load(Ordering::Relaxed));

        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
                assert_eq!(note, 60);
                assert_eq!(velocity, 90);
            }
            _ => panic!("Expected NoteOn event"),
        }

        // Send Note Off (velocity 0 NoteOn variant)
        let note_off = [0x90, 60, 0];
        handle_midi_bytes(&note_off, &handle, &active_keys);
        assert!(!active_keys[60].load(Ordering::Relaxed));

        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOff { note, .. })) => {
                assert_eq!(note, 60);
            }
            _ => panic!("Expected NoteOff event"),
        }
    }

    #[test]
    fn test_handle_midi_bytes_cc() {
        let (handle, rx) = AudioHandle::new();
        let active_keys: Arc<[AtomicBool; 128]> =
            Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));

        // Sustain CC (64, 127)
        let cc = [0xB0, 64, 127];
        handle_midi_bytes(&cc, &handle, &active_keys);
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::ControlChange {
                controller, value, ..
            })) => {
                assert_eq!(controller, 64);
                assert_eq!(value, 127);
            }
            _ => panic!("Expected CC event"),
        }
    }
}
