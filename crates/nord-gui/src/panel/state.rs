//! Interactive front-panel state.

use super::prelude::*;
use super::render_stage4_panel;

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

impl Stage4PanelState {
    /// Synchronizes the front-panel controls with a loaded Clavia `ProgramDecoded`.
    pub fn apply_program(&mut self, prog: &nord_core::cbin::ProgramDecoded) {
        if !prog.name.is_empty() {
            self.prog_title = prog.name.clone();
        }
        let bank_ch = (b'A' + (prog.bank_index as u8).min(7)) as char;
        let slot_idx = prog.location_index & 63;
        let page = (slot_idx / 8) + 1;
        let btn = (slot_idx % 8) + 1;
        self.prog_id = format!("{bank_ch}:{page}{btn}");
        self.active_program_idx = btn as u8;

        let cat_mask = match prog.category_id {
            0x07 | 0x12 => 1 << 0,                             // Organ A
            0x09 | 0x16 | 0x17 | 0x18 | 0x19 => 1 << 2,        // Piano A
            0x1E => (1 << 2) | (1 << 4),                       // Piano A + Synth A
            0x2A..=0x2F | 0x06 | 0x0B | 0x0D | 0x34 => 1 << 4, // Synth A
            _ => (1 << 0) | (1 << 2) | (1 << 4),
        };
        let scene_mask = if prog.global.scene_1_mask != 0 {
            prog.global.scene_1_mask
        } else {
            cat_mask
        };

        // Organ section
        self.organ_a_active = (scene_mask & (1 << 0)) != 0;
        self.organ_b_active = (scene_mask & (1 << 1)) != 0;
        self.organ_a_level = prog.organ.layer_a_level;
        self.organ_b_level = prog.organ.layer_b_level;
        self.organ_model = prog.organ.model;
        self.organ_drawbars = prog.organ.drawbars_a;
        self.scanner_on = prog.organ.vibrato_on;

        // Piano section
        self.piano_a_active = (scene_mask & (1 << 2)) != 0;
        self.piano_b_active = (scene_mask & (1 << 3)) != 0;
        self.piano_a_level = prog.piano.layer_a.level;
        self.piano_b_level = prog.piano.layer_b.level;
        self.piano_cat = prog.piano.layer_a.category;
        self.piano_timbre = prog.piano.layer_a.timbre;
        self.piano_touch = prog.piano.layer_a.touch;
        self.string_res = prog.piano.layer_a.string_res;
        self.pedal_noise = prog.piano.layer_a.pedal_noise;

        // Synth section
        self.synth_a_active = (scene_mask & (1 << 4)) != 0;
        self.synth_b_active = (scene_mask & (1 << 5)) != 0;
        self.synth_c_active = (scene_mask & (1 << 6)) != 0;
        self.synth_a_level = prog.synth.layer_a.level;
        self.synth_b_level = prog.synth.layer_b.level;
        self.synth_c_level = prog.synth.layer_c.level;
        self.synth_osc_mode = prog.synth.layer_a.osc_mode;
        self.synth_waveform = prog.synth.layer_a.waveform;
        self.synth_filter_model = prog.synth.layer_a.filter_model;

        // Global / FX focus on first active primary section
        self.fx_focus = if self.synth_a_active {
            2
        } else if self.piano_a_active {
            1
        } else {
            0
        };
        let primary_fx = match self.fx_focus {
            2 => &prog.synth.fx_a,
            1 => &prog.piano.fx_a,
            _ => &prog.organ.fx,
        };
        self.mod1_type = primary_fx.mod1_type();
        self.mod2_type = primary_fx.mod2_type();
        self.delay_on = primary_fx.delay_enabled;
        self.reverb_model = primary_fx.reverb_model();

        // Physical potentiometer knobs remain fixed if sync_knobs_on_program_load is false (matching NS4 hardware).
        if self.sync_knobs_on_program_load {
            self.synth_cutoff = prog.synth.layer_a.filter_cutoff_hz.clamp(40.0, 15000.0);
            self.synth_resonance = prog.synth.layer_a.filter_resonance.clamp(0.0, 1.0);
            self.synth_drive = prog.synth.layer_a.filter_drive.clamp(0.0, 1.0);
            self.synth_attack = (prog.synth.layer_a.env_attack_ms / 1000.0).clamp(0.001, 2.0);
            self.synth_decay = (prog.synth.layer_a.env_decay_ms / 1000.0).clamp(0.01, 4.0);
            self.synth_release = (prog.synth.layer_a.env_release_ms / 1000.0).clamp(0.01, 4.0);
            self.master_comp = (prog.global.master_compressor / 10.0).clamp(0.0, 1.0);
            self.mod1_rate = 0.1 + primary_fx.mod1_rate * 14.9;
            self.mod1_amount = if primary_fx.mod1_enabled {
                primary_fx.mod1_amount
            } else {
                0.0
            };
            self.mod2_rate = 0.1 + primary_fx.mod2_rate * 9.9;
            self.mod2_amount = if primary_fx.mod2_enabled {
                primary_fx.mod2_amount
            } else {
                0.0
            };
            self.eq_drive = if primary_fx.amp_enabled {
                (primary_fx.amp_drive / 10.0).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.eq_bass = primary_fx.eq_bass_db;
            self.eq_mid = primary_fx.eq_mid_db;
            self.eq_mid_freq = primary_fx.eq_mid_freq_hz();
            self.eq_treble = primary_fx.eq_treble_db;
            self.delay_feedback = primary_fx.delay_feedback;
            self.delay_mix = if primary_fx.delay_enabled {
                primary_fx.delay_mix
            } else {
                0.0
            };
            self.reverb_mix = if primary_fx.reverb_enabled {
                primary_fx.reverb_amount
            } else {
                0.0
            };
        }
    }
}

impl Stage4PanelState {
    /// Renders the front panel directly targeting a synchronous engine (e.g. for testing/offline rendering).
    pub fn ui(&mut self, ui: &mut Ui, engine: &mut Stage4Engine) {
        let (audio_handle, rx) = AudioHandle::new();
        render_stage4_panel(ui, self, &audio_handle);

        while let Ok(cmd) = rx.try_recv() {
            apply_audio_command(cmd, engine, &mut self.master_volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_values() {
        let s = Stage4PanelState::default();
        assert_eq!(s.prog_id, "A:11");
        assert_eq!(s.organ_drawbars[0], 8);
        assert!(s.mouse_held_note.is_none());
    }
}
