//! Real-Time Audio Engine Host Thread & CPAL Output Callback & MIDIR Input.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use midir::{Ignore, MidiInput};
use nord_core::engine::Stage4Engine;
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::midi::MidiEvent;
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};
use std::sync::atomic::{AtomicBool, Ordering};
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
        // MIDI events preserve note-on/note-off ordering; wait for queue capacity rather than
        // silently dropping an event and leaving a voice held indefinitely.
        let _ = self.cmd_sender.send(AudioCommand::Midi(event));
    }

    pub fn send_command(&self, cmd: AudioCommand) {
        let _ = self.cmd_sender.try_send(cmd);
    }
}

/// Applies a single `AudioCommand` to the `Stage4Engine` and `master_vol`.
pub fn apply_audio_command(cmd: AudioCommand, engine: &mut Stage4Engine, master_vol: &mut f32) {
    match cmd {
        AudioCommand::Midi(ev) => engine.handle_midi(ev),
        AudioCommand::SetMasterVolume(v) => *master_vol = v.clamp(0.0, 1.0),
        AudioCommand::SetDrawbars(d) => engine.organ_a.drawbars = d,
        AudioCommand::SetOrganModel(m) => {
            engine.organ_a.model = m;
            engine.organ_b.model = m;
        }
        AudioCommand::SetScannerMode(sm) => {
            engine.organ_a.scanner_mode = sm;
        }
        AudioCommand::SetScannerOn(on) => {
            if !on {
                engine.organ_a.scanner_mode = ScannerMode::Off;
            } else if engine.organ_a.scanner_mode == ScannerMode::Off {
                engine.organ_a.scanner_mode = ScannerMode::C3;
            }
        }
        AudioCommand::SetPercussion {
            enabled,
            soft,
            fast,
            third,
        } => {
            engine.organ_a.percussion_enabled = enabled;
            engine.organ_a.percussion_soft = soft;
            engine.organ_a.percussion_fast_decay = fast;
            engine.organ_a.percussion_third_harmonic = third;
        }
        AudioCommand::SetRotarySpeed(sp) => {
            engine.organ_a.rotary_speed = sp;
            engine.organ_b.rotary_speed = sp;
        }
        AudioCommand::SetRotaryDrive(drv) => {
            engine.organ_a.rotary_drive = drv;
            engine.organ_b.rotary_drive = drv;
        }
        AudioCommand::SetRotaryEnabled(en) => {
            engine.organ_a.rotary_enabled = en;
            engine.organ_b.rotary_enabled = en;
        }
        AudioCommand::SetOrganLevel(lvl) => engine.layer_levels[0].base = lvl,
        AudioCommand::SetOrganBLevel(lvl) => engine.layer_levels[1].base = lvl,
        AudioCommand::SetPianoLevel(lvl) => engine.layer_levels[2].base = lvl,
        AudioCommand::SetPianoBLevel(lvl) => engine.layer_levels[3].base = lvl,
        AudioCommand::SetPianoCategory(cat) => {
            engine.piano_a.category = cat;
            engine.piano_b.category = cat;
        }
        AudioCommand::SetPianoTimbre(t) => {
            engine.piano_a.timbre = t;
            engine.piano_b.timbre = t;
        }
        AudioCommand::SetPianoTouch(tc) => {
            engine.piano_a.touch = tc;
            engine.piano_b.touch = tc;
        }
        AudioCommand::SetPianoAcoustics {
            string_res,
            pedal_noise,
        } => {
            engine.piano_a.string_resonance_enabled = string_res;
            engine.piano_a.pedal_noise_enabled = pedal_noise;
        }
        AudioCommand::SetSynthLevel(lvl) => engine.layer_levels[4].base = lvl,
        AudioCommand::SetSynthBLevel(lvl) => engine.layer_levels[5].base = lvl,
        AudioCommand::SetSynthCLevel(lvl) => engine.layer_levels[6].base = lvl,
        AudioCommand::SetSynthOscMode(m) => engine.synth_a.osc_mode = m,
        AudioCommand::SetSynthWaveform(w) => engine.synth_a.waveform = w,
        AudioCommand::SetSynthDetune(d) => engine.synth_a.supersaw_detune = d,
        AudioCommand::SetSynthFilterModel(fm) => engine.synth_a.filter_model = fm,
        AudioCommand::SetSynthCutoff(co) => engine.synth_a.cutoff_hz = co,
        AudioCommand::SetSynthResonance(res) => engine.synth_a.resonance = res,
        AudioCommand::SetSynthDrive(drv) => engine.synth_a.filter_drive = drv,
        AudioCommand::SetSynthAttack(atk) => engine.synth_a.attack_sec = atk,
        AudioCommand::SetSynthDecay(dec) => engine.synth_a.decay_sec = dec,
        AudioCommand::SetSynthRelease(rel) => engine.synth_a.release_sec = rel,
        AudioCommand::SetFxMod1 { kind, rate, amount } => {
            for fx in &mut engine.fx_chains {
                fx.mod1_type = kind;
                fx.mod1_rate_hz = rate;
                fx.mod1_amount = amount;
            }
        }
        AudioCommand::SetFxMod2 { kind, rate, amount } => {
            for fx in &mut engine.fx_chains {
                fx.mod2_type = kind;
                fx.mod2_rate_hz = rate;
                fx.mod2_amount = amount;
            }
        }
        AudioCommand::SetFxEq {
            drive,
            bass,
            mid,
            mid_freq,
            treble,
        } => {
            for fx in &mut engine.fx_chains {
                fx.drive_amount = drive;
                fx.eq_bass_db = bass;
                fx.eq_mid_db = mid;
                fx.eq_mid_freq_hz = mid_freq;
                fx.eq_treble_db = treble;
            }
        }
        AudioCommand::SetFxDelay {
            enabled,
            feedback,
            mix,
        } => {
            for fx in &mut engine.fx_chains {
                fx.delay_enabled = enabled;
                fx.delay_feedback = feedback;
                fx.delay_mix = mix;
            }
        }
        AudioCommand::SetFxReverb { model, mix } => {
            for fx in &mut engine.fx_chains {
                fx.reverb_model = model;
                fx.reverb_mix = mix;
            }
        }
        AudioCommand::SetMasterComp(amt) => {
            engine.master_bus.compressor_amount = amt;
        }
        AudioCommand::SetLayerActive(idx, on) => {
            if idx < 7 {
                let bit = 1u8 << idx;
                if on {
                    engine.scene.scene_1_mask |= bit;
                    engine.scene.scene_2_mask |= bit;
                } else {
                    engine.scene.scene_1_mask &= !bit;
                    engine.scene.scene_2_mask &= !bit;
                    // Release anything still sounding on that layer.
                    for n in 0..128u8 {
                        match idx {
                            0 => engine.organ_a.note_off(n),
                            1 => engine.organ_b.note_off(n),
                            2 => engine.piano_a.note_off(n),
                            3 => engine.piano_b.note_off(n),
                            4 => engine.synth_a.note_off(n),
                            5 => engine.synth_b.note_off(n),
                            _ => engine.synth_c.note_off(n),
                        }
                    }
                }
            }
        }
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
    if channels == 0 {
        eprintln!("[Audio] Output device reported zero channels.");
        return;
    }
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();

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

    let mut master_vol: f32 = 0.85;

    let err_fn = |err| eprintln!("[Audio] Stream error: {}", err);

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                render_output_f32(data, channels, &rx, &mut engine, &mut master_vol);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                render_output_i16(data, channels, &rx, &mut engine, &mut master_vol);
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
            loop {
                std::thread::park();
            }
        }
    }
}

fn drain_audio_commands(
    rx: &Receiver<AudioCommand>,
    engine: &mut Stage4Engine,
    master_vol: &mut f32,
) {
    while let Ok(cmd) = rx.try_recv() {
        apply_audio_command(cmd, engine, master_vol);
    }
}

fn render_output_f32(
    data: &mut [f32],
    channels: usize,
    rx: &Receiver<AudioCommand>,
    engine: &mut Stage4Engine,
    master_vol: &mut f32,
) {
    if channels == 0 {
        return;
    }
    drain_audio_commands(rx, engine, master_vol);
    for frame in data.chunks_mut(channels) {
        let [left, right] = engine.render_frame();
        write_f32_frame(frame, [left * *master_vol, right * *master_vol]);
    }
}

fn render_output_i16(
    data: &mut [i16],
    channels: usize,
    rx: &Receiver<AudioCommand>,
    engine: &mut Stage4Engine,
    master_vol: &mut f32,
) {
    if channels == 0 {
        return;
    }
    drain_audio_commands(rx, engine, master_vol);
    for frame in data.chunks_mut(channels) {
        let [left, right] = engine.render_frame();
        write_i16_frame(frame, [left * *master_vol, right * *master_vol]);
    }
}

fn write_f32_frame(frame: &mut [f32], stereo: [f32; 2]) {
    frame.fill(0.0);
    match frame.len() {
        0 => {}
        1 => frame[0] = (stereo[0] + stereo[1]) * 0.5,
        _ => {
            frame[0] = stereo[0];
            frame[1] = stereo[1];
        }
    }
}

fn write_i16_frame(frame: &mut [i16], stereo: [f32; 2]) {
    frame.fill(0);
    match frame.len() {
        0 => {}
        1 => frame[0] = float_to_i16((stereo[0] + stereo[1]) * 0.5),
        _ => {
            frame[0] = float_to_i16(stereo[0]);
            frame[1] = float_to_i16(stereo[1]);
        }
    }
}

fn float_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32768.0)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
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

    #[test]
    fn test_apply_audio_command_updates_engine() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut master = 0.85;
        apply_audio_command(
            AudioCommand::SetOrganModel(OrganModel::Farfisa),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.organ_a.model, OrganModel::Farfisa);

        apply_audio_command(
            AudioCommand::SetSynthCutoff(1234.0),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.synth_a.cutoff_hz, 1234.0);
    }

    #[test]
    fn test_layer_active_toggles_scene_masks() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut master = 0.85;
        apply_audio_command(
            AudioCommand::SetLayerActive(1, true),
            &mut engine,
            &mut master,
        );
        assert_ne!(engine.scene.scene_1_mask & 0b10, 0);
        apply_audio_command(
            AudioCommand::SetLayerActive(1, false),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.scene.scene_1_mask & 0b10, 0);
    }

    #[test]
    fn output_frames_downmix_mono_and_zero_extra_channels() {
        let mut mono = [99.0];
        write_f32_frame(&mut mono, [0.25, -0.75]);
        assert_eq!(mono, [-0.25]);

        let mut surround = [99.0; 4];
        write_f32_frame(&mut surround, [0.25, -0.75]);
        assert_eq!(surround, [0.25, -0.75, 0.0, 0.0]);
    }

    #[test]
    fn i16_output_converts_clamps_and_clears_extra_channels() {
        let mut stereo = [0; 4];
        write_i16_frame(&mut stereo, [2.0, -2.0]);
        assert_eq!(stereo, [i16::MAX, i16::MIN, 0, 0]);

        let mut mono = [0];
        write_i16_frame(&mut mono, [0.5, 0.0]);
        assert_eq!(mono, [8192]);
    }

    #[test]
    fn midi_note_off_is_delivered_when_command_queue_is_full() {
        let (handle, rx) = AudioHandle::new();
        for _ in 0..1024 {
            handle.send_command(AudioCommand::SetMasterVolume(0.5));
        }

        let midi_sender = handle.clone();
        let sender = std::thread::spawn(move || {
            midi_sender.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: 60,
                velocity: 0,
            });
        });

        // Free one slot, allowing the blocked MIDI sender to enqueue its Note Off.
        rx.recv().expect("queue was filled");
        sender.join().expect("MIDI sender should complete");

        let mut received_note_off = false;
        while let Ok(command) = rx.try_recv() {
            if matches!(
                command,
                AudioCommand::Midi(MidiEvent::NoteOff { note: 60, .. })
            ) {
                received_note_off = true;
            }
        }
        assert!(received_note_off, "Note Off must survive queue saturation");
    }

    #[test]
    fn render_callbacks_accept_zero_channel_buffers_without_panicking() {
        let (handle, rx) = AudioHandle::new();
        let mut engine = Stage4Engine::new(48_000.0);
        let mut master = 0.85;
        let mut output = [0.0; 2];

        render_output_f32(&mut output, 0, &rx, &mut engine, &mut master);
        assert_eq!(output, [0.0; 2]);
        drop(handle);
    }
}
