//! Unified 3-Engine, 7-Layer Multitimbral Nord Stage 4 DSP Coordinator (`Stage4Engine`).
//!
//! Spec: `docs/ARCHITECTURE.md`, `docs/specs/effects-and-routing.md`, `docs/specs/morph-and-zones.md`

use crate::fx::{LayerFxChain, MasterBus, MasterClock};
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
    note_recipients: [u8; 128],
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
            note_recipients: [0; 128],
        }
    }

    /// Handles a real-time MIDI event across the 4-zone split router, Morph matrix, and 7 layers.
    pub fn handle_midi(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                let mut recipients = 0u8;
                for layer in LayerId::ALL {
                    if !self.scene.is_layer_active(layer) {
                        continue;
                    }
                    let split_g = self.split_router.note_gain(layer, note);
                    if split_g <= 0.0 {
                        continue;
                    }
                    recipients |= 1 << (layer as u8);
                    match layer {
                        LayerId::OrganA => self.organ_a.note_on_with_gain(note, split_g),
                        LayerId::OrganB => self.organ_b.note_on_with_gain(note, split_g),
                        LayerId::PianoA => self.piano_a.note_on_with_gain(note, velocity, split_g),
                        LayerId::PianoB => self.piano_b.note_on_with_gain(note, velocity, split_g),
                        LayerId::SynthA => self.synth_a.note_on_with_gain(note, velocity, split_g),
                        LayerId::SynthB => self.synth_b.note_on_with_gain(note, velocity, split_g),
                        LayerId::SynthC => self.synth_c.note_on_with_gain(note, velocity, split_g),
                    }
                }
                self.note_recipients[note as usize] |= recipients;
            }
            MidiEvent::NoteOff { note, .. } => {
                let recipients = std::mem::take(&mut self.note_recipients[note as usize]);
                for layer in LayerId::ALL {
                    if recipients & (1 << (layer as u8)) == 0 {
                        continue;
                    }
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
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
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
                self.morph_controllers.aftertouch = f32::from(pressure.min(127)) / 127.0;
            }
            MidiEvent::ProgramChange { .. } => {}
        }
    }

    /// Applies a decoded Nord Stage 4 Program to the DSP engine.
    pub fn apply_program(&mut self, prog: &crate::cbin::ProgramDecoded) {
        // Organ
        self.organ_a.model = prog.organ.model;
        self.organ_b.model = prog.organ.model;
        self.organ_a.drawbars = prog.organ.drawbars_a;
        self.organ_b.drawbars = prog.organ.drawbars_b;
        self.organ_a.percussion_enabled = prog.organ.percussion_on;
        self.organ_a.percussion_soft = prog.organ.percussion_soft;
        self.organ_a.percussion_fast_decay = prog.organ.percussion_fast;
        self.organ_a.percussion_third_harmonic = prog.organ.percussion_third;
        self.organ_a.rotary_speed = prog.organ.rotary_speed;
        self.organ_a.rotary_drive = prog.organ.rotary_drive;

        // Piano
        self.piano_a.category = prog.piano.layer_a.category;
        self.piano_b.category = prog.piano.layer_b.category;
        self.piano_a.timbre = prog.piano.layer_a.timbre;
        self.piano_b.timbre = prog.piano.layer_b.timbre;
        self.piano_a.touch = prog.piano.layer_a.touch;
        self.piano_b.touch = prog.piano.layer_b.touch;
        self.piano_a.string_resonance_enabled = prog.piano.layer_a.string_res;
        self.piano_b.string_resonance_enabled = prog.piano.layer_b.string_res;
        self.piano_a.pedal_noise_enabled = prog.piano.layer_a.pedal_noise;
        self.piano_b.pedal_noise_enabled = prog.piano.layer_b.pedal_noise;

        // Synth
        self.synth_a.osc_mode = prog.synth.layer_a.osc_mode;
        self.synth_a.waveform = prog.synth.layer_a.waveform;
        self.synth_a.filter_model = prog.synth.layer_a.filter_model;
        self.synth_a.cutoff_hz = prog.synth.layer_a.filter_cutoff_hz;
        self.synth_a.resonance = prog.synth.layer_a.filter_resonance;
        self.synth_a.filter_drive = prog.synth.layer_a.filter_drive;
        self.synth_a.attack_sec = prog.synth.layer_a.env_attack_ms / 1000.0;
        self.synth_a.decay_sec = prog.synth.layer_a.env_decay_ms / 1000.0;
        self.synth_a.release_sec = prog.synth.layer_a.env_release_ms / 1000.0;

        self.synth_b.osc_mode = prog.synth.layer_b.osc_mode;
        self.synth_b.waveform = prog.synth.layer_b.waveform;
        self.synth_b.filter_model = prog.synth.layer_b.filter_model;
        self.synth_b.cutoff_hz = prog.synth.layer_b.filter_cutoff_hz;
        self.synth_b.resonance = prog.synth.layer_b.filter_resonance;
        self.synth_b.filter_drive = prog.synth.layer_b.filter_drive;
        self.synth_b.attack_sec = prog.synth.layer_b.env_attack_ms / 1000.0;
        self.synth_b.decay_sec = prog.synth.layer_b.env_decay_ms / 1000.0;
        self.synth_b.release_sec = prog.synth.layer_b.env_release_ms / 1000.0;

        self.synth_c.osc_mode = prog.synth.layer_c.osc_mode;
        self.synth_c.waveform = prog.synth.layer_c.waveform;
        self.synth_c.filter_model = prog.synth.layer_c.filter_model;
        self.synth_c.cutoff_hz = prog.synth.layer_c.filter_cutoff_hz;
        self.synth_c.resonance = prog.synth.layer_c.filter_resonance;
        self.synth_c.filter_drive = prog.synth.layer_c.filter_drive;
        self.synth_c.attack_sec = prog.synth.layer_c.env_attack_ms / 1000.0;
        self.synth_c.decay_sec = prog.synth.layer_c.env_decay_ms / 1000.0;
        self.synth_c.release_sec = prog.synth.layer_c.env_release_ms / 1000.0;

        // Global
        self.master_clock.bpm = prog.global.tempo_bpm;
        self.master_bus.compressor_amount = (prog.global.master_compressor / 10.0).clamp(0.0, 1.0);

        // If the patch bitstream has scene_1_mask == 0, activate layers based on category and enabled flags
        let category_mask = match prog.category_id {
            0x07 | 0x12 => 1 << 0,                             // Organ A
            0x09 | 0x16 | 0x17 | 0x18 | 0x19 => 1 << 2,        // Piano A
            0x1E => (1 << 2) | (1 << 4),                       // Piano A + Synth A
            0x2A..=0x2F | 0x06 | 0x0B | 0x0D | 0x34 => 1 << 4, // Synth A
            _ => (1 << 0) | (1 << 2) | (1 << 4),               // Default: All primary layers active
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

        for (i, &pt) in prog.global.split_points.iter().enumerate() {
            self.split_router.splits[i].midi_note = pt;
            self.split_router.splits[i].crossfade = prog.global.split_crossfades[i];
            self.split_router.splits[i].enabled = prog.global.split_enabled;
        }

        // Layer base levels (ensure active layers are audible)
        self.layer_levels[0].base =
            if (self.scene.scene_1_mask & (1 << 0)) != 0 && prog.organ.layer_a_level < 0.1 {
                0.85
            } else {
                prog.organ.layer_a_level
            };
        self.layer_levels[1].base = prog.organ.layer_b_level;
        self.layer_levels[2].base =
            if (self.scene.scene_1_mask & (1 << 2)) != 0 && prog.piano.layer_a.level < 0.1 {
                0.85
            } else {
                prog.piano.layer_a.level
            };
        self.layer_levels[3].base = prog.piano.layer_b.level;
        self.layer_levels[4].base =
            if (self.scene.scene_1_mask & (1 << 4)) != 0 && prog.synth.layer_a.level < 0.1 {
                0.85
            } else {
                prog.synth.layer_a.level
            };
        self.layer_levels[5].base = prog.synth.layer_b.level;
        self.layer_levels[6].base = prog.synth.layer_c.level;

        // Ensure Organ drawbars are active if Organ A is enabled
        if (self.scene.scene_1_mask & (1 << 0)) != 0
            && self.organ_a.drawbars.iter().all(|&d| d == 0)
        {
            self.organ_a.drawbars = [8, 8, 8, 0, 0, 0, 0, 0, 0];
        }

        // Guard against bitstream header bytes decoding as HighPass > 5 kHz (which mutes audible notes)
        if self.synth_a.filter_model == crate::synth::FilterModel::HighPass
            && self.synth_a.cutoff_hz > 5000.0
        {
            self.synth_a.filter_model = crate::synth::FilterModel::Lp24;
            self.synth_a.cutoff_hz = match prog.category_id {
                0x2C => 750.0,  // Synth Bass
                0x2E => 1900.0, // Synth Pad
                _ => 4200.0,    // Lead / Poly / Arp
            };
            self.synth_a.resonance = 0.25;
        }
    }

    /// Renders a single stereo frame `[left, right]` across all 7 layers + 7 FX racks + Master Bus.
    pub fn render_frame(&mut self) -> [f32; 2] {
        self.master_clock.advance(self.sample_rate);

        // Apply Morphing Matrix to Synth filter cutoffs (80 Hz .. 16 kHz exponential mapping)
        let cut_a =
            80.0 * 200.0f32.powf(self.synth_cutoff_morph[0].evaluate(&self.morph_controllers));
        let cut_b =
            80.0 * 200.0f32.powf(self.synth_cutoff_morph[1].evaluate(&self.morph_controllers));
        let cut_c =
            80.0 * 200.0f32.powf(self.synth_cutoff_morph[2].evaluate(&self.morph_controllers));
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
            let [fx_l, fx_r] = self.fx_chains[i].process_frame(raw);
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
    fn note_off_uses_note_on_recipients_even_after_scene_switch() {
        let mut engine = Stage4Engine::new(48_000.0);
        engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 91,
        });
        let recipients = engine.note_recipients[60];
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
        assert_eq!(engine.note_recipients[60], 0b0111_1111);
        engine.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(engine.note_recipients[60], 0);
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
        let (velocity_response, split_gain) = engine.piano_a.voice_excitation_for_test(60).unwrap();
        assert_eq!(velocity_response, 91.0 / 127.0);
        assert_eq!(split_gain, 0.5);
    }
}
