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
        }
    }

    /// Handles a real-time MIDI event across the 4-zone split router, Morph matrix, and 7 layers.
    pub fn handle_midi(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { note, velocity, .. } => {
                for layer in LayerId::ALL {
                    if !self.scene.is_layer_active(layer) {
                        continue;
                    }
                    let split_g = self.split_router.note_gain(layer, note);
                    if split_g <= 0.0 {
                        continue;
                    }
                    let eff_vel = ((f32::from(velocity) * split_g).round() as u8).clamp(1, 127);
                    match layer {
                        LayerId::OrganA => self.organ_a.note_on(note),
                        LayerId::OrganB => self.organ_b.note_on(note),
                        LayerId::PianoA => self.piano_a.note_on(note, eff_vel),
                        LayerId::PianoB => self.piano_b.note_on(note, eff_vel),
                        LayerId::SynthA => self.synth_a.note_on(note, eff_vel),
                        LayerId::SynthB => self.synth_b.note_on(note, eff_vel),
                        LayerId::SynthC => self.synth_c.note_on(note, eff_vel),
                    }
                }
            }
            MidiEvent::NoteOff { note, .. } => {
                self.organ_a.note_off(note);
                self.organ_b.note_off(note);
                self.piano_a.note_off(note);
                self.piano_b.note_off(note);
                self.synth_a.note_off(note);
                self.synth_b.note_off(note);
                self.synth_c.note_off(note);
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
        self.scene.scene_1_mask = prog.global.scene_1_mask;
        self.scene.scene_2_mask = prog.global.scene_2_mask;
        for (i, &pt) in prog.global.split_points.iter().enumerate() {
            self.split_router.splits[i].midi_note = pt;
            self.split_router.splits[i].crossfade = prog.global.split_crossfades[i];
            self.split_router.splits[i].enabled = prog.global.split_enabled;
        }

        // Layer base levels
        self.layer_levels[0].base = prog.organ.layer_a_level;
        self.layer_levels[1].base = prog.organ.layer_b_level;
        self.layer_levels[2].base = prog.piano.layer_a.level;
        self.layer_levels[3].base = prog.piano.layer_b.level;
        self.layer_levels[4].base = prog.synth.layer_a.level;
        self.layer_levels[5].base = prog.synth.layer_b.level;
        self.layer_levels[6].base = prog.synth.layer_c.level;
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
        self.synth_a.cutoff_hz = cut_a;
        self.synth_b.cutoff_hz = cut_b;
        self.synth_c.cutoff_hz = cut_c;

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
    }
}
