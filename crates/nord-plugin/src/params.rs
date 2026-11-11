//! Host-automatable parameters for the Nord Stage 4 plugin.
//!
//! Exposes all 7 layers (Organ A/B, Piano A/B, Synth A/B/C), global controls,
//! morph controllers, split points, and master bus parameters with stable string IDs.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::morph::LayerId;
use nord_core::organ::{OrganModel, RotarySpeed};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::SynthOscMode;

/// MIDI input routing mode for the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum, Default)]
pub enum MidiRoutingMode {
    /// Standard hardware split/zone mode across all incoming channels (omni).
    #[default]
    OmniSplit,
    /// Ch 1: Organ A, Ch 2: Organ B, Ch 3: Piano A, Ch 4: Piano B,
    /// Ch 5: Synth A, Ch 6: Synth B, Ch 7: Synth C.
    MultitimbralPerLayer,
}

/// Host-automatable parameter container for `NordStage4Plugin`.
#[derive(Params)]
pub struct Stage4Params {
    /// MIDI Routing Mode: OmniSplit or MultitimbralPerLayer.
    #[id = "midi_mode"]
    pub midi_mode: EnumParam<MidiRoutingMode>,

    // Master Bus & Global Controls
    #[id = "master_level"]
    pub master_level: FloatParam,
    #[id = "master_bpm"]
    pub master_bpm: FloatParam,
    #[id = "master_compressor"]
    pub master_compressor: FloatParam,
    #[id = "layer_scene_ii"]
    pub layer_scene_ii: BoolParam,
    #[id = "split_enabled"]
    pub split_enabled: BoolParam,

    // Morph Controllers
    #[id = "mod_wheel"]
    pub mod_wheel: FloatParam,
    #[id = "control_pedal"]
    pub control_pedal: FloatParam,
    #[id = "aftertouch"]
    pub aftertouch: FloatParam,

    // Organ Section
    #[id = "organ_model"]
    pub organ_model: IntParam,
    #[id = "organ_a_level"]
    pub organ_a_level: FloatParam,
    #[id = "organ_b_level"]
    pub organ_b_level: FloatParam,
    #[id = "organ_a_enabled"]
    pub organ_a_enabled: BoolParam,
    #[id = "organ_b_enabled"]
    pub organ_b_enabled: BoolParam,
    #[id = "drawbar_1_16"]
    pub drawbar_1_16: IntParam,
    #[id = "drawbar_2_5_1_3"]
    pub drawbar_2_5_1_3: IntParam,
    #[id = "drawbar_3_8"]
    pub drawbar_3_8: IntParam,
    #[id = "drawbar_4_4"]
    pub drawbar_4_4: IntParam,
    #[id = "drawbar_5_2_2_3"]
    pub drawbar_5_2_2_3: IntParam,
    #[id = "drawbar_6_2"]
    pub drawbar_6_2: IntParam,
    #[id = "drawbar_7_1_3_5"]
    pub drawbar_7_1_3_5: IntParam,
    #[id = "drawbar_8_1_1_3"]
    pub drawbar_8_1_1_3: IntParam,
    #[id = "drawbar_9_1"]
    pub drawbar_9_1: IntParam,
    #[id = "organ_perc_enable"]
    pub organ_perc_enable: BoolParam,
    #[id = "organ_perc_soft"]
    pub organ_perc_soft: BoolParam,
    #[id = "organ_perc_fast"]
    pub organ_perc_fast: BoolParam,
    #[id = "organ_perc_third"]
    pub organ_perc_third: BoolParam,
    #[id = "rotary_speed_fast"]
    pub rotary_speed_fast: BoolParam,
    #[id = "rotary_drive"]
    pub rotary_drive: FloatParam,

    // Piano Section
    #[id = "piano_a_level"]
    pub piano_a_level: FloatParam,
    #[id = "piano_b_level"]
    pub piano_b_level: FloatParam,
    #[id = "piano_a_enabled"]
    pub piano_a_enabled: BoolParam,
    #[id = "piano_b_enabled"]
    pub piano_b_enabled: BoolParam,
    #[id = "piano_category"]
    pub piano_category: IntParam,
    #[id = "piano_timbre"]
    pub piano_timbre: IntParam,
    #[id = "piano_touch"]
    pub piano_touch: IntParam,
    #[id = "piano_string_res"]
    pub piano_string_res: BoolParam,
    #[id = "piano_pedal_noise"]
    pub piano_pedal_noise: BoolParam,

    // Synth Section
    #[id = "synth_a_level"]
    pub synth_a_level: FloatParam,
    #[id = "synth_b_level"]
    pub synth_b_level: FloatParam,
    #[id = "synth_c_level"]
    pub synth_c_level: FloatParam,
    #[id = "synth_a_enabled"]
    pub synth_a_enabled: BoolParam,
    #[id = "synth_b_enabled"]
    pub synth_b_enabled: BoolParam,
    #[id = "synth_c_enabled"]
    pub synth_c_enabled: BoolParam,
    #[id = "synth_osc_mode"]
    pub synth_osc_mode: IntParam,
    #[id = "synth_filter_cutoff"]
    pub synth_filter_cutoff: FloatParam,
    #[id = "synth_filter_resonance"]
    pub synth_filter_resonance: FloatParam,
    #[id = "synth_filter_drive"]
    pub synth_filter_drive: FloatParam,
    #[id = "synth_attack"]
    pub synth_attack: FloatParam,
    #[id = "synth_decay"]
    pub synth_decay: FloatParam,
    #[id = "synth_release"]
    pub synth_release: FloatParam,
}

impl Default for Stage4Params {
    fn default() -> Self {
        Self {
            midi_mode: EnumParam::new("MIDI Mode", MidiRoutingMode::OmniSplit),

            master_level: FloatParam::new(
                "Master Level",
                0.8,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            master_bpm: FloatParam::new(
                "Master BPM",
                120.0,
                FloatRange::Linear {
                    min: 30.0,
                    max: 300.0,
                },
            )
            .with_unit(" BPM")
            .with_step_size(0.1),
            master_compressor: FloatParam::new(
                "Master Compressor",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            layer_scene_ii: BoolParam::new("Layer Scene II", false),
            split_enabled: BoolParam::new("Split Enabled", false),

            mod_wheel: FloatParam::new("Mod Wheel", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            control_pedal: FloatParam::new(
                "Control Pedal",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            aftertouch: FloatParam::new(
                "Aftertouch",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),

            organ_model: IntParam::new("Organ Model", 0, IntRange::Linear { min: 0, max: 4 }),
            organ_a_level: FloatParam::new(
                "Organ A Level",
                0.8,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            organ_b_level: FloatParam::new(
                "Organ B Level",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            organ_a_enabled: BoolParam::new("Organ A Active", true),
            organ_b_enabled: BoolParam::new("Organ B Active", false),

            drawbar_1_16: IntParam::new("Drawbar 16'", 8, IntRange::Linear { min: 0, max: 8 }),
            drawbar_2_5_1_3: IntParam::new(
                "Drawbar 5 1/3'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_3_8: IntParam::new("Drawbar 8'", 8, IntRange::Linear { min: 0, max: 8 }),
            drawbar_4_4: IntParam::new("Drawbar 4'", 0, IntRange::Linear { min: 0, max: 8 }),
            drawbar_5_2_2_3: IntParam::new(
                "Drawbar 2 2/3'",
                0,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_6_2: IntParam::new("Drawbar 2'", 0, IntRange::Linear { min: 0, max: 8 }),
            drawbar_7_1_3_5: IntParam::new(
                "Drawbar 1 3/5'",
                0,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_8_1_1_3: IntParam::new(
                "Drawbar 1 1/3'",
                0,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_9_1: IntParam::new("Drawbar 1'", 0, IntRange::Linear { min: 0, max: 8 }),

            organ_perc_enable: BoolParam::new("Percussion Enable", false),
            organ_perc_soft: BoolParam::new("Percussion Soft", false),
            organ_perc_fast: BoolParam::new("Percussion Fast", true),
            organ_perc_third: BoolParam::new("Percussion 3rd", true),
            rotary_speed_fast: BoolParam::new("Rotary Fast", false),
            rotary_drive: FloatParam::new(
                "Rotary Drive",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            piano_a_level: FloatParam::new(
                "Piano A Level",
                0.85,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            piano_b_level: FloatParam::new(
                "Piano B Level",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            piano_a_enabled: BoolParam::new("Piano A Active", true),
            piano_b_enabled: BoolParam::new("Piano B Active", false),
            piano_category: IntParam::new("Piano Category", 0, IntRange::Linear { min: 0, max: 5 }),
            piano_timbre: IntParam::new("Piano Timbre", 1, IntRange::Linear { min: 0, max: 5 }),
            piano_touch: IntParam::new("Piano Touch", 0, IntRange::Linear { min: 0, max: 2 }),
            piano_string_res: BoolParam::new("String Resonance", true),
            piano_pedal_noise: BoolParam::new("Pedal Noise", true),

            synth_a_level: FloatParam::new(
                "Synth A Level",
                0.75,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            synth_b_level: FloatParam::new(
                "Synth B Level",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            synth_c_level: FloatParam::new(
                "Synth C Level",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            synth_a_enabled: BoolParam::new("Synth A Active", true),
            synth_b_enabled: BoolParam::new("Synth B Active", false),
            synth_c_enabled: BoolParam::new("Synth C Active", false),
            synth_osc_mode: IntParam::new("Synth Osc Mode", 0, IntRange::Linear { min: 0, max: 3 }),
            synth_filter_cutoff: FloatParam::new(
                "Synth Filter Cutoff",
                2500.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz"),
            synth_filter_resonance: FloatParam::new(
                "Synth Filter Res",
                0.2,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            synth_filter_drive: FloatParam::new(
                "Synth Filter Drive",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            synth_attack: FloatParam::new(
                "Synth Attack",
                0.01,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),
            synth_decay: FloatParam::new(
                "Synth Decay",
                0.8,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),
            synth_release: FloatParam::new(
                "Synth Release",
                0.3,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),
        }
    }
}

impl Stage4Params {
    fn set_mask_bit(mask: &mut u8, layer: LayerId, active: bool) {
        let bit = 1 << (layer as u8);
        if active {
            *mask |= bit;
        } else {
            *mask &= !bit;
        }
    }

    /// Synchronizes current parameter values into `Stage4Engine` without heap allocation.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        match self.midi_mode.value() {
            MidiRoutingMode::OmniSplit => {
                engine.global_midi_channel = Some(0);
                engine.layer_midi_channels = [None; 7];
            }
            MidiRoutingMode::MultitimbralPerLayer => {
                engine.global_midi_channel = None;
                engine.layer_midi_channels = [
                    Some(0),
                    Some(1),
                    Some(2),
                    Some(3),
                    Some(4),
                    Some(5),
                    Some(6),
                ];
            }
        }

        engine.master_clock.bpm = self.master_bpm.value();
        engine.master_bus.compressor_amount = self.master_compressor.value();
        engine.scene.scene_2_active = self.layer_scene_ii.value();

        let target_mask = if self.layer_scene_ii.value() {
            &mut engine.scene.scene_2_mask
        } else {
            &mut engine.scene.scene_1_mask
        };
        Self::set_mask_bit(target_mask, LayerId::OrganA, self.organ_a_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::OrganB, self.organ_b_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::PianoA, self.piano_a_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::PianoB, self.piano_b_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::SynthA, self.synth_a_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::SynthB, self.synth_b_enabled.value());
        Self::set_mask_bit(target_mask, LayerId::SynthC, self.synth_c_enabled.value());

        let split_on = self.split_enabled.value();
        for s in &mut engine.split_router.splits {
            s.enabled = split_on;
        }

        engine.morph_controllers.mod_wheel = self.mod_wheel.value();
        engine.morph_controllers.control_pedal = self.control_pedal.value();
        engine.morph_controllers.aftertouch = self.aftertouch.value();

        let master_gain = self.master_level.value();
        engine.layer_levels[0].base = self.organ_a_level.value() * master_gain;
        engine.layer_levels[1].base = self.organ_b_level.value() * master_gain;
        engine.layer_levels[2].base = self.piano_a_level.value() * master_gain;
        engine.layer_levels[3].base = self.piano_b_level.value() * master_gain;
        engine.layer_levels[4].base = self.synth_a_level.value() * master_gain;
        engine.layer_levels[5].base = self.synth_b_level.value() * master_gain;
        engine.layer_levels[6].base = self.synth_c_level.value() * master_gain;

        let organ_model = match self.organ_model.value() {
            1 => OrganModel::Vox,
            2 => OrganModel::Farfisa,
            3 => OrganModel::Pipe1,
            4 => OrganModel::Pipe2,
            _ => OrganModel::B3,
        };
        let drawbars = [
            self.drawbar_1_16.value() as u8,
            self.drawbar_2_5_1_3.value() as u8,
            self.drawbar_3_8.value() as u8,
            self.drawbar_4_4.value() as u8,
            self.drawbar_5_2_2_3.value() as u8,
            self.drawbar_6_2.value() as u8,
            self.drawbar_7_1_3_5.value() as u8,
            self.drawbar_8_1_1_3.value() as u8,
            self.drawbar_9_1.value() as u8,
        ];
        let spd = if self.rotary_speed_fast.value() {
            RotarySpeed::Fast
        } else {
            RotarySpeed::Slow
        };
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            organ.model = organ_model;
            organ.drawbars = drawbars;
            organ.percussion_enabled = self.organ_perc_enable.value();
            organ.percussion_soft = self.organ_perc_soft.value();
            organ.percussion_fast_decay = self.organ_perc_fast.value();
            organ.percussion_third_harmonic = self.organ_perc_third.value();
            organ.rotary_speed = spd;
            organ.rotary_drive = self.rotary_drive.value();
        }

        let cat = match self.piano_category.value() {
            1 => PianoCategory::Upright,
            2 => PianoCategory::Electric,
            3 => PianoCategory::Clav,
            4 => PianoCategory::Digital,
            5 => PianoCategory::Misc,
            _ => PianoCategory::Grand,
        };
        let timbre = match self.piano_timbre.value() {
            0 => PianoTimbre::Soft,
            2 => PianoTimbre::Bright,
            3 => PianoTimbre::Dyno1,
            4 => PianoTimbre::Dyno2,
            5 => PianoTimbre::Off,
            _ => PianoTimbre::Mid,
        };
        let touch = match self.piano_touch.value() {
            1 => VelocityTouch::Light,
            2 => VelocityTouch::Heavy,
            _ => VelocityTouch::Medium,
        };
        for piano in [&mut engine.piano_a, &mut engine.piano_b] {
            piano.category = cat;
            piano.timbre = timbre;
            piano.touch = touch;
            piano.string_resonance_enabled = self.piano_string_res.value();
            piano.pedal_noise_enabled = self.piano_pedal_noise.value();
        }

        let osc_mode = match self.synth_osc_mode.value() {
            1 => SynthOscMode::Wavetable,
            2 => SynthOscMode::Fm,
            3 => SynthOscMode::Sample,
            _ => SynthOscMode::Analog,
        };
        for synth in [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ] {
            synth.osc_mode = osc_mode;
            synth.cutoff_hz = self.synth_filter_cutoff.value();
            synth.resonance = self.synth_filter_resonance.value();
            synth.filter_drive = self.synth_filter_drive.value();
            synth.attack_sec = self.synth_attack.value();
            synth.decay_sec = self.synth_decay.value();
            synth.release_sec = self.synth_release.value();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage4_params_initialization() {
        let params = Stage4Params::default();
        assert_eq!(params.master_bpm.value(), 120.0);
        assert_eq!(params.organ_a_level.value(), 0.8);
        assert_eq!(params.drawbar_1_16.value(), 8);
        assert_eq!(params.midi_mode.value(), MidiRoutingMode::OmniSplit);
    }

    #[test]
    fn test_params_sync_to_engine() {
        let params = Stage4Params::default();
        let mut engine = Stage4Engine::new(44100.0);

        params.sync_to_engine(&mut engine);

        assert_eq!(engine.master_clock.bpm, 120.0);
        assert_eq!(engine.organ_a.drawbars[0], 8);
        assert_eq!(engine.organ_a.model, OrganModel::B3);
        assert_eq!(engine.piano_a.timbre, PianoTimbre::Mid);
        assert_eq!(engine.synth_a.osc_mode, SynthOscMode::Analog);
        assert_eq!(engine.synth_a.cutoff_hz, 2500.0);
        assert_eq!(engine.global_midi_channel, Some(0));
    }
}
