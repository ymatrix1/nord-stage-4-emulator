//! Plugin state serialization, restoration, and preset handling.
//!
//! Provides backwards-compatible JSON snapshot serialization for DAW projects,
//! plus loading of Clavia `.ns4p` programs into a caller-owned engine.

use crate::params::{MidiRoutingMode, Stage4Params};
use nord_core::cbin::{CbinError, CbinFile, ProgramDecoded};
use nord_core::engine::Stage4Engine;
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
    #[serde(default = "default_split_points")]
    pub split_points: [i32; 3],
    #[serde(default)]
    pub split_crossfades: [i32; 3],

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
    #[serde(default = "default_drawbars_b")]
    pub drawbars_b: [i32; 9],
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
            split_points: [
                params.split_1_note.value(),
                params.split_2_note.value(),
                params.split_3_note.value(),
            ],
            split_crossfades: [
                params.split_1_crossfade.value(),
                params.split_2_crossfade.value(),
                params.split_3_crossfade.value(),
            ],

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
            drawbars_b: [
                params.drawbar_b_1_16.value(),
                params.drawbar_b_2_5_1_3.value(),
                params.drawbar_b_3_8.value(),
                params.drawbar_b_4_4.value(),
                params.drawbar_b_5_2_2_3.value(),
                params.drawbar_b_6_2.value(),
                params.drawbar_b_7_1_3_5.value(),
                params.drawbar_b_8_1_1_3.value(),
                params.drawbar_b_9_1.value(),
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

    /// Serializes snapshot to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a snapshot from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

fn default_split_points() -> [i32; 3] {
    [53, 65, 77]
}

fn default_drawbars_b() -> [i32; 9] {
    [8; 9]
}

/// Loads a raw `.ns4p` program into an engine owned by the caller.
///
/// Call this before handing the engine to the audio callback. Updating host
/// parameters must go through NIH-plug's host/context APIs.
pub fn load_ns4p_into_engine(
    bytes: &[u8],
    engine: &mut Stage4Engine,
) -> Result<ProgramDecoded, CbinError> {
    let cbin = CbinFile::parse(bytes)?;
    let prog = ProgramDecoded::from_cbin_file(&cbin)?;
    engine.apply_program(&prog);
    Ok(prog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_snapshot_json_roundtrip() {
        let params = Stage4Params::default();
        let mut snapshot = PluginStateSnapshot::from_params(&params, "Rock Organ B3");
        assert_eq!(snapshot.preset_name, "Rock Organ B3");
        snapshot.master_bpm = 140.0;
        snapshot.drawbars[0] = 5;
        snapshot.drawbars_b[0] = 3;
        snapshot.split_points[0] = 48;
        snapshot.split_crossfades[1] = 1;

        let json = snapshot.to_json().expect("Serialization must succeed");
        let restored = PluginStateSnapshot::from_json(&json).expect("Deserialization must succeed");

        assert_eq!(snapshot, restored);

        assert_eq!(restored.master_bpm, 140.0);
        assert_eq!(restored.drawbars[0], 5);
        assert_eq!(restored.drawbars_b[0], 3);
        assert_eq!(restored.split_points[0], 48);
        assert_eq!(restored.split_crossfades[1], 1);
    }

    #[test]
    fn test_legacy_snapshot_defaults_new_parameters() {
        let snapshot = PluginStateSnapshot::from_params(&Stage4Params::default(), "Legacy");
        let mut value: serde_json::Value =
            serde_json::from_str(&snapshot.to_json().unwrap()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("split_points");
        object.remove("split_crossfades");
        object.remove("drawbars_b");

        let restored = PluginStateSnapshot::from_json(&value.to_string()).unwrap();
        assert_eq!(restored.split_points, [53, 65, 77]);
        assert_eq!(restored.split_crossfades, [0, 0, 0]);
        assert_eq!(restored.drawbars_b, [8; 9]);
    }

    #[test]
    fn test_load_program_into_engine() {
        let mut prog = ProgramDecoded::default();
        prog.global.tempo_bpm = 155.0;
        prog.organ.drawbars_a[0] = 7;
        prog.organ.drawbars_b[0] = 4;
        prog.piano.layer_a.string_res = false;
        prog.synth.layer_a.filter_cutoff_hz = 3200.0;
        prog.global.split_points = [48, 60, 72];
        prog.global.split_crossfades = [
            nord_core::morph::CrossfadeWidth::Off,
            nord_core::morph::CrossfadeWidth::Small,
            nord_core::morph::CrossfadeWidth::Large,
        ];

        let bytes = prog.to_cbin_file(0, 0).unwrap().to_bytes();
        let mut engine = Stage4Engine::new(44100.0);
        let loaded = load_ns4p_into_engine(&bytes, &mut engine).unwrap();
        assert!((loaded.global.tempo_bpm - 155.0).abs() < 0.3);
        assert!((engine.master_clock.bpm - 155.0).abs() < 0.3);
        assert_eq!(engine.organ_a.drawbars[0], 7);
        assert_eq!(engine.organ_b.drawbars[0], 4);
        assert_eq!(engine.split_router.splits[0].midi_note, 48);
        assert_eq!(
            engine.split_router.splits[1].crossfade,
            nord_core::morph::CrossfadeWidth::Small
        );
        assert_eq!(
            engine.split_router.splits[2].crossfade,
            nord_core::morph::CrossfadeWidth::Large
        );
    }
}
