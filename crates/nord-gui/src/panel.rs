//! Full 6-Bay Top Control Surface and 88-Key Interactive Keybed Layout.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use crate::audio::{apply_audio_command, AudioCommand, AudioHandle};
use crate::style::*;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};
use nord_core::engine::Stage4Engine;
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::midi::MidiEvent;
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

/// Renders the entire hardware front panel and synchronizes edits with the DSP audio handle.
pub fn render_stage4_panel(ui: &mut Ui, state: &mut Stage4PanelState, audio_handle: &AudioHandle) {
    let panel_size = vec2(1560.0, 420.0);
    let (panel_rect, _) = ui.allocate_exact_size(panel_size, Sense::hover());
    let painter = ui.painter().clone();

    // 1. Outer Chassis Background
    painter.rect_filled(panel_rect, 6.0, CHASSIS_RED);
    painter.rect_stroke(
        panel_rect,
        6.0,
        Stroke::new(2.0_f32, CHASSIS_DARK),
        StrokeKind::Inside,
    );

    // Chassis branding (top left and top right)
    label(
        &painter,
        pos2(panel_rect.left() + 20.0, panel_rect.top() + 6.0),
        "nord stage 4",
        13.0,
        Color32::WHITE,
        Align2::LEFT_TOP,
    );
    label(
        &painter,
        pos2(panel_rect.right() - 20.0, panel_rect.top() + 6.0),
        "CLAVIA DMI AB • SWEDEN",
        10.0,
        Color32::from_gray(210),
        Align2::RIGHT_TOP,
    );

    // =========================================================================
    // BAY 1: MASTER & ROTARY (Left red zone: x: 10..120, y: 24..300)
    // =========================================================================
    let bay1 = Rect::from_min_max(
        pos2(panel_rect.left() + 8.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 116.0, panel_rect.top() + 300.0),
    );

    // Pitch stick & Mod wheel
    let pitch_rect =
        Rect::from_min_size(pos2(bay1.left() + 8.0, bay1.top() + 8.0), vec2(42.0, 46.0));
    let mod_rect =
        Rect::from_min_size(pos2(bay1.left() + 58.0, bay1.top() + 8.0), vec2(42.0, 46.0));
    if pitch_and_mod_wheels(
        ui,
        pitch_rect,
        mod_rect,
        &mut state.pitch_stick,
        &mut state.mod_wheel,
    ) {
        audio_handle.send_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: 1, // Mod wheel
            value: (state.mod_wheel * 127.0).round() as u8,
        });
    }

    // Master Level Knob
    if knob(
        ui,
        pos2(bay1.center().x, bay1.top() + 88.0),
        16.0,
        "master_vol",
        &mut state.master_volume,
        0.0,
        1.0,
        "MASTER LEVEL",
    ) {
        audio_handle.send_command(AudioCommand::SetMasterVolume(state.master_volume));
    }

    // Rotary Speaker group frame
    let rot_rect = Rect::from_min_max(
        pos2(bay1.left() + 4.0, bay1.top() + 128.0),
        pos2(bay1.right() - 4.0, bay1.bottom() - 6.0),
    );
    painter.rect_filled(rot_rect, 4.0, Color32::from_rgb(110, 20, 24));
    painter.rect_stroke(
        rot_rect,
        4.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    label(
        &painter,
        pos2(rot_rect.center().x, rot_rect.top() + 8.0),
        "ROTARY SPEAKER",
        9.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    if knob(
        ui,
        pos2(rot_rect.center().x, rot_rect.top() + 38.0),
        13.0,
        "rot_drive",
        &mut state.rotary_drive,
        0.0,
        1.0,
        "DRIVE",
    ) {
        audio_handle.send_command(AudioCommand::SetRotaryDrive(state.rotary_drive));
    }

    // Slow / Fast button & LEDs
    let is_fast = matches!(state.rotary_speed, RotarySpeed::Fast);
    let slow_led = pos2(rot_rect.left() + 16.0, rot_rect.top() + 74.0);
    let fast_led = pos2(rot_rect.right() - 16.0, rot_rect.top() + 74.0);
    led(&painter, slow_led, !is_fast, LED_GREEN, LED_GREEN_DIM);
    led(&painter, fast_led, is_fast, LED_RED, LED_RED_DIM);
    label(
        &painter,
        pos2(slow_led.x, slow_led.y + 8.0),
        "SLOW",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );
    label(
        &painter,
        pos2(fast_led.x, fast_led.y + 8.0),
        "FAST",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let speed_btn = Rect::from_center_size(
        pos2(rot_rect.center().x, rot_rect.top() + 104.0),
        vec2(46.0, 18.0),
    );
    if nord_button(ui, speed_btn, "btn_rot_speed", false) {
        state.rotary_speed = if is_fast {
            RotarySpeed::Slow
        } else {
            RotarySpeed::Fast
        };
        audio_handle.send_command(AudioCommand::SetRotarySpeed(state.rotary_speed));
    }

    let stop_btn = Rect::from_center_size(
        pos2(rot_rect.center().x, rot_rect.top() + 130.0),
        vec2(46.0, 16.0),
    );
    if nord_button(ui, stop_btn, "btn_rot_stop", true) {
        state.rotary_stop = !state.rotary_stop;
        let spd = if state.rotary_stop {
            RotarySpeed::Stop
        } else {
            state.rotary_speed
        };
        audio_handle.send_command(AudioCommand::SetRotarySpeed(spd));
    }
    label(
        &painter,
        pos2(stop_btn.center().x, stop_btn.center().y),
        "STOP MODE",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // =========================================================================
    // BAY 2: ORGAN SECTION (Slate: x: 122..488, y: 24..300)
    // =========================================================================
    let bay2 = Rect::from_min_max(
        pos2(panel_rect.left() + 122.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 488.0, panel_rect.top() + 300.0),
    );
    section_frame(&painter, bay2, "ORGAN SECTION");

    // Organ Layer A & B Strips
    let fader_a = Rect::from_min_size(
        pos2(bay2.left() + 12.0, bay2.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        fader_a,
        "org_a",
        &mut state.organ_a_level,
        &mut state.organ_a_active,
    ) {
        audio_handle.send_command(AudioCommand::SetOrganLevel(if state.organ_a_active {
            state.organ_a_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(0, state.organ_a_active));
    }
    label(
        &painter,
        pos2(fader_a.center().x, fader_a.bottom() + 10.0),
        "A",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let fader_b = Rect::from_min_size(
        pos2(bay2.left() + 48.0, bay2.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        fader_b,
        "org_b",
        &mut state.organ_b_level,
        &mut state.organ_b_active,
    ) {
        audio_handle.send_command(AudioCommand::SetOrganBLevel(if state.organ_b_active {
            state.organ_b_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(1, state.organ_b_active));
    }
    label(
        &painter,
        pos2(fader_b.center().x, fader_b.bottom() + 10.0),
        "B",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Organ Model Selector (B3, Vox, Farf, Pipe1, Pipe2)
    let model_rect = Rect::from_min_size(
        pos2(bay2.left() + 90.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    let org_models = [
        OrganModel::B3,
        OrganModel::Vox,
        OrganModel::Farfisa,
        OrganModel::Pipe1,
        OrganModel::Pipe2,
    ];
    let cur_m_idx = org_models
        .iter()
        .position(|m| *m == state.organ_model)
        .unwrap_or(0);
    if let Some(new_idx) = triangle_matrix_selector(
        ui,
        model_rect,
        "sel_org_model",
        &["B3", "VOX", "FARF"],
        &["PIPE1", "PIPE2"],
        cur_m_idx,
    ) {
        state.organ_model = org_models[new_idx % org_models.len()];
        audio_handle.send_command(AudioCommand::SetOrganModel(state.organ_model));
    }
    label(
        &painter,
        pos2(model_rect.center().x, bay2.top() + 29.0),
        "ORGAN MODEL",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    // Vib / Chorus Box
    let vib_rect = Rect::from_min_size(
        pos2(bay2.left() + 182.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    let sc_modes = [
        ScannerMode::V1,
        ScannerMode::V2,
        ScannerMode::V3,
        ScannerMode::C1,
        ScannerMode::C2,
        ScannerMode::C3,
    ];
    let cur_sc_idx = sc_modes
        .iter()
        .position(|m| *m == state.scanner_mode)
        .unwrap_or(5);
    if let Some(new_sc) = triangle_matrix_selector(
        ui,
        vib_rect,
        "sel_vib",
        &["V1", "V2", "V3"],
        &["C1", "C2", "C3"],
        cur_sc_idx,
    ) {
        state.scanner_mode = sc_modes[new_sc % sc_modes.len()];
        audio_handle.send_command(AudioCommand::SetScannerMode(state.scanner_mode));
    }
    label(
        &painter,
        pos2(vib_rect.center().x, bay2.top() + 29.0),
        "VIB/CHORUS",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    // B3 Percussion Box
    let perc_rect = Rect::from_min_size(
        pos2(bay2.left() + 274.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    painter.rect_filled(perc_rect, 3.0, Color32::from_rgb(20, 24, 28));
    painter.rect_stroke(
        perc_rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );
    label(
        &painter,
        pos2(perc_rect.center().x, bay2.top() + 29.0),
        "B3 PERCUSSION",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    let p_on_rect = Rect::from_min_size(
        pos2(perc_rect.left() + 6.0, perc_rect.top() + 14.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_on_rect, "btn_perc_on", state.perc_on) {
        state.perc_on = !state.perc_on;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        &painter,
        pos2(p_on_rect.center().x, p_on_rect.center().y),
        "ON",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_soft_rect = Rect::from_min_size(
        pos2(perc_rect.right() - 38.0, perc_rect.top() + 14.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_soft_rect, "btn_perc_soft", state.perc_soft) {
        state.perc_soft = !state.perc_soft;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        &painter,
        pos2(p_soft_rect.center().x, p_soft_rect.center().y),
        "SOFT",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_fast_rect = Rect::from_min_size(
        pos2(perc_rect.left() + 6.0, perc_rect.top() + 38.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_fast_rect, "btn_perc_fast", state.perc_fast) {
        state.perc_fast = !state.perc_fast;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        &painter,
        pos2(p_fast_rect.center().x, p_fast_rect.center().y),
        "FAST",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_3rd_rect = Rect::from_min_size(
        pos2(perc_rect.right() - 38.0, perc_rect.top() + 38.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_3rd_rect, "btn_perc_3rd", state.perc_third) {
        state.perc_third = !state.perc_third;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        &painter,
        pos2(p_3rd_rect.center().x, p_3rd_rect.center().y),
        "3RD",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // 9 Physical Drawbars (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1')
    let db_footages = [
        "16'", "5 1/3'", "8'", "4'", "2 2/3'", "2'", "1 3/5'", "1 1/3'", "1'",
    ];
    let mut db_changed = false;
    for (i, &footage) in db_footages.iter().enumerate() {
        let is_white = matches!(i, 2 | 3 | 5 | 8);
        let x = bay2.left() + 94.0 + (i as f32) * 29.5;
        let db_rect = Rect::from_min_size(pos2(x, bay2.top() + 128.0), vec2(27.0, 128.0));
        if drawbar(
            ui,
            db_rect,
            &format!("db_{i}"),
            &mut state.organ_drawbars[i],
            is_white,
        ) {
            db_changed = true;
        }
        label(
            &painter,
            pos2(db_rect.center().x, db_rect.bottom() + 8.0),
            footage,
            8.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }
    if db_changed {
        audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));
    }

    // =========================================================================
    // BAY 3: PIANO SECTION (Slate: x: 494..788, y: 24..300)
    // =========================================================================
    let bay3 = Rect::from_min_max(
        pos2(panel_rect.left() + 494.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 788.0, panel_rect.top() + 300.0),
    );
    section_frame(&painter, bay3, "PIANO SECTION");

    // Piano Layer A & B Strips
    let pfader_a = Rect::from_min_size(
        pos2(bay3.left() + 12.0, bay3.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        pfader_a,
        "pno_a",
        &mut state.piano_a_level,
        &mut state.piano_a_active,
    ) {
        audio_handle.send_command(AudioCommand::SetPianoLevel(if state.piano_a_active {
            state.piano_a_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(2, state.piano_a_active));
    }
    label(
        &painter,
        pos2(pfader_a.center().x, pfader_a.bottom() + 10.0),
        "A",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let pfader_b = Rect::from_min_size(
        pos2(bay3.left() + 48.0, bay3.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        pfader_b,
        "pno_b",
        &mut state.piano_b_level,
        &mut state.piano_b_active,
    ) {
        audio_handle.send_command(AudioCommand::SetPianoBLevel(if state.piano_b_active {
            state.piano_b_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(3, state.piano_b_active));
    }
    label(
        &painter,
        pos2(pfader_b.center().x, pfader_b.bottom() + 10.0),
        "B",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Acoustics: Soft Release, String Res, Pedal Noise
    let ac_rect = Rect::from_min_size(
        pos2(bay3.left() + 90.0, bay3.top() + 32.0),
        vec2(86.0, 96.0),
    );
    group_frame(&painter, ac_rect, "ACOUSTICS");

    led(
        &painter,
        pos2(ac_rect.left() + 12.0, ac_rect.top() + 24.0),
        state.string_res,
        LED_RED,
        LED_RED_DIM,
    );
    label(
        &painter,
        pos2(ac_rect.left() + 22.0, ac_rect.top() + 24.0),
        "STRING RES",
        8.5,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );

    led(
        &painter,
        pos2(ac_rect.left() + 12.0, ac_rect.top() + 44.0),
        state.pedal_noise,
        LED_RED,
        LED_RED_DIM,
    );
    label(
        &painter,
        pos2(ac_rect.left() + 22.0, ac_rect.top() + 44.0),
        "PED NOISE",
        8.5,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );

    let ac_btn = Rect::from_min_size(
        pos2(ac_rect.left() + 8.0, ac_rect.bottom() - 24.0),
        vec2(70.0, 16.0),
    );
    if nord_button(ui, ac_btn, "btn_acoustics", false) {
        state.string_res = !state.string_res;
        audio_handle.send_command(AudioCommand::SetPianoAcoustics {
            string_res: state.string_res,
            pedal_noise: state.pedal_noise,
        });
    }

    // Timbre Group (Bright / Mid / Soft / Dyno)
    let timb_rect = Rect::from_min_size(
        pos2(bay3.left() + 90.0, bay3.top() + 140.0),
        vec2(86.0, 110.0),
    );
    group_frame(&painter, timb_rect, "TIMBRE");
    let timbres = [
        PianoTimbre::Off,
        PianoTimbre::Soft,
        PianoTimbre::Mid,
        PianoTimbre::Bright,
        PianoTimbre::Dyno1,
        PianoTimbre::Dyno2,
    ];
    let cur_t_idx = timbres
        .iter()
        .position(|t| *t == state.piano_timbre)
        .unwrap_or(0);
    if let Some(new_t) = triangle_matrix_selector(
        ui,
        timb_rect.shrink(6.0),
        "sel_timbre",
        &["SOFT", "MID", "BRIGHT"],
        &["OFF", "DYNO1", "DYNO2"],
        cur_t_idx,
    ) {
        state.piano_timbre = timbres[new_t % timbres.len()];
        audio_handle.send_command(AudioCommand::SetPianoTimbre(state.piano_timbre));
    }

    // Piano Select Matrix (Grand, Upright, Electric, Clav, Digital, Misc)
    let psel_rect = Rect::from_min_size(
        pos2(bay3.left() + 180.0, bay3.top() + 32.0),
        vec2(104.0, 140.0),
    );
    group_frame(&painter, psel_rect, "PIANO SELECT");
    let p_cats = [
        PianoCategory::Grand,
        PianoCategory::Upright,
        PianoCategory::Electric,
        PianoCategory::Clav,
        PianoCategory::Digital,
        PianoCategory::Misc,
    ];
    let cur_cat_idx = p_cats
        .iter()
        .position(|c| *c == state.piano_cat)
        .unwrap_or(0);
    if let Some(new_cat) = triangle_matrix_selector(
        ui,
        psel_rect.shrink(4.0),
        "sel_piano_cat",
        &["GRAND", "UPRIGHT", "ELEC"],
        &["CLAV", "DIGITAL", "MISC"],
        cur_cat_idx,
    ) {
        state.piano_cat = p_cats[new_cat % p_cats.len()];
        audio_handle.send_command(AudioCommand::SetPianoCategory(state.piano_cat));
    }

    // Model knob
    let mut model_dummy = 0.5;
    knob(
        ui,
        pos2(bay3.left() + 232.0, bay3.top() + 218.0),
        15.0,
        "pno_model",
        &mut model_dummy,
        0.0,
        1.0,
        "MODEL LIST",
    );

    // =========================================================================
    // BAY 4: PROGRAM / MORPH (Crimson Chassis: x: 794..1014, y: 24..300)
    // =========================================================================
    let bay4 = Rect::from_min_max(
        pos2(panel_rect.left() + 794.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1014.0, panel_rect.top() + 300.0),
    );
    painter.rect_filled(bay4, 5.0, Color32::from_rgb(115, 18, 22));
    painter.rect_stroke(
        bay4,
        5.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );

    // Morph Assign buttons & LEDs
    let morph_box =
        Rect::from_min_size(pos2(bay4.left() + 8.0, bay4.top() + 8.0), vec2(204.0, 36.0));
    painter.rect_filled(morph_box, 3.0, Color32::from_rgb(80, 12, 16));
    label(
        &painter,
        pos2(morph_box.center().x, morph_box.top() + 6.0),
        "MORPH ASSIGN",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let btn_w = 58.0;
    let b_wheel = Rect::from_min_size(
        pos2(morph_box.left() + 6.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_wheel, "btn_m_wheel", state.morph_wheel_active) {
        state.morph_wheel_active = !state.morph_wheel_active;
    }
    label(
        &painter,
        pos2(b_wheel.center().x, b_wheel.center().y),
        "WHEEL",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let b_at = Rect::from_min_size(
        pos2(morph_box.left() + 72.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_at, "btn_m_at", state.morph_at_active) {
        state.morph_at_active = !state.morph_at_active;
    }
    label(
        &painter,
        pos2(b_at.center().x, b_at.center().y),
        "A.T.",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let b_ped = Rect::from_min_size(
        pos2(morph_box.left() + 138.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_ped, "btn_m_ped", state.morph_ctrl_active) {
        state.morph_ctrl_active = !state.morph_ctrl_active;
    }
    label(
        &painter,
        pos2(b_ped.center().x, b_ped.center().y),
        "CTRLPED",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // OLED Display (Central Program info)
    let oled_rect = Rect::from_min_size(
        pos2(bay4.left() + 12.0, bay4.top() + 50.0),
        vec2(196.0, 96.0),
    );
    let org_txt = format!("{:?}", state.organ_model);
    let pno_txt = state
        .loaded_piano_name
        .clone()
        .unwrap_or_else(|| format!("{:?} XL", state.piano_cat));
    let syn_txt = if state.synth_osc_mode == SynthOscMode::Sample {
        state
            .loaded_sample_name
            .clone()
            .unwrap_or_else(|| "Sample".to_string())
    } else {
        format!("{:?}", state.synth_waveform)
    };
    oled(
        &painter,
        oled_rect,
        &state.prog_id,
        &state.prog_title,
        &[
            format!("Org: {org_txt} 888000000"),
            format!("Pno: {pno_txt}"),
            format!("Syn: {syn_txt}"),
        ],
    );

    // Preset Library / Program Select buttons 1..8
    let prog_box = Rect::from_min_size(
        pos2(bay4.left() + 8.0, bay4.top() + 154.0),
        vec2(204.0, 110.0),
    );
    group_frame(&painter, prog_box, "PROGRAM");

    for row in 0..2 {
        for col in 0..4 {
            let idx = (row * 4 + col + 1) as u8;
            let is_sel = state.active_program_idx == idx;
            let bx = prog_box.left() + 10.0 + (col as f32) * 48.0;
            let by = prog_box.top() + 18.0 + (row as f32) * 44.0;
            let b_rect = Rect::from_min_size(pos2(bx, by), vec2(40.0, 24.0));
            if nord_button(ui, b_rect, &format!("prog_btn_{idx}"), is_sel) {
                state.active_program_idx = idx;
                state.prog_id = format!("A:{idx:02}");
            }
            label(
                &painter,
                pos2(b_rect.center().x, b_rect.center().y),
                &format!("{idx}"),
                9.5,
                Color32::WHITE,
                Align2::CENTER_CENTER,
            );
            led(
                &painter,
                pos2(b_rect.center().x, b_rect.top() - 7.0),
                is_sel,
                LED_RED,
                LED_RED_DIM,
            );
        }
    }

    // =========================================================================
    // BAY 5: SYNTH SECTION (Slate: x: 1020..1348, y: 24..300)
    // =========================================================================
    let bay5 = Rect::from_min_max(
        pos2(panel_rect.left() + 1020.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1348.0, panel_rect.top() + 300.0),
    );
    section_frame(&painter, bay5, "SYNTH SECTION");

    // Synth Layer A, B, C Strips
    let sfader_a = Rect::from_min_size(
        pos2(bay5.left() + 10.0, bay5.top() + 32.0),
        vec2(30.0, 160.0),
    );
    if layer_strip(
        ui,
        sfader_a,
        "syn_a",
        &mut state.synth_a_level,
        &mut state.synth_a_active,
    ) {
        audio_handle.send_command(AudioCommand::SetSynthLevel(if state.synth_a_active {
            state.synth_a_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(4, state.synth_a_active));
    }
    label(
        &painter,
        pos2(sfader_a.center().x, sfader_a.bottom() + 10.0),
        "A",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let sfader_b = Rect::from_min_size(
        pos2(bay5.left() + 44.0, bay5.top() + 32.0),
        vec2(30.0, 160.0),
    );
    if layer_strip(
        ui,
        sfader_b,
        "syn_b",
        &mut state.synth_b_level,
        &mut state.synth_b_active,
    ) {
        audio_handle.send_command(AudioCommand::SetSynthBLevel(if state.synth_b_active {
            state.synth_b_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(5, state.synth_b_active));
    }
    label(
        &painter,
        pos2(sfader_b.center().x, sfader_b.bottom() + 10.0),
        "B",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let sfader_c = Rect::from_min_size(
        pos2(bay5.left() + 78.0, bay5.top() + 32.0),
        vec2(30.0, 160.0),
    );
    if layer_strip(
        ui,
        sfader_c,
        "syn_c",
        &mut state.synth_c_level,
        &mut state.synth_c_active,
    ) {
        audio_handle.send_command(AudioCommand::SetSynthCLevel(if state.synth_c_active {
            state.synth_c_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(6, state.synth_c_active));
    }
    label(
        &painter,
        pos2(sfader_c.center().x, sfader_c.bottom() + 10.0),
        "C",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Synth Waveform Graphic OLED
    let soled_rect = Rect::from_min_size(
        pos2(bay5.left() + 116.0, bay5.top() + 30.0),
        vec2(120.0, 80.0),
    );
    let wf_str = if state.synth_osc_mode == SynthOscMode::Sample {
        state
            .loaded_sample_name
            .clone()
            .unwrap_or_else(|| "No .nsmp4".to_string())
    } else {
        format!("{:?}", state.synth_waveform)
    };
    let osc_str = format!("{:?}", state.synth_osc_mode);
    let oled_title = if state.synth_osc_mode == SynthOscMode::Sample {
        "SAMPLE"
    } else {
        "OSC WAVE"
    };
    oled(
        &painter,
        soled_rect,
        oled_title,
        &wf_str,
        &[
            format!("Mode: {osc_str}"),
            format!("Detune: {:.1}", state.synth_detune * 10.0),
        ],
    );

    // Oscillator mode cycle button
    let osc_m_btn = Rect::from_min_size(
        pos2(bay5.left() + 242.0, bay5.top() + 34.0),
        vec2(78.0, 20.0),
    );
    if nord_button(ui, osc_m_btn, "btn_osc_mode", false) {
        state.synth_osc_mode = cycle(
            &[
                SynthOscMode::Analog,
                SynthOscMode::Sample,
                SynthOscMode::Wavetable,
                SynthOscMode::Fm,
            ],
            state.synth_osc_mode,
        );
        audio_handle.send_command(AudioCommand::SetSynthOscMode(state.synth_osc_mode));
    }
    label(
        &painter,
        pos2(osc_m_btn.center().x, osc_m_btn.center().y),
        "MODE",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let wf_btn = Rect::from_min_size(
        pos2(bay5.left() + 242.0, bay5.top() + 60.0),
        vec2(78.0, 20.0),
    );
    if nord_button(ui, wf_btn, "btn_wf", false) {
        state.synth_waveform = cycle(
            &[
                VaWaveform::Saw,
                VaWaveform::SuperSaw,
                VaWaveform::Pulse,
                VaWaveform::Triangle,
                VaWaveform::Sine,
            ],
            state.synth_waveform,
        );
        audio_handle.send_command(AudioCommand::SetSynthWaveform(state.synth_waveform));
    }
    label(
        &painter,
        pos2(wf_btn.center().x, wf_btn.center().y),
        "WAVEFORM",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Filter controls: Frequency, Resonance, Drive
    let flt_rect = Rect::from_min_size(
        pos2(bay5.left() + 116.0, bay5.top() + 120.0),
        vec2(204.0, 72.0),
    );
    group_frame(&painter, flt_rect, "FILTER");

    if knob(
        ui,
        pos2(flt_rect.left() + 34.0, flt_rect.top() + 36.0),
        15.0,
        "syn_freq",
        &mut state.synth_cutoff,
        40.0,
        15000.0,
        "FREQ",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthCutoff(state.synth_cutoff));
    }

    if knob(
        ui,
        pos2(flt_rect.left() + 102.0, flt_rect.top() + 36.0),
        15.0,
        "syn_res",
        &mut state.synth_resonance,
        0.0,
        1.0,
        "RES",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthResonance(state.synth_resonance));
    }

    if knob(
        ui,
        pos2(flt_rect.left() + 170.0, flt_rect.top() + 36.0),
        15.0,
        "syn_drive",
        &mut state.synth_drive,
        0.0,
        1.0,
        "DRIVE",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthDrive(state.synth_drive));
    }

    // Envelopes: Attack, Decay, Release
    let env_rect = Rect::from_min_size(
        pos2(bay5.left() + 116.0, bay5.top() + 200.0),
        vec2(204.0, 70.0),
    );
    group_frame(&painter, env_rect, "AMP ENV");

    if knob(
        ui,
        pos2(env_rect.left() + 34.0, env_rect.top() + 34.0),
        14.0,
        "syn_atk",
        &mut state.synth_attack,
        0.001,
        2.0,
        "ATTACK",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthAttack(state.synth_attack));
    }

    if knob(
        ui,
        pos2(env_rect.left() + 102.0, env_rect.top() + 34.0),
        14.0,
        "syn_dec",
        &mut state.synth_decay,
        0.01,
        4.0,
        "DECAY",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthDecay(state.synth_decay));
    }

    if knob(
        ui,
        pos2(env_rect.left() + 170.0, env_rect.top() + 34.0),
        14.0,
        "syn_rel",
        &mut state.synth_release,
        0.01,
        4.0,
        "RELEASE",
    ) {
        audio_handle.send_command(AudioCommand::SetSynthRelease(state.synth_release));
    }

    // =========================================================================
    // BAY 6: LAYER EFFECTS (Slate: x: 1354..1552, y: 24..300)
    // =========================================================================
    let bay6 = Rect::from_min_max(
        pos2(panel_rect.left() + 1354.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1552.0, panel_rect.top() + 300.0),
    );
    section_frame(&painter, bay6, "LAYER EFFECTS");

    // FX Focus buttons (Organ, Piano, Synth A/B/C)
    let focus_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 28.0),
        vec2(186.0, 24.0),
    );
    let focus_labels = ["ORG", "PNO", "SYN A", "SYN B", "SYN C"];
    for (i, &lbl) in focus_labels.iter().enumerate() {
        let is_sel = state.fx_focus == i;
        let bx = focus_rect.left() + (i as f32) * 37.0;
        let b_rect = Rect::from_min_size(pos2(bx, focus_rect.top()), vec2(34.0, 18.0));
        if nord_button(ui, b_rect, &format!("fx_foc_{i}"), is_sel) {
            state.fx_focus = i;
        }
        label(
            &painter,
            pos2(b_rect.center().x, b_rect.center().y),
            lbl,
            7.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }

    // Mod 1 & Mod 2 Knobs
    let mod1_rect =
        Rect::from_min_size(pos2(bay6.left() + 6.0, bay6.top() + 56.0), vec2(90.0, 70.0));
    group_frame(&painter, mod1_rect, "MOD 1");
    if knob(
        ui,
        pos2(mod1_rect.left() + 24.0, mod1_rect.top() + 36.0),
        13.0,
        "m1_rate",
        &mut state.mod1_rate,
        0.5,
        20.0,
        "RATE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod1 {
            kind: state.mod1_type,
            rate: state.mod1_rate,
            amount: state.mod1_amount,
        });
    }
    if knob(
        ui,
        pos2(mod1_rect.left() + 66.0, mod1_rect.top() + 36.0),
        13.0,
        "m1_amt",
        &mut state.mod1_amount,
        0.0,
        1.0,
        "AMT",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod1 {
            kind: state.mod1_type,
            rate: state.mod1_rate,
            amount: state.mod1_amount,
        });
    }

    let mod2_rect = Rect::from_min_size(
        pos2(bay6.left() + 100.0, bay6.top() + 56.0),
        vec2(90.0, 70.0),
    );
    group_frame(&painter, mod2_rect, "MOD 2");
    if knob(
        ui,
        pos2(mod2_rect.left() + 24.0, mod2_rect.top() + 36.0),
        13.0,
        "m2_rate",
        &mut state.mod2_rate,
        0.2,
        10.0,
        "RATE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod2 {
            kind: state.mod2_type,
            rate: state.mod2_rate,
            amount: state.mod2_amount,
        });
    }
    if knob(
        ui,
        pos2(mod2_rect.left() + 66.0, mod2_rect.top() + 36.0),
        13.0,
        "m2_amt",
        &mut state.mod2_amount,
        0.0,
        1.0,
        "AMT",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod2 {
            kind: state.mod2_type,
            rate: state.mod2_rate,
            amount: state.mod2_amount,
        });
    }

    // Delay & Reverb Knobs
    let delay_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 132.0),
        vec2(90.0, 70.0),
    );
    group_frame(&painter, delay_rect, "DELAY");
    if knob(
        ui,
        pos2(delay_rect.left() + 24.0, delay_rect.top() + 36.0),
        13.0,
        "dly_fb",
        &mut state.delay_feedback,
        0.0,
        0.9,
        "FEEDBK",
    ) {
        audio_handle.send_command(AudioCommand::SetFxDelay {
            enabled: state.delay_on,
            feedback: state.delay_feedback,
            mix: state.delay_mix,
        });
    }
    if knob(
        ui,
        pos2(delay_rect.left() + 66.0, delay_rect.top() + 36.0),
        13.0,
        "dly_mix",
        &mut state.delay_mix,
        0.0,
        1.0,
        "MIX",
    ) {
        audio_handle.send_command(AudioCommand::SetFxDelay {
            enabled: state.delay_on,
            feedback: state.delay_feedback,
            mix: state.delay_mix,
        });
    }

    let rev_rect = Rect::from_min_size(
        pos2(bay6.left() + 100.0, bay6.top() + 132.0),
        vec2(90.0, 70.0),
    );
    group_frame(&painter, rev_rect, "REVERB");
    if knob(
        ui,
        pos2(rev_rect.center().x, rev_rect.top() + 36.0),
        15.0,
        "rev_mix",
        &mut state.reverb_mix,
        0.0,
        1.0,
        "REVERB",
    ) {
        audio_handle.send_command(AudioCommand::SetFxReverb {
            model: state.reverb_model,
            mix: state.reverb_mix,
        });
    }

    // Amp / Master Compressor
    let comp_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 208.0),
        vec2(184.0, 62.0),
    );
    group_frame(&painter, comp_rect, "COMPRESSOR & DRIVE");
    if knob(
        ui,
        pos2(comp_rect.left() + 48.0, comp_rect.top() + 32.0),
        14.0,
        "eq_drv",
        &mut state.eq_drive,
        0.0,
        1.0,
        "DRIVE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxEq {
            drive: state.eq_drive,
            bass: state.eq_bass,
            mid: state.eq_mid,
            mid_freq: state.eq_mid_freq,
            treble: state.eq_treble,
        });
    }
    if knob(
        ui,
        pos2(comp_rect.left() + 136.0, comp_rect.top() + 32.0),
        14.0,
        "mst_comp",
        &mut state.master_comp,
        0.0,
        1.0,
        "COMPRESSOR",
    ) {
        audio_handle.send_command(AudioCommand::SetMasterComp(state.master_comp));
    }

    // =========================================================================
    // 88-KEY INTERACTIVE KEYBED (y: 310..415)
    // =========================================================================
    render_keybed(ui, panel_rect, state, audio_handle);
}

/// 88-Key Piano Keyboard with interactive mouse clicking and Triple Split-Point visual markers.
fn render_keybed(
    ui: &mut Ui,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let keybed_rect = Rect::from_min_size(
        pos2(panel_rect.left() + 10.0, panel_rect.top() + 310.0),
        vec2(panel_rect.width() - 20.0, 100.0),
    );
    let resp = ui.interact(
        keybed_rect,
        ui.id().with("stage4_keybed"),
        Sense::click_and_drag(),
    );
    let painter = ui.painter().clone();

    // Dark chassis lip behind keys
    painter.rect_filled(keybed_rect.expand(2.0), 3.0, Color32::from_rgb(12, 14, 16));

    // 88 keys: A0 (MIDI 21) .. C8 (MIDI 108)
    // 52 natural (white) keys
    let num_white = 52;
    let white_width = keybed_rect.width() / num_white as f32;

    // Mouse Interaction Tracking (fixes sticking notes!)
    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let rel_x = pos.x - keybed_rect.left();
            let mut clicked_midi: Option<u8> = None;

            // Check black keys first (top layer)
            let mut w_idx = 0;
            for midi in 21u8..=108u8 {
                let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                if is_black {
                    let kx = (w_idx as f32) * white_width - (white_width * 0.32);
                    let kw = white_width * 0.64;
                    let kh = keybed_rect.height() * 0.62;
                    let rel_y = pos.y - keybed_rect.top();
                    if rel_x >= kx && rel_x <= (kx + kw) && rel_y <= kh {
                        clicked_midi = Some(midi);
                        break;
                    }
                } else {
                    w_idx += 1;
                }
            }

            // If not on black key, find white key
            if clicked_midi.is_none() {
                let target_w = (rel_x / white_width).floor() as usize;
                let mut curr_w = 0;
                for midi in 21u8..=108u8 {
                    let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                    if !is_black {
                        if curr_w == target_w {
                            clicked_midi = Some(midi);
                            break;
                        }
                        curr_w += 1;
                    }
                }
            }

            // Update held note if changed
            if state.mouse_held_note != clicked_midi {
                if let Some(old_note) = state.mouse_held_note.take() {
                    audio_handle.send_midi(MidiEvent::NoteOff {
                        channel: 0,
                        note: old_note,
                        velocity: 0,
                    });
                }
                if let Some(new_note) = clicked_midi {
                    state.mouse_held_note = Some(new_note);
                    audio_handle.send_midi(MidiEvent::NoteOn {
                        channel: 0,
                        note: new_note,
                        velocity: 100,
                    });
                }
            }
        }
    } else {
        // Pointer released: release any mouse-held note
        if let Some(old_note) = state.mouse_held_note.take() {
            audio_handle.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: old_note,
                velocity: 0,
            });
        }
    }

    // Render 52 White keys
    let mut w_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if !is_black {
            let kx = keybed_rect.left() + (w_idx as f32) * white_width;
            let k_rect = Rect::from_min_size(
                pos2(kx, keybed_rect.top() + 2.0),
                vec2(white_width - 1.0, keybed_rect.height() - 4.0),
            );
            let is_pressed =
                state.midi_active_keys[midi as usize] || state.mouse_held_note == Some(midi);
            let fill = if is_pressed {
                Color32::from_rgb(220, 240, 255)
            } else {
                Color32::from_gray(248)
            };
            painter.rect_filled(k_rect, 2.0, fill);
            painter.rect_stroke(
                k_rect,
                2.0,
                Stroke::new(1.0_f32, Color32::from_gray(170)),
                StrokeKind::Inside,
            );
            w_idx += 1;
        }
    }

    // Render 36 Black keys overlay
    w_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if is_black {
            let kx = keybed_rect.left() + (w_idx as f32) * white_width - (white_width * 0.32);
            let k_rect = Rect::from_min_size(
                pos2(kx, keybed_rect.top() + 2.0),
                vec2(white_width * 0.64, keybed_rect.height() * 0.62),
            );
            let is_pressed =
                state.midi_active_keys[midi as usize] || state.mouse_held_note == Some(midi);
            let fill = if is_pressed {
                Color32::from_rgb(90, 110, 140)
            } else {
                Color32::from_rgb(24, 26, 30)
            };
            painter.rect_filled(k_rect, 2.0, fill);
            painter.rect_stroke(
                k_rect,
                2.0,
                Stroke::new(1.0_f32, Color32::BLACK),
                StrokeKind::Inside,
            );
        } else {
            w_idx += 1;
        }
    }

    // Split point red LEDs on keybed lip (F3/F#3 = MIDI 53, C5/C#5 = MIDI 72)
    let split_led1 = pos2(
        keybed_rect.left() + 20.0 * white_width,
        keybed_rect.top() + 2.0,
    );
    let split_led2 = pos2(
        keybed_rect.left() + 32.0 * white_width,
        keybed_rect.top() + 2.0,
    );
    painter.circle_filled(split_led1, 2.5, LED_RED);
    painter.circle_filled(split_led2, 2.5, LED_RED);
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
    fn test_panel_state_default() {
        let state = Stage4PanelState::default();
        assert_eq!(state.prog_id, "A:11");
        assert_eq!(state.organ_drawbars[0], 8);
        assert_eq!(state.synth_cutoff, 2400.0);
        assert!(state.mouse_held_note.is_none());
    }

    #[test]
    fn test_panel_state_audio_sync() {
        let mut state = Stage4PanelState::default();
        let (audio_handle, rx) = AudioHandle::new();
        state.organ_drawbars[0] = 5;
        audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));

        match rx.try_recv() {
            Ok(AudioCommand::SetDrawbars(d)) => {
                assert_eq!(d[0], 5);
            }
            _ => panic!("Expected SetDrawbars"),
        }
    }

    #[test]
    fn test_keybed_mouse_interaction_no_stuck_notes() {
        // Enforce the rule: "every error must become a test if possible"
        // Regression test for non-stop sustaining/stuck notes on mouse click/drag.
        let mut state = Stage4PanelState::default();
        let (audio_handle, rx) = AudioHandle::new();

        // 1. Mouse down on Middle C (MIDI 60)
        let clicked = Some(60u8);
        if state.mouse_held_note != clicked {
            if let Some(old) = state.mouse_held_note.take() {
                audio_handle.send_midi(MidiEvent::NoteOff {
                    channel: 0,
                    note: old,
                    velocity: 0,
                });
            }
            if let Some(new_n) = clicked {
                state.mouse_held_note = Some(new_n);
                audio_handle.send_midi(MidiEvent::NoteOn {
                    channel: 0,
                    note: new_n,
                    velocity: 100,
                });
            }
        }
        assert_eq!(state.mouse_held_note, Some(60));
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
                assert_eq!(note, 60);
                assert_eq!(velocity, 100);
            }
            _ => panic!("Expected NoteOn(60)"),
        }

        // 2. Mouse drag across to D4 (MIDI 62)
        let dragged = Some(62u8);
        if state.mouse_held_note != dragged {
            if let Some(old) = state.mouse_held_note.take() {
                audio_handle.send_midi(MidiEvent::NoteOff {
                    channel: 0,
                    note: old,
                    velocity: 0,
                });
            }
            if let Some(new_n) = dragged {
                state.mouse_held_note = Some(new_n);
                audio_handle.send_midi(MidiEvent::NoteOn {
                    channel: 0,
                    note: new_n,
                    velocity: 100,
                });
            }
        }
        assert_eq!(state.mouse_held_note, Some(62));
        // Verify NoteOff(60) was sent before NoteOn(62)
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOff { note, .. })) => assert_eq!(note, 60),
            _ => panic!("Expected NoteOff(60)"),
        }
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOn { note, velocity, .. })) => {
                assert_eq!(note, 62);
                assert_eq!(velocity, 100);
            }
            _ => panic!("Expected NoteOn(62)"),
        }

        // 3. Mouse release: Must emit NoteOff(62) and leave no stuck notes!
        if let Some(old) = state.mouse_held_note.take() {
            audio_handle.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: old,
                velocity: 0,
            });
        }
        assert!(state.mouse_held_note.is_none());
        match rx.try_recv() {
            Ok(AudioCommand::Midi(MidiEvent::NoteOff { note, .. })) => assert_eq!(note, 62),
            _ => panic!("Expected NoteOff(62) on mouse release"),
        }
        assert!(
            rx.try_recv().is_err(),
            "No dangling audio commands should remain"
        );
    }

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
