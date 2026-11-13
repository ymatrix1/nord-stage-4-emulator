//! Front-panel bay: synth layers, oscillator, filter and envelopes.

use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Painter, Rect, Ui};
use nord_core::synth::{SynthOscMode, VaWaveform};

/// Draws and drives the bay: synth layers, oscillator, filter and envelopes.
pub(super) fn render_bay_synth(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 5: SYNTH SECTION (Slate: x: 1020..1348, y: 24..300)
    // =========================================================================
    let bay5 = Rect::from_min_max(
        pos2(panel_rect.left() + 1020.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1348.0, panel_rect.top() + 300.0),
    );
    section_frame(painter, bay5, "SYNTH SECTION");

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
        painter,
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
        painter,
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
        painter,
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
        painter,
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
        painter,
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
        painter,
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
    group_frame(painter, flt_rect, "FILTER");

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
    group_frame(painter, env_rect, "AMP ENV");

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::run_panel_frame;

    #[test]
    fn test_render_bay_synth_draws_shapes_headless() {
        let shapes = run_panel_frame(|ui, painter, rect, state, audio| {
            render_bay_synth(ui, painter, rect, state, audio);
        });
        assert!(shapes > 0);
    }
}
