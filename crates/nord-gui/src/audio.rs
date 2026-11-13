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
    /// Sets every opt-in firmware path at once (A/B switching, see `firmware_panel`).
    SetFirmwareOptions(nord_core::render_opts::FirmwareOptions),
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

/// Converts a normalized float [-1.0, 1.0] to a signed 16-bit PCM integer [-32767, 32767].
#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Writes a stereo frame [out_l, out_r] into an F32 audio buffer of `channels` width.
///
/// If `channels >= 2`:
/// - `frame[0] = out_l`
/// - `frame[1] = out_r`
/// - All additional channels `frame[2..channels]` are explicitly zeroed.
///
/// If `channels == 1`:
/// - `frame[0] = (out_l + out_r) * 0.5`
pub fn render_frame_to_f32(channels: usize, out_l: f32, out_r: f32, frame: &mut [f32]) {
    if channels >= 2 {
        frame[0] = out_l;
        frame[1] = out_r;
        for ch in frame[2..channels].iter_mut() {
            *ch = 0.0;
        }
    } else if channels == 1 && !frame.is_empty() {
        frame[0] = (out_l + out_r) * 0.5;
    }
}

/// Writes a stereo frame [out_l, out_r] into an I16 audio buffer of `channels` width.
///
/// If `channels >= 2`:
/// - `frame[0] = f32_to_i16(out_l)`
/// - `frame[1] = f32_to_i16(out_r)`
/// - All additional channels `frame[2..channels]` are explicitly zeroed.
///
/// If `channels == 1`:
/// - `frame[0] = f32_to_i16((out_l + out_r) * 0.5)`
pub fn render_frame_to_i16(channels: usize, out_l: f32, out_r: f32, frame: &mut [i16]) {
    if channels >= 2 {
        frame[0] = f32_to_i16(out_l);
        frame[1] = f32_to_i16(out_r);
        for ch in frame[2..channels].iter_mut() {
            *ch = 0;
        }
    } else if channels == 1 && !frame.is_empty() {
        frame[0] = f32_to_i16((out_l + out_r) * 0.5);
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
        AudioCommand::SetFirmwareOptions(opts) => opts.apply_exact(engine),
        AudioCommand::LoadProgram(prog) => {
            engine.apply_program(&prog);
        }
        AudioCommand::LoadPianoSound { slot_b, sound } => {
            if slot_b {
                engine.piano_b.set_sound(sound);
            } else {
                engine.piano_a.set_sound(sound);
            }
        }
        AudioCommand::LoadSynthSample { layer_idx, sound } => {
            let layer = match layer_idx {
                0 => &mut engine.synth_a,
                1 => &mut engine.synth_b,
                2 => &mut engine.synth_c,
                _ => return,
            };
            layer.load_sample(sound);
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
    fn test_render_frame_to_f32_and_extra_channel_zeroing() {
        // Multi-channel (4 channels, e.g. surround / quad interface)
        let mut frame_quad = [1.0f32, 1.0, 1.0, 1.0];
        render_frame_to_f32(4, 0.75, -0.5, &mut frame_quad);
        assert_eq!(frame_quad[0], 0.75);
        assert_eq!(frame_quad[1], -0.5);
        assert_eq!(frame_quad[2], 0.0);
        assert_eq!(frame_quad[3], 0.0);

        // Mono channel
        let mut frame_mono = [0.0f32];
        render_frame_to_f32(1, 0.4, 0.6, &mut frame_mono);
        assert!((frame_mono[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_render_frame_to_i16_and_format_conversion() {
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(-1.0), -32767);
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.5), 32767); // Clamped
        assert_eq!(f32_to_i16(-2.0), -32767); // Clamped

        let mut frame_quad = [999i16; 4];
        render_frame_to_i16(4, 1.0, -1.0, &mut frame_quad);
        assert_eq!(frame_quad[0], 32767);
        assert_eq!(frame_quad[1], -32767);
        assert_eq!(frame_quad[2], 0);
        assert_eq!(frame_quad[3], 0);

        let mut frame_mono = [0i16];
        render_frame_to_i16(1, 0.5, 0.5, &mut frame_mono);
        assert_eq!(frame_mono[0], 16384);
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

    #[test]
    fn test_apply_audio_command_load_program() {
        let mut engine = Stage4Engine::new(48000.0);
        let mut vol = 0.8;
        let mut prog = nord_core::cbin::ProgramDecoded::default();
        prog.organ.layer_a_level = 0.42;
        prog.organ.drawbars_a = [8, 8, 8, 0, 0, 0, 0, 0, 0];

        apply_audio_command(
            AudioCommand::LoadProgram(Box::new(prog)),
            &mut engine,
            &mut vol,
        );

        assert_eq!(engine.organ_a.drawbars, [8, 8, 8, 0, 0, 0, 0, 0, 0]);
        assert!((engine.layer_levels[0].base - 0.42).abs() < 1e-5);
    }

    #[test]
    fn test_apply_audio_command_load_piano_and_synth_samples() {
        let mut engine = Stage4Engine::new(48000.0);
        let mut vol = 0.8;

        let piano_sound = Arc::new(nord_core::piano::NpnoSound::new_synthetic(
            "Test Grand",
            60,
            &[0.1, 0.2, 0.1, 0.0],
            48000.0,
        ));
        apply_audio_command(
            AudioCommand::LoadPianoSound {
                slot_b: false,
                sound: Some(Arc::clone(&piano_sound)),
            },
            &mut engine,
            &mut vol,
        );
        engine.piano_a.note_on(60, 100);
        let [l, r] = engine.piano_a.render_frame();
        assert!(l.is_finite() && r.is_finite());

        let synth_sound = Arc::new(nord_core::synth::SynthSampleSound {
            name: "Test Sample".to_string(),
            zones: vec![],
        });
        apply_audio_command(
            AudioCommand::LoadSynthSample {
                layer_idx: 0,
                sound: Some(Arc::clone(&synth_sound)),
            },
            &mut engine,
            &mut vol,
        );
        assert_eq!(engine.synth_a.loaded_sample_name(), Some("Test Sample"));

        apply_audio_command(
            AudioCommand::LoadSynthSample {
                layer_idx: 0,
                sound: None,
            },
            &mut engine,
            &mut vol,
        );
        assert_eq!(engine.synth_a.loaded_sample_name(), None);
    }
}
