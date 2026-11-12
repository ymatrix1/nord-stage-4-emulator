//! Unified 3-Engine, 7-Layer Multitimbral Nord Stage 4 DSP Coordinator (`Stage4Engine`).
//!
//! Spec: `docs/ARCHITECTURE.md`, `docs/specs/effects-and-routing.md`, `docs/specs/morph-and-zones.md`

use crate::fx::{AmpCabinet, DelayFilterMode, LayerFxChain, MasterBus, MasterClock, ReverbColor};
use crate::midi::{cc, MidiEvent};
use crate::morph::{KeybedSplitRouter, LayerId, LayerSceneState, MorphControllers, MorphParameter};
use crate::organ::{OrganLayer, RotarySpeed};
use crate::piano::PianoLayer;
use crate::synth::SynthLayer;

/// Complete 7-layer Nord Stage 4 real-time DSP engine.
#[derive(Debug, Clone)]
pub struct Stage4Engine {
    pub sample_rate: f32,
    pub organ_a: OrganLayer,
    pub organ_b: OrganLayer,
    pub piano_a: PianoLayer,
    pub piano_b: PianoLayer,
    pub synth_a: SynthLayer,
    pub synth_b: SynthLayer,
    pub synth_c: SynthLayer,
    pub fx_chains: [LayerFxChain; 7],
    pub layer_levels: [MorphParameter; 7],
    pub synth_cutoff_morph: [MorphParameter; 3],
    pub morph_controllers: MorphControllers,
    pub scene: LayerSceneState,
    pub split_router: KeybedSplitRouter,
    pub master_bus: MasterBus,
    pub master_clock: MasterClock,
    /// MIDI channel (0..15) used for global keybed, controller, and program input.
    pub global_midi_channel: Option<u8>,
    /// Optional direct MIDI channel for each layer; these notes bypass keybed splits.
    pub layer_midi_channels: [Option<u8>; 7],
    note_hold_counts: [[[u8; 7]; 128]; 16],
}

impl Stage4Engine {
    /// Creates a pre-allocated 7-layer Stage 4 DSP engine at `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        Self {
            sample_rate: fs,
            organ_a: OrganLayer::new(fs),
            organ_b: OrganLayer::new(fs),
            piano_a: PianoLayer::new(fs),
            piano_b: PianoLayer::new(fs),
            synth_a: SynthLayer::new(fs),
            synth_b: SynthLayer::new(fs),
            synth_c: SynthLayer::new(fs),
            fx_chains: std::array::from_fn(|_| LayerFxChain::new(fs)),
            layer_levels: [MorphParameter::new(0.8); 7],
            synth_cutoff_morph: [MorphParameter::new(0.5); 3],
            morph_controllers: MorphControllers::default(),
            scene: LayerSceneState::default(),
            split_router: KeybedSplitRouter::default(),
            master_bus: MasterBus::default(),
            master_clock: MasterClock::default(),
            global_midi_channel: Some(0),
            layer_midi_channels: [None; 7],
            note_hold_counts: [[[0; 7]; 128]; 16],
        }
    }

    /// Handles a real-time MIDI event across the 4-zone split router, Morph matrix, and 7 layers.
    pub fn handle_midi(&mut self, event: MidiEvent) {
        let channel = match event {
            MidiEvent::NoteOn { channel, .. }
            | MidiEvent::NoteOff { channel, .. }
            | MidiEvent::ControlChange { channel, .. }
            | MidiEvent::ChannelAftertouch { channel, .. }
            | MidiEvent::ProgramChange { channel, .. } => channel,
        };
        if channel >= 16 {
            return;
        }
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                let channel_idx = channel as usize;
                let mut recipients = 0u8;
                let is_global_channel = self.global_midi_channel == Some(channel);
                for layer in LayerId::ALL {
                    if !self.scene.is_layer_active(layer) {
                        continue;
                    }
                    let is_layer_channel =
                        self.layer_midi_channels[layer as usize] == Some(channel);
                    if !is_global_channel && !is_layer_channel {
                        continue;
                    }
                    let split_gain = if is_global_channel && !is_layer_channel {
                        self.split_router.note_gain(layer, note)
                    } else {
                        1.0
                    };
                    if split_gain <= 0.0 {
                        continue;
                    }
                    recipients |= 1 << (layer as u8);
                    self.layer_note_on(layer, note, velocity, split_gain);
                }
                for layer in LayerId::ALL {
                    if recipients & (1 << (layer as u8)) != 0 {
                        let count =
                            &mut self.note_hold_counts[channel_idx][note as usize][layer as usize];
                        *count = count.saturating_add(1);
                    }
                }
            }
            MidiEvent::NoteOff { note, .. } => {
                let channel_idx = channel as usize;
                for layer in LayerId::ALL {
                    let count =
                        &mut self.note_hold_counts[channel_idx][note as usize][layer as usize];
                    if *count == 0 {
                        continue;
                    }
                    *count -= 1;
                    let held_on_any_channel = self
                        .note_hold_counts
                        .iter()
                        .any(|counts| counts[note as usize][layer as usize] > 0);
                    if !held_on_any_channel {
                        self.layer_note_off(layer, note);
                    }
                }
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                if self.global_midi_channel != Some(channel) {
                    return;
                }
                let norm = f32::from(value.min(127)) / 127.0;
                match controller {
                    cc::MOD_WHEEL => self.morph_controllers.mod_wheel = norm,
                    cc::CONTROL_PEDAL => self.morph_controllers.control_pedal = norm,
                    cc::DRAWBAR_1_16..=cc::DRAWBAR_9_1 => {
                        let idx = (controller - cc::DRAWBAR_1_16) as usize;
                        let step = ((norm * 8.0).round() as u8).min(8);
                        self.organ_a.drawbars[idx] = step;
                    }
                    cc::SUSTAIN_PEDAL => {
                        let down = value >= 64;
                        self.piano_a.set_damper_pedal(down, value);
                        self.piano_b.set_damper_pedal(down, value);
                    }
                    cc::SOSTENUTO_PEDAL => {
                        let down = value >= 64;
                        self.piano_a.set_sostenuto_pedal(down, value);
                        self.piano_b.set_sostenuto_pedal(down, value);
                    }
                    cc::SOFT_PEDAL => {
                        let down = value >= 64;
                        self.piano_a.set_soft_pedal(down, value);
                        self.piano_b.set_soft_pedal(down, value);
                    }
                    cc::SYNTH_FILTER_CUTOFF => {
                        for m in &mut self.synth_cutoff_morph {
                            m.base = norm;
                        }
                    }
                    cc::ROTARY_SPEED => {
                        let spd = if value >= 64 {
                            RotarySpeed::Fast
                        } else {
                            RotarySpeed::Slow
                        };
                        self.organ_a.rotary_speed = spd;
                        self.organ_b.rotary_speed = spd;
                    }
                    cc::LAYER_SCENE_II => {
                        self.scene.scene_2_active = value >= 64;
                    }
                    cc::MASTER_COMPRESSOR => {
                        self.master_bus.compressor_amount = norm;
                    }
                    _ => {}
                }
            }
            MidiEvent::ChannelAftertouch { pressure, .. } => {
                if self.global_midi_channel == Some(channel) {
                    self.morph_controllers.aftertouch = f32::from(pressure.min(127)) / 127.0;
                }
            }
            MidiEvent::ProgramChange { .. } => {}
        }
    }

    fn layer_note_on(&mut self, layer: LayerId, note: u8, velocity: u8, gain: f32) {
        match layer {
            LayerId::OrganA => self.organ_a.note_on_with_gain(note, gain),
            LayerId::OrganB => self.organ_b.note_on_with_gain(note, gain),
            LayerId::PianoA => self.piano_a.note_on_with_gain(note, velocity, gain),
            LayerId::PianoB => self.piano_b.note_on_with_gain(note, velocity, gain),
            LayerId::SynthA => self.synth_a.note_on_with_gain(note, velocity, gain),
            LayerId::SynthB => self.synth_b.note_on_with_gain(note, velocity, gain),
            LayerId::SynthC => self.synth_c.note_on_with_gain(note, velocity, gain),
        }
    }

    fn layer_note_off(&mut self, layer: LayerId, note: u8) {
        match layer {
            LayerId::OrganA => self.organ_a.note_off(note),
            LayerId::OrganB => self.organ_b.note_off(note),
            LayerId::PianoA => self.piano_a.note_off(note),
            LayerId::PianoB => self.piano_b.note_off(note),
            LayerId::SynthA => self.synth_a.note_off(note),
            LayerId::SynthB => self.synth_b.note_off(note),
            LayerId::SynthC => self.synth_c.note_off(note),
        }
    }

    #[cfg(test)]
    fn note_recipients_for_test(&self, channel: u8, note: u8) -> u8 {
        self.note_hold_counts[channel as usize][note as usize]
            .iter()
            .enumerate()
            .fold(0, |mask, (layer, count)| {
                if *count > 0 {
                    mask | (1 << layer)
                } else {
                    mask
                }
            })
    }

    /// Loads an `.npno` multi-velocity fixed-LPC piano bank into Piano Layer A (`slot_b = false`) or B (`slot_b = true`).
    pub fn load_piano_npno_bytes(
        &mut self,
        slot_b: bool,
        bytes: &[u8],
    ) -> Result<std::sync::Arc<crate::piano::NpnoSound>, String> {
        let sound = std::sync::Arc::new(crate::piano::NpnoSound::from_npno_bytes(bytes)?);
        if slot_b {
            self.piano_b.set_sound(Some(std::sync::Arc::clone(&sound)));
        } else {
            self.piano_a.set_sound(Some(std::sync::Arc::clone(&sound)));
        }
        Ok(sound)
    }

    /// Loads an `.nsmp4` multisample instrument into Synth Layer A (`0`), B (`1`), or C (`2`).
    pub fn load_synth_nsmp4_bytes(
        &mut self,
        layer_idx: usize,
        bytes: &[u8],
    ) -> Result<std::sync::Arc<crate::synth::SynthSampleSound>, crate::cbin::CbinError> {
        let sound = std::sync::Arc::new(crate::synth::SynthSampleSound::from_nsmp4_bytes(bytes)?);
        let layer = match layer_idx {
            0 => &mut self.synth_a,
            1 => &mut self.synth_b,
            2 => &mut self.synth_c,
            _ => {
                return Err(crate::cbin::CbinError::BitstreamError(format!(
                    "invalid synth layer index {layer_idx} (expected 0..=2)"
                )));
            }
        };
        layer.load_sample(Some(std::sync::Arc::clone(&sound)));
        Ok(sound)
    }

    /// Reads an `.ns4p` program entry from an `.ns4b` backup archive, applies its parameters,
    /// and auto-resolves/loads matching active `.npno` & `.nsmp4` banks via `ARM0` MurmurHash2 hashes.
    pub fn load_program_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        entry_query: &str,
    ) -> Result<crate::cbin::ProgramDecoded, crate::cbin::CbinError> {
        let (prog, _refs) =
            self.load_program_with_sounds_from_backup(archive, reader, entry_query)?;
        Ok(prog)
    }

    /// Reads an `.ns4p` program entry and auto-loads resolved Piano/Synth sample entries from `archive`.
    pub fn load_program_with_sounds_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        entry_query: &str,
    ) -> Result<(crate::cbin::ProgramDecoded, crate::cbin::ProgramSoundRefs), crate::cbin::CbinError>
    {
        let bytes = archive.read_entry(reader, entry_query)?;
        self.load_program_bytes_with_sounds_from_backup(archive, reader, &bytes)
    }

    /// Like [`Self::load_program_with_sounds_from_backup`] but takes the `.ns4p` bytes directly
    /// (e.g. an edited copy of a factory program) while still resolving sounds from the backup.
    pub fn load_program_bytes_with_sounds_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        bytes: &[u8],
    ) -> Result<(crate::cbin::ProgramDecoded, crate::cbin::ProgramSoundRefs), crate::cbin::CbinError>
    {
        let cbin = crate::cbin::CbinFile::parse(bytes)?;
        let prog = cbin.decode_program()?;
        self.apply_program(&prog);

        let refs = archive.resolve_program_sounds(&prog);
        let active_mask = prog.global.scene_1_mask | prog.global.scene_2_mask;
        let piano_slots = [
            (false, prog.piano.layer_a.enabled, &refs.piano_a, 2u8),
            (true, prog.piano.layer_b.enabled, &refs.piano_b, 3u8),
        ];
        for (slot_b, enabled, entry_opt, bit) in piano_slots {
            let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
            if needed {
                if let Some(entry_name) = entry_opt {
                    if let Ok(npno_bytes) = archive.read_entry(reader, entry_name) {
                        let _ = self.load_piano_npno_bytes(slot_b, &npno_bytes);
                    }
                }
            }
        }
        let synth_slots = [
            (0usize, prog.synth.layer_a.enabled, &refs.synth_a, 4u8),
            (1usize, prog.synth.layer_b.enabled, &refs.synth_b, 5u8),
            (2usize, prog.synth.layer_c.enabled, &refs.synth_c, 6u8),
        ];
        for (layer_idx, enabled, entry_opt, bit) in synth_slots {
            let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
            if needed {
                if let Some(entry_name) = entry_opt {
                    if let Ok(nsmp_bytes) = archive.read_entry(reader, entry_name) {
                        let _ = self.load_synth_nsmp4_bytes(layer_idx, &nsmp_bytes);
                    }
                }
            }
        }

        Ok((prog, refs))
    }

    /// Applies a decoded Nord Stage 4 Program to the DSP engine.
    /// Applies all decoded CBIN program settings (layers, scenes, splits, morphs, and 7 FX chains).
    pub fn apply_program(&mut self, prog: &crate::cbin::ProgramDecoded) {
        // Organ (Layers A & B + shared Organ FX)
        self.organ_a.model = prog.organ.model;
        self.organ_b.model = prog.organ.model_b;
        self.organ_a.drawbars = prog.organ.drawbars_a;
        self.organ_b.drawbars = prog.organ.drawbars_b;
        self.organ_a.octave_shift = prog.organ.layer_a_octave.clamp(-2, 2);
        self.organ_b.octave_shift = prog.organ.layer_b_octave.clamp(-2, 2);
        for (organ, vib_on) in [
            (&mut self.organ_a, prog.organ.vibrato_on),
            (&mut self.organ_b, prog.organ.vibrato_on_b),
        ] {
            organ.percussion_enabled = prog.organ.percussion_on;
            organ.percussion_soft = prog.organ.percussion_soft;
            organ.percussion_fast_decay = prog.organ.percussion_fast;
            organ.percussion_third_harmonic = prog.organ.percussion_third;
            organ.scanner_mode = prog.organ.scanner_mode(vib_on);
            organ.rotary_enabled = prog.organ.rotary_enabled;
            organ.rotary_speed = prog.organ.rotary_speed;
            organ.rotary_drive = prog.organ.rotary_drive;
        }

        // Piano (Layers A & B)
        for (piano, dec) in [
            (&mut self.piano_a, &prog.piano.layer_a),
            (&mut self.piano_b, &prog.piano.layer_b),
        ] {
            piano.category = dec.category;
            piano.timbre = dec.timbre;
            piano.touch = dec.touch;
            piano.string_resonance_enabled = dec.string_res;
            piano.pedal_noise_enabled = dec.pedal_noise;
            piano.soft_release = dec.soft_release;
            piano.unison = dec.unison.min(3);
            piano.dyn_comp = dec.dyn_comp.min(3);
            piano.octave_shift = dec.octave.clamp(-2, 2);
        }

        // Synth (Layers A, B & C + cutoff morph matrix)
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_a,
            &mut self.synth_cutoff_morph[0],
            &prog.synth.layer_a,
        );
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_b,
            &mut self.synth_cutoff_morph[1],
            &prog.synth.layer_b,
        );
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_c,
            &mut self.synth_cutoff_morph[2],
            &prog.synth.layer_c,
        );

        // Per-layer 0x36 FX chains
        Self::apply_fx_chain(&mut self.fx_chains[0], &prog.organ.fx);
        Self::apply_fx_chain(&mut self.fx_chains[1], &prog.organ.fx);
        Self::apply_fx_chain(&mut self.fx_chains[2], &prog.piano.fx_a);
        Self::apply_fx_chain(&mut self.fx_chains[3], &prog.piano.fx_b);
        Self::apply_fx_chain(&mut self.fx_chains[4], &prog.synth.fx_a);
        Self::apply_fx_chain(&mut self.fx_chains[5], &prog.synth.fx_b);
        Self::apply_fx_chain(&mut self.fx_chains[6], &prog.synth.fx_c);

        // Global
        self.master_clock.bpm = prog.global.tempo_bpm;
        self.master_bus.compressor_amount = (prog.global.master_compressor / 10.0).clamp(0.0, 1.0);
        for fx in &mut self.fx_chains {
            fx.snap_delay_time(self.master_clock.bpm);
        }

        // Scene / section masks (with fallback when synthetic tests leave scene_1_mask == 0)
        let category_mask = match prog.category_id {
            0x07 | 0x12 => 1 << 0,
            0x09 | 0x16 | 0x17 | 0x18 | 0x19 => 1 << 2,
            0x1E => (1 << 2) | (1 << 4),
            0x2A..=0x2F | 0x06 | 0x0B | 0x0D | 0x34 => 1 << 4,
            _ => (1 << 0) | (1 << 2) | (1 << 4),
        };
        let default_layer_mask = (prog.organ.layer_a_enabled as u8)
            | ((prog.organ.layer_b_enabled as u8) << 1)
            | ((prog.piano.layer_a.enabled as u8) << 2)
            | ((prog.piano.layer_b.enabled as u8) << 3)
            | ((prog.synth.layer_a.enabled as u8) << 4)
            | ((prog.synth.layer_b.enabled as u8) << 5)
            | ((prog.synth.layer_c.enabled as u8) << 6);

        self.scene.scene_1_mask = if prog.global.scene_1_mask != 0 {
            prog.global.scene_1_mask
        } else if default_layer_mask != 0 && default_layer_mask != (1 << 2) {
            default_layer_mask
        } else {
            category_mask
        };
        self.scene.scene_2_mask = if prog.global.scene_2_mask != 0 {
            prog.global.scene_2_mask
        } else {
            self.scene.scene_1_mask
        };

        let any_zone_flag = prog.global.split_zone_enabled.iter().any(|&e| e);
        for (i, &pt) in prog.global.split_points.iter().enumerate() {
            self.split_router.splits[i].midi_note = pt;
            self.split_router.splits[i].crossfade = prog.global.split_crossfades[i];
            self.split_router.splits[i].enabled =
                prog.global.split_enabled && (!any_zone_flag || prog.global.split_zone_enabled[i]);
        }

        // Layer levels + morph deltas
        let level_specs = [
            (prog.organ.layer_a_level, prog.organ.layer_a_level_morph),
            (prog.organ.layer_b_level, prog.organ.layer_b_level_morph),
            (prog.piano.layer_a.level, prog.piano.layer_a.level_morph),
            (prog.piano.layer_b.level, prog.piano.layer_b.level_morph),
            (prog.synth.layer_a.level, prog.synth.layer_a.level_morph),
            (prog.synth.layer_b.level, prog.synth.layer_b.level_morph),
            (prog.synth.layer_c.level, prog.synth.layer_c.level_morph),
        ];
        for (slot, (lvl, morph)) in self.layer_levels.iter_mut().zip(level_specs) {
            slot.base = lvl.clamp(0.0, 1.0);
            slot.delta_wheel = morph.wheel;
            slot.delta_aftertouch = morph.aftertouch;
            slot.delta_pedal = morph.ctrl_pedal;
        }
    }

    fn apply_fx_chain(fx: &mut LayerFxChain, dec: &crate::cbin::FxDecoded) {
        fx.mod1_type = dec.mod1_type();
        fx.mod1_rate_hz = 0.1 + dec.mod1_rate.clamp(0.0, 1.0) * 14.9;
        fx.mod1_amount = if dec.mod1_enabled {
            dec.mod1_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.mod2_type = dec.mod2_type();
        fx.mod2_rate_hz = 0.1 + dec.mod2_rate.clamp(0.0, 1.0) * 9.9;
        fx.mod2_amount = if dec.mod2_enabled {
            dec.mod2_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if dec.amp_enabled {
            let mode = dec.amp_mode();
            fx.amp_cabinet = match mode {
                1 | 2 => AmpCabinet::Twin,
                3 | 4 => AmpCabinet::Jc,
                5 | 6 => AmpCabinet::Small,
                7 => AmpCabinet::ToRotary,
                8 => AmpCabinet::LpFilter,
                9 => AmpCabinet::HpFilter,
                _ => AmpCabinet::Off,
            };
            fx.amp_variation = matches!(mode, 2 | 4 | 6);
            fx.eq_filter_res = dec.amp_filter_res();
            fx.drive_amount = (dec.amp_drive / 10.0).clamp(0.0, 1.0);
            fx.eq_bass_db = dec.eq_bass_db;
            fx.eq_mid_db = dec.eq_mid_db;
            fx.eq_mid_freq_hz = dec.eq_mid_freq_hz();
            fx.eq_treble_db = dec.eq_treble_db;
        } else {
            fx.amp_cabinet = AmpCabinet::Off;
            fx.drive_amount = 0.0;
            fx.eq_bass_db = 0.0;
            fx.eq_mid_db = 0.0;
            fx.eq_treble_db = 0.0;
        }
        fx.comp_enabled = dec.comp_enabled;
        fx.comp_amount = if dec.comp_enabled {
            dec.comp_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.comp_fast = dec.comp_fast;
        fx.delay_enabled = dec.delay_enabled;
        fx.delay_time_ms = dec.delay_time_ms();
        fx.delay_clock_sync = dec.delay_clock_sync;
        fx.delay_analog = dec.delay_mode != 0;
        fx.delay_ping_pong = dec.delay_ping_pong;
        fx.delay_filter = if !dec.delay_fb_filter {
            DelayFilterMode::Off
        } else {
            match dec.delay_fb_fx_type & 0x03 {
                0 => DelayFilterMode::Hp,
                1 => DelayFilterMode::Lp,
                _ => DelayFilterMode::Bp,
            }
        };
        fx.delay_variation = dec.delay_fb_fx_type >= 4;
        fx.delay_feedback = dec.delay_feedback.clamp(0.0, 0.95);
        fx.delay_mix = if dec.delay_enabled {
            dec.delay_mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.reverb_model = dec.reverb_model();
        fx.reverb_color = match dec.reverb_color {
            1 => ReverbColor::Bright,
            2 => ReverbColor::Dark,
            3 => ReverbColor::Chorale,
            _ => ReverbColor::Normal,
        };
        fx.reverb_mix = if dec.reverb_enabled {
            dec.reverb_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self::apply_fx_morph(fx, dec);
    }

    /// Loads `Wheel` / `A.T.` / `CtrlPed` morph bases and deltas; disabled slots stay un-morphed.
    fn apply_fx_morph(fx: &mut LayerFxChain, dec: &crate::cbin::FxDecoded) {
        fn set(
            slot: &mut crate::morph::MorphParameter,
            on: bool,
            base: f32,
            d: crate::cbin::MorphDeltas,
        ) {
            *slot = crate::morph::MorphParameter::new(base.clamp(0.0, 1.0));
            if on {
                slot.delta_wheel = d.wheel;
                slot.delta_aftertouch = d.aftertouch;
                slot.delta_pedal = d.ctrl_pedal;
            }
        }
        let m = &mut fx.morph;
        set(
            &mut m.mod1_rate,
            dec.mod1_enabled,
            dec.mod1_rate,
            dec.mod1_rate_morph,
        );
        set(
            &mut m.mod1_amount,
            dec.mod1_enabled,
            dec.mod1_amount,
            dec.mod1_amount_morph,
        );
        set(
            &mut m.mod2_rate,
            dec.mod2_enabled,
            dec.mod2_rate,
            dec.mod2_rate_morph,
        );
        set(
            &mut m.mod2_amount,
            dec.mod2_enabled,
            dec.mod2_amount,
            dec.mod2_amount_morph,
        );
        set(
            &mut m.eq_mid_freq,
            dec.amp_enabled,
            dec.eq_mid_freq_norm,
            dec.eq_mid_freq_morph,
        );
        set(
            &mut m.amp_drive,
            dec.amp_enabled,
            dec.amp_drive / 10.0,
            dec.amp_drive_morph,
        );
        set(
            &mut m.delay_mix,
            dec.delay_enabled,
            dec.delay_mix,
            dec.delay_mix_morph,
        );
        set(
            &mut m.delay_feedback,
            dec.delay_enabled,
            dec.delay_feedback,
            dec.delay_feedback_morph,
        );
        set(
            &mut m.reverb_amount,
            dec.reverb_enabled,
            dec.reverb_amount,
            dec.reverb_amount_morph,
        );
    }

    /// Renders a single stereo frame `[left, right]` across all 7 layers + 7 FX racks + Master Bus.
    pub fn render_frame(&mut self) -> [f32; 2] {
        self.master_clock.advance(self.sample_rate);

        // Apply Morphing Matrix to Synth filter cutoffs via OS v1.66 14 Hz .. 32 kHz curve
        let cut_a = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[0].evaluate(&self.morph_controllers),
        );
        let cut_b = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[1].evaluate(&self.morph_controllers),
        );
        let cut_c = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[2].evaluate(&self.morph_controllers),
        );
        if self.synth_cutoff_morph[0].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[0].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[0].delta_aftertouch.abs() > 1e-5
        {
            self.synth_a.cutoff_hz = cut_a;
        }
        if self.synth_cutoff_morph[1].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[1].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[1].delta_aftertouch.abs() > 1e-5
        {
            self.synth_b.cutoff_hz = cut_b;
        }
        if self.synth_cutoff_morph[2].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[2].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[2].delta_aftertouch.abs() > 1e-5
        {
            self.synth_c.cutoff_hz = cut_c;
        }

        let raw_layers = [
            self.organ_a.render_frame(),
            self.organ_b.render_frame(),
            self.piano_a.render_frame(),
            self.piano_b.render_frame(),
            self.synth_a.render_frame(),
            self.synth_b.render_frame(),
            self.synth_c.render_frame(),
        ];

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for (i, raw) in raw_layers.into_iter().enumerate() {
            self.fx_chains[i].apply_morph(&self.morph_controllers);
            let [fx_l, fx_r] = self.fx_chains[i].process_frame_with_bpm(raw, self.master_clock.bpm);
            let gain = self.layer_levels[i].evaluate(&self.morph_controllers);
            mix_l += fx_l * gain;
            mix_r += fx_r * gain;
        }

        self.master_bus.process_frame([mix_l, mix_r])
    }

    /// Processes an interleaved or dual-channel block in place with zero heap allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        for i in 0..frames {
            let [l, r] = self.render_frame();
            left[i] = l;
            right[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_7_layer_engine_render_morph_and_scene_ii() {
        let mut engine = Stage4Engine::new(48_000.0);
        // Assign ModWheel morph to boost Synth A filter cutoff
        engine.synth_cutoff_morph[0].delta_wheel = 0.45;

        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 110,
        });
        engine.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::MOD_WHEEL,
            value: 127,
        });

        let mut left = [0.0f32; 256];
        let mut right = [0.0f32; 256];
        engine.process_block(&mut left, &mut right);

        let energy: f32 = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| l * l + r * r)
            .sum();
        assert!(
            energy > 0.05,
            "7-layer engine must output non-zero stereo audio"
        );
        assert!(left.iter().all(|x| x.is_finite()));

        // Toggle Scene II via CC85
        engine.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::LAYER_SCENE_II,
            value: 127,
        });
        assert!(engine.scene.scene_2_active);
    }

    #[test]
    fn test_engine_apply_decoded_program() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut prog = crate::cbin::ProgramDecoded::default();
        prog.organ.model = crate::organ::OrganModel::Vox;
        prog.organ.drawbars_a = [8, 0, 4, 0, 2, 0, 1, 0, 0];
        prog.organ.percussion_soft = true;
        prog.global.tempo_bpm = 145.0;
        prog.global.master_compressor = 8.0;
        prog.piano.layer_a.timbre = crate::piano::PianoTimbre::Bright;
        prog.synth.layer_a.filter_cutoff_hz = 1500.0;

        engine.apply_program(&prog);

        assert_eq!(engine.organ_a.model, crate::organ::OrganModel::Vox);
        assert_eq!(engine.organ_a.drawbars, [8, 0, 4, 0, 2, 0, 1, 0, 0]);
        assert!(engine.organ_a.percussion_soft);
        assert_eq!(engine.master_clock.bpm, 145.0);
        assert_eq!(engine.piano_a.timbre, crate::piano::PianoTimbre::Bright);
        assert_eq!(engine.synth_a.cutoff_hz, 1500.0);

        // Synth category routing check (when scene_1_mask == 0 & Piano A fallback artifact occurs)
        let mut synth_prog = crate::cbin::ProgramDecoded {
            category_id: 0x2D, // Synth
            ..Default::default()
        };
        synth_prog.global.scene_1_mask = 0;
        synth_prog.organ.layer_a_enabled = false;
        synth_prog.synth.layer_a.enabled = false;
        synth_prog.synth.layer_a.waveform = crate::synth::VaWaveform::Pulse;
        engine.apply_program(&synth_prog);
        assert_ne!(engine.scene.scene_1_mask & (1 << 4), 0);
        assert_eq!(engine.synth_a.waveform, crate::synth::VaWaveform::Pulse);

        // Organ category routing check
        let mut organ_prog = crate::cbin::ProgramDecoded {
            category_id: 0x07, // Organ
            ..Default::default()
        };
        organ_prog.global.scene_1_mask = 0;
        organ_prog.organ.layer_a_enabled = false;
        organ_prog.synth.layer_a.enabled = false;
        organ_prog.organ.model = crate::organ::OrganModel::Farfisa;
        engine.apply_program(&organ_prog);
        assert_ne!(engine.scene.scene_1_mask & (1 << 0), 0);
        assert_eq!(engine.organ_a.model, crate::organ::OrganModel::Farfisa);
    }

    #[test]
    fn test_engine_apply_program_wires_fx_morph_and_preserves_highpass() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut prog = crate::cbin::ProgramDecoded::default();
        prog.organ.vibrato_on = true;
        prog.organ.vibrato_type = 5; // C3
        prog.organ.rotary_enabled = false;
        prog.organ.fx.mod2_enabled = true;
        prog.organ.fx.mod2_type_raw = 0; // Phaser
        prog.organ.fx.mod2_amount = 0.6;
        prog.organ.fx.amp_enabled = true;
        prog.organ.fx.eq_mid_db = -4.0;

        prog.synth.layer_a.filter_model = crate::synth::FilterModel::HighPass;
        prog.synth.layer_a.filter_cutoff_hz = 6500.0;
        prog.synth.layer_a.filter_cutoff_morph.wheel = 0.25;
        prog.synth.layer_a.level = 0.0;
        prog.synth.layer_a.level_morph.wheel = 0.85;
        prog.synth.fx_a.reverb_enabled = true;
        prog.synth.fx_a.reverb_type_raw = 4; // Hall
        prog.synth.fx_a.reverb_amount = 0.45;
        prog.synth.fx_a.delay_enabled = true;
        prog.synth.fx_a.delay_mix = 0.35;

        engine.apply_program(&prog);

        assert_eq!(engine.organ_a.scanner_mode, crate::organ::ScannerMode::C3);
        assert!(!engine.organ_a.rotary_enabled);
        assert_eq!(engine.fx_chains[0].mod2_type, crate::fx::Mod2Type::Phaser);
        assert!((engine.fx_chains[0].eq_mid_db + 4.0).abs() < 1e-4);

        assert_eq!(
            engine.synth_a.filter_model,
            crate::synth::FilterModel::HighPass
        );
        assert_eq!(engine.synth_a.cutoff_hz, 6500.0);
        assert!((engine.synth_cutoff_morph[0].delta_wheel - 0.25).abs() < 1e-4);
        assert_eq!(engine.layer_levels[4].base, 0.0);
        assert!((engine.layer_levels[4].delta_wheel - 0.85).abs() < 1e-4);
        assert_eq!(
            engine.fx_chains[4].reverb_model,
            crate::fx::ReverbModel::Hall
        );
        assert!((engine.fx_chains[4].reverb_mix - 0.45).abs() < 1e-4);
        assert!(engine.fx_chains[4].delay_enabled);
        assert!((engine.fx_chains[4].delay_mix - 0.35).abs() < 1e-4);
    }

    #[test]
    fn note_off_uses_note_on_recipients_even_after_scene_switch() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 91,
        });
        let recipients = engine.note_recipients_for_test(0, 60);
        assert_eq!(
            recipients,
            (1 << LayerId::OrganA as u8)
                | (1 << LayerId::PianoA as u8)
                | (1 << LayerId::SynthA as u8)
        );
        engine.scene.scene_2_active = true;
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 91,
        });
        assert_eq!(engine.note_recipients_for_test(0, 60), 0b0111_1111);
        engine.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(engine.note_recipients_for_test(0, 60), recipients);
        engine.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(engine.note_recipients_for_test(0, 60), 0);
    }

    #[test]
    fn split_gain_is_applied_without_reducing_midi_velocity() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.split_router.splits[0].enabled = true;
        engine.split_router.splits[0].midi_note = 60;
        engine.split_router.splits[0].crossfade = crate::morph::CrossfadeWidth::Small;
        engine.split_router.layer_zone_masks[LayerId::PianoA as usize] = 0b0001;
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 91,
        });
        let (velocity_response, split_gain, _) =
            engine.piano_a.voice_excitation_for_test(60).unwrap();
        assert_eq!(velocity_response, 91.0 / 127.0);
        assert_eq!(split_gain, 0.5);
    }

    #[test]
    fn unmapped_midi_channel_does_not_trigger_layers_or_global_controls() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 1,
            note: 60,
            velocity: 100,
        });
        engine.handle_midi(MidiEvent::ControlChange {
            channel: 1,
            controller: cc::MOD_WHEEL,
            value: 127,
        });
        assert_eq!(engine.note_recipients_for_test(1, 60), 0);
        assert_eq!(engine.morph_controllers.mod_wheel, 0.0);
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 16,
            note: 60,
            velocity: 100,
        });
        assert_eq!(engine.note_recipients_for_test(1, 60), 0);
    }

    #[test]
    fn layer_channel_routes_directly_and_note_off_uses_its_channel() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.layer_midi_channels[LayerId::PianoA as usize] = Some(4);
        engine.split_router.layer_zone_masks[LayerId::PianoA as usize] = 0;

        engine.handle_midi(MidiEvent::NoteOn {
            channel: 4,
            note: 60,
            velocity: 100,
        });
        let (velocity_response, direct_gain, released) =
            engine.piano_a.voice_excitation_for_test(60).unwrap();
        assert_eq!(velocity_response, 100.0 / 127.0);
        assert_eq!(direct_gain, 1.0);
        assert!(!released);
        assert_eq!(
            engine.note_recipients_for_test(4, 60),
            1 << LayerId::PianoA as u8
        );
        engine.handle_midi(MidiEvent::ControlChange {
            channel: 4,
            controller: cc::MOD_WHEEL,
            value: 127,
        });
        assert_eq!(engine.morph_controllers.mod_wheel, 0.0);

        engine.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(
            engine.note_recipients_for_test(4, 60),
            1 << LayerId::PianoA as u8
        );
        assert!(!engine.piano_a.voice_excitation_for_test(60).unwrap().2);

        engine.handle_midi(MidiEvent::NoteOff {
            channel: 4,
            note: 60,
            velocity: 0,
        });
        assert_eq!(engine.note_recipients_for_test(4, 60), 0);
        assert!(engine.piano_a.voice_excitation_for_test(60).unwrap().2);
    }

    #[test]
    fn global_channel_is_configurable_and_can_be_disabled() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.global_midi_channel = Some(7);
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 90,
        });
        assert_eq!(engine.note_recipients_for_test(0, 60), 0);

        engine.handle_midi(MidiEvent::NoteOn {
            channel: 7,
            note: 60,
            velocity: 90,
        });
        assert_ne!(engine.note_recipients_for_test(7, 60), 0);

        engine.global_midi_channel = None;
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 7,
            note: 62,
            velocity: 90,
        });
        assert_eq!(engine.note_recipients_for_test(7, 62), 0);
    }

    #[test]
    fn overlapping_channels_do_not_release_a_pitch_while_still_held() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.layer_midi_channels[LayerId::PianoA as usize] = Some(4);
        for channel in [0, 4] {
            engine.handle_midi(MidiEvent::NoteOn {
                channel,
                note: 60,
                velocity: 100,
            });
        }

        engine.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert!(engine
            .piano_a
            .voice_excitation_for_test(60)
            .is_some_and(|(_, _, released)| !released));

        engine.handle_midi(MidiEvent::NoteOff {
            channel: 4,
            note: 60,
            velocity: 0,
        });
        assert!(engine
            .piano_a
            .voice_excitation_for_test(60)
            .is_some_and(|(_, _, released)| released));
    }

    #[test]
    fn apply_program_routes_synth_layer_params_through_synth_params() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut prog = crate::cbin::ProgramDecoded::default();
        let l = &mut prog.synth.layer_b;
        l.unison = 2;
        l.legato = true;
        l.glide = 5.0;
        l.filter_env_dedicated = true;
        l.filter_env_decay_ms = 500.0;
        l.lfo_dest = crate::synth::LfoDestination::OscPitch;
        engine.apply_program(&prog);

        let s = &engine.synth_b;
        assert_eq!(s.unison, 2);
        assert!(s.mono && s.legato);
        assert!(s.glide_sec > 0.0);
        assert!(s.filter_env_dedicated);
        assert!((s.filter_env_decay_sec - 0.5).abs() < 1e-6);
        assert_eq!(s.lfo_dest, crate::synth::LfoDestination::OscPitch);
        // Untouched layers keep polyphonic defaults.
        assert!(!engine.synth_a.mono);
    }

    #[test]
    fn load_synth_nsmp4_and_program_from_backup_helpers() {
        let mut engine = Stage4Engine::new(48_000.0);

        // Invalid synth layer index rejected cleanly
        assert!(engine.load_synth_nsmp4_bytes(3, &[]).is_err());

        // Valid minimal .nsmp4 loaded into Synth Layer B (index 1)
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(b"CBIN");
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[8..12].copy_from_slice(b"nsmp");
        data[0x2c..0x30].copy_from_slice(b"NSMP");
        data.extend_from_slice(&[0u8; 12]);
        let mut hdr = vec![0u8; 64];
        hdr[10..15].copy_from_slice(b"Pad1\0");
        data.extend_from_slice(b"\0hdr");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(hdr.len() as u32).to_be_bytes());
        data.extend_from_slice(&hdr);

        let snd = engine
            .load_synth_nsmp4_bytes(1, &data)
            .expect("empty-stroke nsmp4 parses");
        assert_eq!(snd.name, "Pad1");
        assert_eq!(engine.synth_b.loaded_sample_name(), Some("Pad1"));

        // Auto-resolve .npno and .nsmp4 when loading .ns4p from .ns4b
        let piano_stem = "Astoria Grand Lrg 6.1";
        let sample_stem = "Ac Guitar Finger_ST Mart 4.1";
        let mut npno = vec![0u8; 48];
        npno[0..4].copy_from_slice(b"CBIN");
        npno[4..8].copy_from_slice(&1u32.to_le_bytes());
        npno[8..12].copy_from_slice(b"npno");
        let mut pno_hdr = vec![0u8; 0x90];
        pno_hdr[0x48..0x48 + 7].copy_from_slice(b"Astoria");
        pno_hdr[0x68..0x68 + 7].copy_from_slice(b"Astoria");
        npno.extend_from_slice(b"\0hdr");
        npno.extend_from_slice(&1u32.to_be_bytes());
        npno.extend_from_slice(&(pno_hdr.len() as u32).to_be_bytes());
        npno.extend_from_slice(&pno_hdr);

        let mut prog = crate::cbin::ProgramDecoded {
            name: "Split Resolve".to_string(),
            ..Default::default()
        };
        prog.global.scene_1_mask = (1 << 2) | (1 << 5); // Piano A + Synth B
        prog.piano.layer_a.enabled = true;
        prog.piano.layer_a.name_hash = crate::cbin::nord_name_hash(piano_stem);
        prog.synth.layer_b.enabled = true;
        prog.synth.layer_b.osc_mode = crate::synth::SynthOscMode::Sample;
        prog.synth.layer_b.sample_id = crate::cbin::nord_name_hash(sample_stem);
        let ns4p_bytes = prog.to_cbin_file(0, 0).expect("pack ns4p").to_bytes();

        let build_test_zip = |entries: &[(&str, &[u8])]| -> Vec<u8> {
            let mut archive = Vec::new();
            let mut cd = Vec::new();
            for &(name, payload) in entries {
                let lfh_off = archive.len() as u32;
                let n = name.as_bytes();
                let sz = payload.len() as u32;
                archive.extend_from_slice(b"PK\x03\x04");
                archive.extend_from_slice(&20u16.to_le_bytes());
                archive.extend_from_slice(&[0u8; 12]);
                archive.extend_from_slice(&sz.to_le_bytes());
                archive.extend_from_slice(&sz.to_le_bytes());
                archive.extend_from_slice(&(n.len() as u16).to_le_bytes());
                archive.extend_from_slice(&0u16.to_le_bytes());
                archive.extend_from_slice(n);
                archive.extend_from_slice(payload);

                cd.extend_from_slice(b"PK\x01\x02");
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&[0u8; 12]);
                cd.extend_from_slice(&sz.to_le_bytes());
                cd.extend_from_slice(&sz.to_le_bytes());
                cd.extend_from_slice(&(n.len() as u16).to_le_bytes());
                cd.extend_from_slice(&[0u8; 12]);
                cd.extend_from_slice(&lfh_off.to_le_bytes());
                cd.extend_from_slice(n);
            }
            let cd_off = archive.len() as u32;
            let cd_sz = cd.len() as u32;
            let cnt = entries.len() as u16;
            archive.extend_from_slice(&cd);
            archive.extend_from_slice(b"PK\x05\x06\0\0\0\0");
            archive.extend_from_slice(&cnt.to_le_bytes());
            archive.extend_from_slice(&cnt.to_le_bytes());
            archive.extend_from_slice(&cd_sz.to_le_bytes());
            archive.extend_from_slice(&cd_off.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive
        };
        let zip_bytes = build_test_zip(&[
            ("Program/Split Resolve.ns4p", &ns4p_bytes),
            ("Piano/Grand/Astoria Grand Lrg 6.1.npno", &npno),
            ("Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4", &data),
        ]);
        let archive = crate::cbin::Ns4BackupArchive::from_bytes(&zip_bytes).expect("parse backup");
        let mut cursor = std::io::Cursor::new(&zip_bytes);
        let mut eng2 = Stage4Engine::new(48_000.0);
        let (_dec, refs) = eng2
            .load_program_with_sounds_from_backup(
                &archive,
                &mut cursor,
                "Program/Split Resolve.ns4p",
            )
            .expect("load program with resolved sounds");
        assert_eq!(
            refs.piano_a.as_deref(),
            Some("Piano/Grand/Astoria Grand Lrg 6.1.npno")
        );
        assert_eq!(
            refs.synth_b.as_deref(),
            Some("Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4")
        );
        assert_eq!(eng2.synth_b.loaded_sample_name(), Some("Pad1"));
    }

    #[test]
    fn test_fx_morph_loaded_from_program_and_applied_per_frame() {
        let mut prog = crate::cbin::ProgramDecoded::default();
        prog.organ.fx.reverb_enabled = true;
        prog.organ.fx.reverb_amount = 0.2;
        prog.organ.fx.reverb_amount_morph = crate::cbin::MorphDeltas {
            wheel: 0.5,
            aftertouch: 0.0,
            ctrl_pedal: 0.0,
        };
        prog.organ.fx.delay_enabled = false;
        prog.organ.fx.delay_mix_morph.wheel = 1.0;
        let mut engine = Stage4Engine::new(48_000.0);
        engine.apply_program(&prog);
        engine.morph_controllers.mod_wheel = 1.0;
        engine.render_frame();
        assert!((engine.fx_chains[0].reverb_mix - 0.7).abs() < 1e-4);
        assert!(
            engine.fx_chains[0].delay_mix.abs() < 1e-6,
            "disabled slot must not be morphed"
        );
    }

    #[test]
    fn test_amp_mode_mapping_from_program() {
        use crate::fx::AmpCabinet;
        let cases: [(u16, AmpCabinet, bool); 10] = [
            (0, AmpCabinet::Off, false),
            (33, AmpCabinet::Twin, false),      // 0b00100001: mode 1
            (50, AmpCabinet::Twin, true),       // mode 2
            (67, AmpCabinet::Jc, false),        // mode 3
            (340, AmpCabinet::Jc, true),        // mode 4
            (101, AmpCabinet::Small, false),    // mode 5
            (118, AmpCabinet::Small, true),     // mode 6
            (135, AmpCabinet::ToRotary, false), // mode 7
            (168, AmpCabinet::LpFilter, false), // mode 8
            (201, AmpCabinet::HpFilter, false), // mode 9
        ];
        for (raw, cab, var) in cases {
            let mut prog = crate::cbin::ProgramDecoded::default();
            prog.organ.fx.amp_enabled = true;
            prog.organ.fx.amp_type_new = raw;
            let mut engine = Stage4Engine::new(48_000.0);
            engine.apply_program(&prog);
            assert_eq!(engine.fx_chains[0].amp_cabinet, cab, "raw {raw}");
            assert_eq!(engine.fx_chains[0].amp_variation, var, "raw {raw}");
        }
    }
}
