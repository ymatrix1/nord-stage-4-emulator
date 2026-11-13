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

/// MIDI input mode for the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum, Default)]
pub enum MidiRoutingMode {
    /// Standard hardware split/zone mode across all incoming channels (omni).
    #[default]
    OmniSplit,
    /// Channel 1: Organ A, Channel 2: Organ B, Channel 3: Piano A, Channel 4: Piano B,
    /// Channel 5: Synth A, Channel 6: Synth B, Channel 7: Synth C.
    MultitimbralPerLayer,
}

/// Host-automatable parameter container for NordStage4Plugin.
#[derive(Params)]
pub struct Stage4Params {
    /// MIDI Routing Mode: OmniSplit or MultitimbralPerLayer.
    #[id = "midi_mode"]
    pub midi_mode: EnumParam<MidiRoutingMode>,

    // ==========================================
    // Master Bus & Global Controls
    // ==========================================
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
    #[id = "split_1_note"]
    pub split_1_note: IntParam,
    #[id = "split_2_note"]
    pub split_2_note: IntParam,
    #[id = "split_3_note"]
    pub split_3_note: IntParam,
    #[id = "split_1_crossfade"]
    pub split_1_crossfade: IntParam,
    #[id = "split_2_crossfade"]
    pub split_2_crossfade: IntParam,
    #[id = "split_3_crossfade"]
    pub split_3_crossfade: IntParam,

    // ==========================================
    // Morph Controllers
    // ==========================================
    #[id = "mod_wheel"]
    pub mod_wheel: FloatParam,

    #[id = "control_pedal"]
    pub control_pedal: FloatParam,

    #[id = "aftertouch"]
    pub aftertouch: FloatParam,

    /// Morph assignment depths (Wheel / Pedal / Aftertouch) per layer level and synth cutoff.
    #[nested(id_prefix = "morph", group = "Morph Assign")]
    pub morph_assign: crate::morph_assign::MorphAssignParams,

    /// Per-layer FX controls (Amp/EQ, Comp, Delay, Reverb mix).
    #[nested(id_prefix = "fx", group = "Layer FX")]
    pub layer_fx: crate::fx_params::LayerFxBank,

    /// Per-layer FX selectors (mod types/rates, cabinet, delay switches, reverb model/colour).
    #[nested(id_prefix = "fxsel", group = "Layer FX Selectors")]
    pub layer_fx_selectors: crate::fx_selectors::LayerFxSelectorBank,

    /// Per-layer Piano Soft Release, Unison, Dyn Comp and Octave Shift.
    #[nested(id_prefix = "pianox", group = "Piano Layers")]
    pub piano_layers: crate::piano_params::PianoLayerBank,

    /// Per-layer Synth voice, kb track and envelope amounts.
    #[nested(id_prefix = "synthx", group = "Synth Layers")]
    pub synth_layers: crate::synth_params::SynthLayerBank,

    /// Per-program Transpose and Program Levels.
    #[nested(id_prefix = "prog", group = "Program")]
    pub program: crate::program_params::ProgramParams,

    /// Per-layer Synth LFO, Arpeggiator and Vibrato.
    #[nested(id_prefix = "synthm", group = "Synth Mod")]
    pub synth_mod: crate::synth_mod_params::SynthModBank,

    /// Per-layer Synth Gate, Pattern, KB Hold / Sync and Master Clock sync.
    #[nested(id_prefix = "synthg", group = "Synth Gate")]
    pub synth_gate: crate::synth_gate_params::SynthGateBank,

    /// Per-layer Organ B3 Bass, octave and vibrato/chorus plus Rotary switches.
    #[nested(id_prefix = "organx", group = "Organ Layers")]
    pub organ_layers: crate::organ_params::OrganLayerBank,

    /// Per-layer Organ model override, key click, rotary on/angle, fine tune, crosstalk, detune.
    #[nested(id_prefix = "organy", group = "Organ Layers Extra")]
    pub organ_extra: crate::organ_extra_params::OrganExtraBank,

    /// Per-layer Piano String Resonance and Pedal Noise switches.
    #[nested(id_prefix = "pianoy", group = "Piano Layers Extra")]
    pub piano_extra: crate::piano_extra_params::PianoExtraBank,

    /// Per-layer FX extras: mod amounts, EQ mid freq, filter res, variations, delay analog.
    #[nested(id_prefix = "fxx", group = "Layer FX Extra")]
    pub layer_fx_extra: crate::fx_extra_params::LayerFxExtraBank,

    /// Per-layer FX variations, Wah PED, delay division, GLOBAL / GROUP sharing.
    #[nested(id_prefix = "fxy", group = "Layer FX Sharing")]
    pub layer_fx_sharing: crate::fx_sharing_params::LayerFxSharingBank,

    // ==========================================
    // Organ Section
    // ==========================================
    #[id = "organ_model"]
    pub organ_model: IntParam, // 0: B3, 1: Vox, 2: Farfisa, 3: Pipe1, 4: Pipe2

    #[id = "organ_a_level"]
    pub organ_a_level: FloatParam,

    #[id = "organ_b_level"]
    pub organ_b_level: FloatParam,

    #[id = "organ_a_enabled"]
    pub organ_a_enabled: BoolParam,

    #[id = "organ_b_enabled"]
    pub organ_b_enabled: BoolParam,

    // Drawbars for Organ A (16' to 1')
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

    // Drawbars for Organ B (16' to 1')
    #[id = "drawbar_b_1_16"]
    pub drawbar_b_1_16: IntParam,
    #[id = "drawbar_b_2_5_1_3"]
    pub drawbar_b_2_5_1_3: IntParam,
    #[id = "drawbar_b_3_8"]
    pub drawbar_b_3_8: IntParam,
    #[id = "drawbar_b_4_4"]
    pub drawbar_b_4_4: IntParam,
    #[id = "drawbar_b_5_2_2_3"]
    pub drawbar_b_5_2_2_3: IntParam,
    #[id = "drawbar_b_6_2"]
    pub drawbar_b_6_2: IntParam,
    #[id = "drawbar_b_7_1_3_5"]
    pub drawbar_b_7_1_3_5: IntParam,
    #[id = "drawbar_b_8_1_1_3"]
    pub drawbar_b_8_1_1_3: IntParam,
    #[id = "drawbar_b_9_1"]
    pub drawbar_b_9_1: IntParam,

    // Organ Percussion & Rotary
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

    // ==========================================
    // Piano Section
    // ==========================================
    #[id = "piano_a_level"]
    pub piano_a_level: FloatParam,

    #[id = "piano_b_level"]
    pub piano_b_level: FloatParam,

    #[id = "piano_a_enabled"]
    pub piano_a_enabled: BoolParam,

    #[id = "piano_b_enabled"]
    pub piano_b_enabled: BoolParam,

    #[id = "piano_category"]
    pub piano_category: IntParam, // Grand, Upright, Electric, Clav, Digital, Misc

    #[id = "piano_timbre"]
    pub piano_timbre: IntParam, // Soft, Mid, Bright, Dyno1, Dyno2

    #[id = "piano_touch"]
    pub piano_touch: IntParam, // Med, Light, Heavy

    #[id = "piano_string_res"]
    pub piano_string_res: BoolParam,

    #[id = "piano_pedal_noise"]
    pub piano_pedal_noise: BoolParam,

    // ==========================================
    // Synth Section
    // ==========================================
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
    pub synth_osc_mode: IntParam, // Analog, Wavetable, FM, Samples

    #[id = "synth_filter_cutoff"]
    pub synth_filter_cutoff: FloatParam, // 20.0 Hz to 20,000.0 Hz

    #[id = "synth_filter_resonance"]
    pub synth_filter_resonance: FloatParam, // 0.0 to 1.0

    #[id = "synth_filter_drive"]
    pub synth_filter_drive: FloatParam, // 0.0 to 1.0

    #[id = "synth_attack"]
    pub synth_attack: FloatParam, // 0.001 to 10.0 sec

    #[id = "synth_decay"]
    pub synth_decay: FloatParam, // 0.001 to 10.0 sec

    #[id = "synth_release"]
    pub synth_release: FloatParam, // 0.001 to 10.0 sec
}

impl Default for Stage4Params {
    fn default() -> Self {
        Self {
            midi_mode: EnumParam::new("MIDI Mode", MidiRoutingMode::OmniSplit),

            // Master Bus
            master_level: FloatParam::new(
                "Master Level",
                0.8,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" dB")
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
            split_1_note: IntParam::new("Split 1 Note", 53, IntRange::Linear { min: 0, max: 127 }),
            split_2_note: IntParam::new("Split 2 Note", 65, IntRange::Linear { min: 0, max: 127 }),
            split_3_note: IntParam::new("Split 3 Note", 77, IntRange::Linear { min: 0, max: 127 }),
            split_1_crossfade: IntParam::new(
                "Split 1 Crossfade",
                0,
                IntRange::Linear { min: 0, max: 2 },
            ),
            split_2_crossfade: IntParam::new(
                "Split 2 Crossfade",
                0,
                IntRange::Linear { min: 0, max: 2 },
            ),
            split_3_crossfade: IntParam::new(
                "Split 3 Crossfade",
                0,
                IntRange::Linear { min: 0, max: 2 },
            ),

            // Morph
            mod_wheel: FloatParam::new("Mod Wheel", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            control_pedal: FloatParam::new(
                "Control Pedal",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            morph_assign: crate::morph_assign::MorphAssignParams::default(),
            layer_fx: crate::fx_params::LayerFxBank::default(),
            layer_fx_selectors: crate::fx_selectors::LayerFxSelectorBank::default(),
            piano_layers: crate::piano_params::PianoLayerBank::default(),
            synth_layers: crate::synth_params::SynthLayerBank::default(),
            program: crate::program_params::ProgramParams::default(),
            synth_mod: crate::synth_mod_params::SynthModBank::default(),
            synth_gate: crate::synth_gate_params::SynthGateBank::default(),
            organ_layers: crate::organ_params::OrganLayerBank::default(),
            organ_extra: crate::organ_extra_params::OrganExtraBank::default(),
            piano_extra: crate::piano_extra_params::PianoExtraBank::default(),
            layer_fx_extra: crate::fx_extra_params::LayerFxExtraBank::default(),
            layer_fx_sharing: crate::fx_sharing_params::LayerFxSharingBank::default(),
            aftertouch: FloatParam::new(
                "Aftertouch",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),

            // Organ
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

            drawbar_b_1_16: IntParam::new(
                "Organ B Drawbar 16'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_2_5_1_3: IntParam::new(
                "Organ B Drawbar 5 1/3'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_3_8: IntParam::new(
                "Organ B Drawbar 8'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_4_4: IntParam::new(
                "Organ B Drawbar 4'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_5_2_2_3: IntParam::new(
                "Organ B Drawbar 2 2/3'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_6_2: IntParam::new(
                "Organ B Drawbar 2'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_7_1_3_5: IntParam::new(
                "Organ B Drawbar 1 3/5'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_8_1_1_3: IntParam::new(
                "Organ B Drawbar 1 1/3'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),
            drawbar_b_9_1: IntParam::new(
                "Organ B Drawbar 1'",
                8,
                IntRange::Linear { min: 0, max: 8 },
            ),

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

            // Piano
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
            piano_timbre: IntParam::new("Piano Timbre", 1, IntRange::Linear { min: 0, max: 4 }),
            piano_touch: IntParam::new("Piano Touch", 0, IntRange::Linear { min: 0, max: 2 }),
            piano_string_res: BoolParam::new("String Resonance", true),
            piano_pedal_noise: BoolParam::new("Pedal Noise", true),

            // Synth
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

    /// Synchronizes current NIH-plug parameter values to the `Stage4Engine`.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        // Master Bus & Clock
        engine.master_clock.bpm = self.master_bpm.value();
        engine.master_bus.compressor_amount = self.master_compressor.value();
        engine.scene.scene_2_active = self.layer_scene_ii.value();

        // Layer mask sync
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

        // Split enabled
        let split_on = self.split_enabled.value();
        let split_notes = [
            self.split_1_note.value() as u8,
            self.split_2_note.value() as u8,
            self.split_3_note.value() as u8,
        ];
        let split_crossfades = [
            self.split_1_crossfade.value(),
            self.split_2_crossfade.value(),
            self.split_3_crossfade.value(),
        ];
        for ((s, note), crossfade) in engine
            .split_router
            .splits
            .iter_mut()
            .zip(split_notes)
            .zip(split_crossfades)
        {
            s.enabled = split_on;
            s.midi_note = note;
            s.crossfade = match crossfade {
                1 => nord_core::morph::CrossfadeWidth::Small,
                2 => nord_core::morph::CrossfadeWidth::Large,
                _ => nord_core::morph::CrossfadeWidth::Off,
            };
        }

        // Morph assignments (depths only; base values are written below)
        self.morph_assign.sync_to_engine(engine);
        self.layer_fx.sync_to_engine(engine);
        self.layer_fx_selectors.sync_to_engine(engine);
        self.piano_layers.sync_to_engine(engine);
        self.synth_layers.sync_to_engine(engine);
        self.program.sync_to_engine(engine);
        self.synth_mod.sync_to_engine(engine);
        self.synth_gate.sync_to_engine(engine);
        self.organ_layers.sync_to_engine(engine);
        self.layer_fx_extra.sync_to_engine(engine);
        self.layer_fx_sharing.sync_to_engine(engine);

        // Morph controllers
        engine.morph_controllers.mod_wheel = self.mod_wheel.value();
        engine.morph_controllers.control_pedal = self.control_pedal.value();
        engine.morph_controllers.aftertouch = self.aftertouch.value();

        // Layer levels
        let master_gain = self.master_level.value();
        engine.layer_levels[0].base = self.organ_a_level.value() * master_gain;
        engine.layer_levels[1].base = self.organ_b_level.value() * master_gain;
        engine.layer_levels[2].base = self.piano_a_level.value() * master_gain;
        engine.layer_levels[3].base = self.piano_b_level.value() * master_gain;
        engine.layer_levels[4].base = self.synth_a_level.value() * master_gain;
        engine.layer_levels[5].base = self.synth_b_level.value() * master_gain;
        engine.layer_levels[6].base = self.synth_c_level.value() * master_gain;

        // Organ settings
        let organ_model = match self.organ_model.value() {
            1 => OrganModel::Vox,
            2 => OrganModel::Farfisa,
            3 => OrganModel::Pipe1,
            4 => OrganModel::Pipe2,
            _ => OrganModel::B3,
        };
        engine.organ_a.model = organ_model;
        engine.organ_b.model = organ_model;

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
        engine.organ_a.drawbars = drawbars;

        engine.organ_b.drawbars = [
            self.drawbar_b_1_16.value() as u8,
            self.drawbar_b_2_5_1_3.value() as u8,
            self.drawbar_b_3_8.value() as u8,
            self.drawbar_b_4_4.value() as u8,
            self.drawbar_b_5_2_2_3.value() as u8,
            self.drawbar_b_6_2.value() as u8,
            self.drawbar_b_7_1_3_5.value() as u8,
            self.drawbar_b_8_1_1_3.value() as u8,
            self.drawbar_b_9_1.value() as u8,
        ];

        engine.organ_a.percussion_enabled = self.organ_perc_enable.value();
        engine.organ_a.percussion_soft = self.organ_perc_soft.value();
        engine.organ_a.percussion_fast_decay = self.organ_perc_fast.value();
        engine.organ_a.percussion_third_harmonic = self.organ_perc_third.value();

        let spd = if self.rotary_speed_fast.value() {
            RotarySpeed::Fast
        } else {
            RotarySpeed::Slow
        };
        engine.organ_a.rotary_speed = spd;
        engine.organ_b.rotary_speed = spd;
        engine.organ_a.rotary_drive = self.rotary_drive.value();
        engine.organ_b.rotary_drive = self.rotary_drive.value();

        // Piano settings
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
            _ => PianoTimbre::Mid,
        };
        let touch = match self.piano_touch.value() {
            1 => VelocityTouch::Light,
            2 => VelocityTouch::Heavy,
            _ => VelocityTouch::Medium,
        };

        engine.piano_a.category = cat;
        engine.piano_b.category = cat;
        engine.piano_a.timbre = timbre;
        engine.piano_b.timbre = timbre;
        engine.piano_a.touch = touch;
        engine.piano_b.touch = touch;
        engine.piano_a.string_resonance_enabled = self.piano_string_res.value();
        engine.piano_b.string_resonance_enabled = self.piano_string_res.value();
        engine.piano_a.pedal_noise_enabled = self.piano_pedal_noise.value();
        engine.piano_b.pedal_noise_enabled = self.piano_pedal_noise.value();

        // Synth settings
        let osc_m = match self.synth_osc_mode.value() {
            1 => SynthOscMode::Wavetable,
            2 => SynthOscMode::Fm,
            3 => SynthOscMode::Sample,
            _ => SynthOscMode::Analog,
        };
        let cutoff = self.synth_filter_cutoff.value();
        let resonance = self.synth_filter_resonance.value();
        let drive = self.synth_filter_drive.value();
        let atk = self.synth_attack.value();
        let dec = self.synth_decay.value();
        let rel = self.synth_release.value();

        for s in [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ] {
            s.osc_mode = osc_m;
            s.cutoff_hz = cutoff;
            s.resonance = resonance;
            s.filter_drive = drive;
            s.attack_sec = atk;
            s.decay_sec = dec;
            s.release_sec = rel;
        }

        // Per-layer extras override the global Organ/Piano blocks above.
        self.organ_extra.sync_to_engine(engine);
        self.piano_extra.sync_to_engine(engine);
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
        assert_eq!(engine.organ_b.drawbars[0], 8);
        assert_eq!(engine.split_router.splits[0].midi_note, 53);
        assert_eq!(engine.split_router.splits[1].midi_note, 65);
        assert_eq!(engine.split_router.splits[2].midi_note, 77);
        assert_eq!(engine.organ_a.model, OrganModel::B3);
        assert_eq!(engine.piano_a.timbre, PianoTimbre::Mid);
        assert_eq!(engine.synth_a.osc_mode, SynthOscMode::Analog);
        assert_eq!(engine.synth_a.cutoff_hz, 2500.0);
    }

    #[test]
    fn test_morph_assign_flows_through_sync() {
        let mut params = Stage4Params::default();
        params.morph_assign.layer_level.targets[0] =
            crate::morph_assign::MorphDeltaParams::with_defaults(0.0, 0.0, -0.5);
        let mut engine = Stage4Engine::new(44100.0);
        params.sync_to_engine(&mut engine);
        assert_eq!(engine.layer_levels[0].delta_aftertouch, -0.5);
        assert!((engine.layer_levels[0].base - 0.8 * 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_all_param_ids_unique_including_morph_assign() {
        let params = Stage4Params::default();
        let ids: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "duplicate host parameter IDs");
        assert!(ids.contains(&"morph_level_wheel_1".to_string()));
        assert!(ids.contains(&"morph_cutoff_at_3".to_string()));
        assert!(ids.contains(&"fx_delay_mix_7".to_string()));
        assert!(ids.contains(&"fxsel_rev_model_7".to_string()));
        assert!(ids.contains(&"fxsel_mod1_type_1".to_string()));
    }
}
