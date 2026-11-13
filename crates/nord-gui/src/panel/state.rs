//! `Stage4PanelState`: every front-panel control value.

use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};

/// Complete Nord Stage 4 interactive front-panel GUI state.
pub struct Stage4PanelState {
    // Master & Controllers (Bay 1)
    pub master_volume: f32,
    pub pitch_stick: f32,
    pub mod_wheel: f32,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    pub rotary_stop: bool,

    // Organ Section (Bay 2)
    pub organ_a_active: bool,
    pub organ_b_active: bool,
    pub organ_a_level: f32,
    pub organ_b_level: f32,
    pub organ_model: OrganModel,
    pub organ_drawbars: [u8; 9],
    pub scanner_mode: ScannerMode,
    pub scanner_on: bool,
    pub perc_on: bool,
    pub perc_soft: bool,
    pub perc_fast: bool,
    pub perc_third: bool,
    pub organ_octave: i8,

    // Piano Section (Bay 3)
    pub piano_a_active: bool,
    pub piano_b_active: bool,
    pub piano_a_level: f32,
    pub piano_b_level: f32,
    pub piano_cat: PianoCategory,
    pub piano_timbre: PianoTimbre,
    pub piano_touch: VelocityTouch,
    pub string_res: bool,
    pub pedal_noise: bool,
    pub piano_octave: i8,
    pub loaded_piano_name: Option<String>,

    // Program / Morph Section (Bay 4)
    pub prog_id: String,
    pub prog_title: String,
    pub morph_wheel_active: bool,
    pub morph_at_active: bool,
    pub morph_ctrl_active: bool,
    pub split_active: bool,
    pub active_program_idx: u8,

    // Synth Section (Bay 5)
    pub synth_a_active: bool,
    pub synth_b_active: bool,
    pub synth_c_active: bool,
    pub synth_a_level: f32,
    pub synth_b_level: f32,
    pub synth_c_level: f32,
    pub synth_osc_mode: SynthOscMode,
    pub synth_waveform: VaWaveform,
    pub synth_detune: f32,
    pub synth_filter_model: FilterModel,
    pub synth_cutoff: f32,
    pub synth_resonance: f32,
    pub synth_drive: f32,
    pub synth_attack: f32,
    pub synth_decay: f32,
    pub synth_release: f32,
    pub synth_octave: i8,
    pub loaded_sample_name: Option<String>,

    // Layer Effects Section (Bay 6)
    pub fx_focus: usize,
    pub mod1_type: Mod1Type,
    pub mod1_rate: f32,
    pub mod1_amount: f32,
    pub mod2_type: Mod2Type,
    pub mod2_rate: f32,
    pub mod2_amount: f32,
    pub eq_drive: f32,
    pub eq_bass: f32,
    pub eq_mid: f32,
    pub eq_mid_freq: f32,
    pub eq_treble: f32,
    pub delay_on: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_mix: f32,
    pub master_comp: f32,

    // Keybed & Panel Recall Options
    pub sync_knobs_on_program_load: bool,
    pub mouse_held_note: Option<u8>,
    pub midi_active_keys: [bool; 128],
}

impl Default for Stage4PanelState {
    fn default() -> Self {
        Self {
            master_volume: 0.85,
            pitch_stick: 0.0,
            mod_wheel: 0.0,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.25,
            rotary_stop: false,

            organ_a_active: true,
            organ_b_active: false,
            organ_a_level: 0.85,
            organ_b_level: 0.0,
            organ_model: OrganModel::B3,
            organ_drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            scanner_mode: ScannerMode::C3,
            scanner_on: false,
            perc_on: true,
            perc_soft: false,
            perc_fast: true,
            perc_third: false,
            organ_octave: 0,

            piano_a_active: true,
            piano_b_active: false,
            piano_a_level: 0.90,
            piano_b_level: 0.0,
            piano_cat: PianoCategory::Grand,
            piano_timbre: PianoTimbre::Off,
            piano_touch: VelocityTouch::Medium,
            string_res: true,
            pedal_noise: true,
            piano_octave: 0,
            loaded_piano_name: None,

            prog_id: "A:11".to_string(),
            prog_title: "Nord Stage 4".to_string(),
            morph_wheel_active: false,
            morph_at_active: false,
            morph_ctrl_active: false,
            split_active: false,
            active_program_idx: 1,

            synth_a_active: false,
            synth_b_active: false,
            synth_c_active: false,
            synth_a_level: 0.70,
            synth_b_level: 0.0,
            synth_c_level: 0.0,
            synth_osc_mode: SynthOscMode::Analog,
            synth_waveform: VaWaveform::SuperSaw,
            synth_detune: 0.35,
            synth_filter_model: FilterModel::Ladder24,
            synth_cutoff: 2400.0,
            synth_resonance: 0.3,
            synth_drive: 0.2,
            synth_attack: 0.01,
            synth_decay: 0.35,
            synth_release: 0.25,
            synth_octave: 0,
            loaded_sample_name: None,

            fx_focus: 0,
            mod1_type: Mod1Type::Off,
            mod1_rate: 4.0,
            mod1_amount: 0.4,
            mod2_type: Mod2Type::Chorus,
            mod2_rate: 0.8,
            mod2_amount: 0.35,
            eq_drive: 0.0,
            eq_bass: 0.0,
            eq_mid: 0.0,
            eq_mid_freq: 1000.0,
            eq_treble: 0.0,
            delay_on: false,
            delay_feedback: 0.35,
            delay_mix: 0.25,
            reverb_model: ReverbModel::Stage,
            reverb_mix: 0.20,
            master_comp: 0.25,

            sync_knobs_on_program_load: false,
            mouse_held_note: None,
            midi_active_keys: [false; 128],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_state_default() {
        let state = Stage4PanelState::default();
        assert_eq!(state.prog_id, "A:11");
        assert_eq!(state.organ_drawbars[0], 8);
        assert_eq!(state.synth_cutoff, 2400.0);
        assert!(state.mouse_held_note.is_none());
    }
}
