//! Applying a decoded Clavia program to the panel state.

use super::state::Stage4PanelState;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_state_apply_program() {
        let mut state = Stage4PanelState::default();
        assert!(!state.sync_knobs_on_program_load);
        state.sync_knobs_on_program_load = true;
        let mut prog = nord_core::cbin::ProgramDecoded {
            name: "Funk Solo".to_string(),
            bank_index: 0,
            location_index: 8,
            ..Default::default()
        };
        prog.organ.layer_a_enabled = true;
        prog.organ.layer_a_level = 0.77;
        prog.organ.drawbars_a = [8, 8, 8, 8, 0, 0, 0, 0, 0];
        prog.piano.layer_a.string_res = true;
        prog.piano.layer_a.pedal_noise = false;
        prog.synth.layer_a.enabled = true;
        prog.synth.layer_a.filter_cutoff_hz = 3200.0;
        prog.global.master_compressor = 4.0;

        state.apply_program(&prog);

        assert_eq!(state.prog_title, "Funk Solo");
        assert_eq!(state.prog_id, "A:21");
        assert_eq!(state.active_program_idx, 1);
        assert!(state.string_res);
        assert!(!state.pedal_noise);
        assert!(state.organ_a_active);
        assert!((state.organ_a_level - 0.77).abs() < 1e-5);
        assert_eq!(state.organ_drawbars, [8, 8, 8, 8, 0, 0, 0, 0, 0]);
        assert_eq!(state.fx_focus, 2);
        assert!((state.synth_cutoff - 3200.0).abs() < 1e-5);
        assert!((state.master_comp - 0.4).abs() < 1e-5);

        // When sync_knobs_on_program_load is false (NS4 hardware pot behavior), OLED/LED levels update while knobs stay fixed
        state.sync_knobs_on_program_load = false;
        let mut piano_prog = nord_core::cbin::ProgramDecoded {
            name: "White Grand".to_string(),
            bank_index: 0,
            location_index: 8,
            category_id: 0x09,
            ..Default::default()
        };
        piano_prog.global.scene_1_mask = 1 << 2;
        piano_prog.piano.layer_a.level = 0.91;
        piano_prog.piano.layer_a.string_res = true;
        piano_prog.piano.layer_a.pedal_noise = true;
        piano_prog.synth.layer_a.filter_cutoff_hz = 650.0;
        piano_prog.global.master_compressor = 8.0;
        state.apply_program(&piano_prog);

        assert_eq!(state.prog_title, "White Grand");
        assert_eq!(state.prog_id, "A:21");
        assert!(state.string_res);
        assert!(state.pedal_noise);
        assert_eq!(state.fx_focus, 1);
        assert!((state.piano_a_level - 0.91).abs() < 1e-5);
        assert!((state.synth_cutoff - 3200.0).abs() < 1e-5);
        assert!((state.master_comp - 0.4).abs() < 1e-5);
    }
}
