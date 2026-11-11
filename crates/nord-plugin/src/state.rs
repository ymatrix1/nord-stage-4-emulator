//! Plugin state serialization, restoration, and preset handling.
//!
//! Provides backwards-compatible JSON snapshot serialization for DAW projects and presets,
//! plus loading of Clavia `.ns4p` programs.

use crate::params::{MidiRoutingMode, Stage4Params};
use nord_core::cbin::{CbinError, CbinFile, ProgramDecoded};
use serde::{Deserialize, Serialize};

/// Serializable snapshot of the plugin state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginStateSnapshot {
    /// State version for backwards compatibility.
    pub version: u32,
    /// Human-readable preset or program name.
    pub preset_name: String,
    /// MIDI routing mode (0: OmniSplit, 1: MultitimbralPerLayer).
    #[serde(default)]
    pub midi_mode: u32,

    // Global
    pub master_level: f32,
    pub master_bpm: f32,
    pub master_compressor: f32,
    pub layer_scene_ii: bool,
    pub split_enabled: bool,

    // Morph
    pub mod_wheel: f32,
    pub control_pedal: f32,
    pub aftertouch: f32,

    // Organ
    pub organ_model: i32,
    pub organ_a_level: f32,
    pub organ_b_level: f32,
    pub organ_a_enabled: bool,
    pub organ_b_enabled: bool,
    pub drawbars: [i32; 9],
    pub organ_perc_enable: bool,
    pub organ_perc_soft: bool,
    pub organ_perc_fast: bool,
    pub organ_perc_third: bool,
    pub rotary_speed_fast: bool,
    pub rotary_drive: f32,

    // Piano
    pub piano_a_level: f32,
    pub piano_b_level: f32,
    pub piano_a_enabled: bool,
    pub piano_b_enabled: bool,
    pub piano_category: i32,
    pub piano_timbre: i32,
    pub piano_touch: i32,
    pub piano_string_res: bool,
    pub piano_pedal_noise: bool,

    // Synth
    pub synth_a_level: f32,
    pub synth_b_level: f32,
    pub synth_c_level: f32,
    pub synth_a_enabled: bool,
    pub synth_b_enabled: bool,
    pub synth_c_enabled: bool,
    pub synth_osc_mode: i32,
    pub synth_filter_cutoff: f32,
    pub synth_filter_resonance: f32,
    pub synth_filter_drive: f32,
    pub synth_attack: f32,
    pub synth_decay: f32,
    pub synth_release: f32,
}

impl PluginStateSnapshot {
    pub const CURRENT_VERSION: u32 = 1;

    /// Captures the current parameters into a snapshot.
    pub fn from_params(params: &Stage4Params, preset_name: &str) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            preset_name: preset_name.to_string(),
            midi_mode: match params.midi_mode.value() {
                MidiRoutingMode::OmniSplit => 0,
                MidiRoutingMode::MultitimbralPerLayer => 1,
            },

            master_level: params.master_level.value(),
            master_bpm: params.master_bpm.value(),
            master_compressor: params.master_compressor.value(),
            layer_scene_ii: params.layer_scene_ii.value(),
            split_enabled: params.split_enabled.value(),

            mod_wheel: params.mod_wheel.value(),
            control_pedal: params.control_pedal.value(),
            aftertouch: params.aftertouch.value(),

            organ_model: params.organ_model.value(),
            organ_a_level: params.organ_a_level.value(),
            organ_b_level: params.organ_b_level.value(),
            organ_a_enabled: params.organ_a_enabled.value(),
            organ_b_enabled: params.organ_b_enabled.value(),
            drawbars: [
                params.drawbar_1_16.value(),
                params.drawbar_2_5_1_3.value(),
                params.drawbar_3_8.value(),
                params.drawbar_4_4.value(),
                params.drawbar_5_2_2_3.value(),
                params.drawbar_6_2.value(),
                params.drawbar_7_1_3_5.value(),
                params.drawbar_8_1_1_3.value(),
                params.drawbar_9_1.value(),
            ],
            organ_perc_enable: params.organ_perc_enable.value(),
            organ_perc_soft: params.organ_perc_soft.value(),
            organ_perc_fast: params.organ_perc_fast.value(),
            organ_perc_third: params.organ_perc_third.value(),
            rotary_speed_fast: params.rotary_speed_fast.value(),
            rotary_drive: params.rotary_drive.value(),

            piano_a_level: params.piano_a_level.value(),
            piano_b_level: params.piano_b_level.value(),
            piano_a_enabled: params.piano_a_enabled.value(),
            piano_b_enabled: params.piano_b_enabled.value(),
            piano_category: params.piano_category.value(),
            piano_timbre: params.piano_timbre.value(),
            piano_touch: params.piano_touch.value(),
            piano_string_res: params.piano_string_res.value(),
            piano_pedal_noise: params.piano_pedal_noise.value(),

            synth_a_level: params.synth_a_level.value(),
            synth_b_level: params.synth_b_level.value(),
            synth_c_level: params.synth_c_level.value(),
            synth_a_enabled: params.synth_a_enabled.value(),
            synth_b_enabled: params.synth_b_enabled.value(),
            synth_c_enabled: params.synth_c_enabled.value(),
            synth_osc_mode: params.synth_osc_mode.value(),
            synth_filter_cutoff: params.synth_filter_cutoff.value(),
            synth_filter_resonance: params.synth_filter_resonance.value(),
            synth_filter_drive: params.synth_filter_drive.value(),
            synth_attack: params.synth_attack.value(),
            synth_decay: params.synth_decay.value(),
            synth_release: params.synth_release.value(),
        }
    }

    /// Restores snapshot values into the plugin parameters.
    pub fn apply_to_params(&self, params: &Stage4Params) {
        let midi_mode = if self.midi_mode == 1 {
            MidiRoutingMode::MultitimbralPerLayer
        } else {
            MidiRoutingMode::OmniSplit
        };
        set_enum(&params.midi_mode, midi_mode);

        set_float(&params.master_level, self.master_level);
        set_float(&params.master_bpm, self.master_bpm);
        set_float(&params.master_compressor, self.master_compressor);
        set_bool(&params.layer_scene_ii, self.layer_scene_ii);
        set_bool(&params.split_enabled, self.split_enabled);

        set_float(&params.mod_wheel, self.mod_wheel);
        set_float(&params.control_pedal, self.control_pedal);
        set_float(&params.aftertouch, self.aftertouch);

        set_int(&params.organ_model, self.organ_model);
        set_float(&params.organ_a_level, self.organ_a_level);
        set_float(&params.organ_b_level, self.organ_b_level);
        set_bool(&params.organ_a_enabled, self.organ_a_enabled);
        set_bool(&params.organ_b_enabled, self.organ_b_enabled);

        set_int(&params.drawbar_1_16, self.drawbars[0]);
        set_int(&params.drawbar_2_5_1_3, self.drawbars[1]);
        set_int(&params.drawbar_3_8, self.drawbars[2]);
        set_int(&params.drawbar_4_4, self.drawbars[3]);
        set_int(&params.drawbar_5_2_2_3, self.drawbars[4]);
        set_int(&params.drawbar_6_2, self.drawbars[5]);
        set_int(&params.drawbar_7_1_3_5, self.drawbars[6]);
        set_int(&params.drawbar_8_1_1_3, self.drawbars[7]);
        set_int(&params.drawbar_9_1, self.drawbars[8]);

        set_bool(&params.organ_perc_enable, self.organ_perc_enable);
        set_bool(&params.organ_perc_soft, self.organ_perc_soft);
        set_bool(&params.organ_perc_fast, self.organ_perc_fast);
        set_bool(&params.organ_perc_third, self.organ_perc_third);
        set_bool(&params.rotary_speed_fast, self.rotary_speed_fast);
        set_float(&params.rotary_drive, self.rotary_drive);

        set_float(&params.piano_a_level, self.piano_a_level);
        set_float(&params.piano_b_level, self.piano_b_level);
        set_bool(&params.piano_a_enabled, self.piano_a_enabled);
        set_bool(&params.piano_b_enabled, self.piano_b_enabled);
        set_int(&params.piano_category, self.piano_category);
        set_int(&params.piano_timbre, self.piano_timbre);
        set_int(&params.piano_touch, self.piano_touch);
        set_bool(&params.piano_string_res, self.piano_string_res);
        set_bool(&params.piano_pedal_noise, self.piano_pedal_noise);

        set_float(&params.synth_a_level, self.synth_a_level);
        set_float(&params.synth_b_level, self.synth_b_level);
        set_float(&params.synth_c_level, self.synth_c_level);
        set_bool(&params.synth_a_enabled, self.synth_a_enabled);
        set_bool(&params.synth_b_enabled, self.synth_b_enabled);
        set_bool(&params.synth_c_enabled, self.synth_c_enabled);
        set_int(&params.synth_osc_mode, self.synth_osc_mode);
        set_float(&params.synth_filter_cutoff, self.synth_filter_cutoff);
        set_float(&params.synth_filter_resonance, self.synth_filter_resonance);
        set_float(&params.synth_filter_drive, self.synth_filter_drive);
        set_float(&params.synth_attack, self.synth_attack);
        set_float(&params.synth_decay, self.synth_decay);
        set_float(&params.synth_release, self.synth_release);
    }

    /// Serializes snapshot to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a snapshot from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// Low-level atomic value update functions
fn set_float(param: &nih_plug::prelude::FloatParam, val: f32) {
    let base = param as *const _ as usize;
    unsafe {
        *((base + 168) as *mut f32) = val;
        *((base + 176) as *mut f32) = val;
        *((base + 188) as *mut f32) = val;
    }
}

fn set_int(param: &nih_plug::prelude::IntParam, val: i32) {
    let base = param as *const _ as usize;
    unsafe {
        *((base + 152) as *mut i32) = val;
        *((base + 160) as *mut i32) = val;
        *((base + 172) as *mut i32) = val;
    }
}

fn set_bool(param: &nih_plug::prelude::BoolParam, val: bool) {
    let base = param as *const _ as usize;
    unsafe {
        *((base + 96) as *mut bool) = val;
        *((base + 97) as *mut bool) = val;
        *((base + 98) as *mut bool) = val;
    }
}

fn set_enum(param: &nih_plug::prelude::EnumParam<MidiRoutingMode>, val: MidiRoutingMode) {
    let base = param as *const _ as usize;
    let idx = val as i32;
    unsafe {
        *((base + 152) as *mut i32) = idx;
        *((base + 160) as *mut i32) = idx;
        *((base + 172) as *mut i32) = idx;
    }
}

/// Applies a decoded Nord Stage 4 `ProgramDecoded` program directly to the plugin parameters.
pub fn apply_program_to_params(prog: &ProgramDecoded, params: &Stage4Params) {
    set_float(&params.master_bpm, prog.global.tempo_bpm);
    set_float(
        &params.master_compressor,
        (prog.global.master_compressor / 10.0).clamp(0.0, 1.0),
    );
    set_bool(&params.split_enabled, prog.global.split_enabled);

    set_float(&params.organ_a_level, prog.organ.layer_a_level);
    set_float(&params.organ_b_level, prog.organ.layer_b_level);
    set_int(&params.organ_model, prog.organ.model as i32);

    set_int(&params.drawbar_1_16, prog.organ.drawbars_a[0] as i32);
    set_int(&params.drawbar_2_5_1_3, prog.organ.drawbars_a[1] as i32);
    set_int(&params.drawbar_3_8, prog.organ.drawbars_a[2] as i32);
    set_int(&params.drawbar_4_4, prog.organ.drawbars_a[3] as i32);
    set_int(&params.drawbar_5_2_2_3, prog.organ.drawbars_a[4] as i32);
    set_int(&params.drawbar_6_2, prog.organ.drawbars_a[5] as i32);
    set_int(&params.drawbar_7_1_3_5, prog.organ.drawbars_a[6] as i32);
    set_int(&params.drawbar_8_1_1_3, prog.organ.drawbars_a[7] as i32);
    set_int(&params.drawbar_9_1, prog.organ.drawbars_a[8] as i32);

    set_bool(&params.organ_perc_enable, prog.organ.percussion_on);
    set_bool(&params.organ_perc_soft, prog.organ.percussion_soft);
    set_bool(&params.organ_perc_fast, prog.organ.percussion_fast);
    set_bool(&params.organ_perc_third, prog.organ.percussion_third);
    set_bool(
        &params.rotary_speed_fast,
        matches!(prog.organ.rotary_speed, nord_core::organ::RotarySpeed::Fast),
    );
    set_float(&params.rotary_drive, prog.organ.rotary_drive);

    set_float(&params.piano_a_level, prog.piano.layer_a.level);
    set_float(&params.piano_b_level, prog.piano.layer_b.level);
    set_int(&params.piano_category, prog.piano.layer_a.category as i32);
    set_int(&params.piano_timbre, prog.piano.layer_a.timbre as i32);
    set_int(&params.piano_touch, prog.piano.layer_a.touch as i32);
    set_bool(&params.piano_string_res, prog.piano.layer_a.string_res);
    set_bool(&params.piano_pedal_noise, prog.piano.layer_a.pedal_noise);

    set_float(&params.synth_a_level, prog.synth.layer_a.level);
    set_float(&params.synth_b_level, prog.synth.layer_b.level);
    set_float(&params.synth_c_level, prog.synth.layer_c.level);
    set_int(&params.synth_osc_mode, prog.synth.layer_a.osc_mode as i32);
    set_float(
        &params.synth_filter_cutoff,
        prog.synth.layer_a.filter_cutoff_hz,
    );
    set_float(
        &params.synth_filter_resonance,
        prog.synth.layer_a.filter_resonance,
    );
    set_float(&params.synth_filter_drive, prog.synth.layer_a.filter_drive);
    set_float(
        &params.synth_attack,
        prog.synth.layer_a.env_attack_ms / 1000.0,
    );
    set_float(
        &params.synth_decay,
        prog.synth.layer_a.env_decay_ms / 1000.0,
    );
    set_float(
        &params.synth_release,
        prog.synth.layer_a.env_release_ms / 1000.0,
    );
}

/// Loads a raw `.ns4p` program file and applies it to plugin parameters.
pub fn load_ns4p_bytes(bytes: &[u8], params: &Stage4Params) -> Result<ProgramDecoded, CbinError> {
    let cbin = CbinFile::parse(bytes)?;
    let prog = ProgramDecoded::from_cbin_file(&cbin)?;
    apply_program_to_params(&prog, params);
    Ok(prog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_snapshot_json_roundtrip() {
        let params = Stage4Params::default();
        set_float(&params.master_bpm, 140.0);
        set_int(&params.drawbar_1_16, 5);

        let snapshot = PluginStateSnapshot::from_params(&params, "Rock Organ B3");
        assert_eq!(snapshot.preset_name, "Rock Organ B3");
        assert_eq!(snapshot.master_bpm, 140.0);
        assert_eq!(snapshot.drawbars[0], 5);

        let json = snapshot.to_json().expect("Serialization must succeed");
        let restored = PluginStateSnapshot::from_json(&json).expect("Deserialization must succeed");

        assert_eq!(snapshot, restored);

        let new_params = Stage4Params::default();
        restored.apply_to_params(&new_params);
        assert_eq!(new_params.master_bpm.value(), 140.0);
        assert_eq!(new_params.drawbar_1_16.value(), 5);
    }

    #[test]
    fn test_apply_program_to_params() {
        let params = Stage4Params::default();
        let mut prog = ProgramDecoded::default();
        prog.global.tempo_bpm = 155.0;
        prog.organ.drawbars_a[0] = 7;
        prog.piano.layer_a.string_res = false;
        prog.synth.layer_a.filter_cutoff_hz = 3200.0;

        apply_program_to_params(&prog, &params);

        assert_eq!(params.master_bpm.value(), 155.0);
        assert_eq!(params.drawbar_1_16.value(), 7);
        assert!(!params.piano_string_res.value());
        assert_eq!(params.synth_filter_cutoff.value(), 3200.0);
    }
}
