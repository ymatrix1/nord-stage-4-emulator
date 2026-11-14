//! CPAL audio output stream and MIDIR MIDI input listener.

use super::frame::{render_frame_to_f32, render_frame_to_i16};
use super::{apply_audio_command, AudioCommand, AudioHandle};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use midir::{Ignore, MidiInput};
use nord_core::engine::{Stage4Engine, TelemetryCell};
use nord_core::midi::MidiEvent;
use nord_core::synth::VaWaveform;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Spawns the audio output stream (CPAL) and MIDI input listener (MIDIR).
pub fn start_audio_and_midi(active_keys: Arc<[AtomicBool; 128]>) -> AudioHandle {
    let (handle, rx) = AudioHandle::new();

    // 1. Spawn Audio Output Thread
    let audio_rx = rx;
    let telemetry = handle.telemetry().cell();
    std::thread::spawn(move || {
        run_cpal_audio(audio_rx, telemetry);
    });

    // 2. Spawn MIDI In Listener
    let midi_handle = handle.clone();
    let midi_keys = Arc::clone(&active_keys);
    std::thread::spawn(move || {
        run_midi_input(midi_handle, midi_keys);
    });

    handle
}

fn run_cpal_audio(rx: Receiver<AudioCommand>, telemetry: Arc<TelemetryCell>) {
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
    // Initialize default levels matching UI state: Organ A + Piano A active, Synth A muted until enabled
    engine.layer_levels[0].base = 0.85;
    engine.layer_levels[1].base = 0.0;
    engine.layer_levels[2].base = 0.90;
    engine.layer_levels[3].base = 0.0;
    engine.layer_levels[4].base = 0.0;
    engine.layer_levels[5].base = 0.0;
    engine.layer_levels[6].base = 0.0;
    engine.synth_a.waveform = VaWaveform::SuperSaw;
    engine.attach_telemetry(telemetry);

    let mut master_vol: f32 = 0.85;

    let err_fn = |err| eprintln!("[Audio] Stream error: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(cmd) = rx.try_recv() {
                    apply_audio_command(cmd, &mut engine, &mut master_vol);
                }

                for frame in data.chunks_mut(channels) {
                    let [l, r] = engine.render_frame();
                    render_frame_to_f32(channels, l * master_vol, r * master_vol, frame);
                }
                engine.publish_telemetry();
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config.into(),
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                while let Ok(cmd) = rx.try_recv() {
                    apply_audio_command(cmd, &mut engine, &mut master_vol);
                }

                for frame in data.chunks_mut(channels) {
                    let [l, r] = engine.render_frame();
                    render_frame_to_i16(channels, l * master_vol, r * master_vol, frame);
                }
                engine.publish_telemetry();
            },
            err_fn,
            None,
        ),
        _ => {
            eprintln!(
                "[Audio] Unsupported sample format: {:?}",
                config.sample_format()
            );
            return;
        }
    };

    if let Ok(s) = stream {
        if let Err(e) = s.play() {
            eprintln!("[Audio] Playback failed: {}", e);
        } else {
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
            MidiEvent::NoteOn { note, velocity, .. } if (note as usize) < active_keys.len() => {
                active_keys[note as usize].store(velocity > 0, Ordering::Relaxed);
            }
            MidiEvent::NoteOff { note, .. } if (note as usize) < active_keys.len() => {
                active_keys[note as usize].store(false, Ordering::Relaxed);
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
    fn test_handle_midi_bytes_note_on_off() {
        let (handle, rx) = AudioHandle::new();
        let active_keys: Arc<[AtomicBool; 128]> =
            Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));

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
}
