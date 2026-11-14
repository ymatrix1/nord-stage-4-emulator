//! Per-voice state of the polyphonic synth layer.

use super::*;

/// Individual polyphonic synth voice.
#[derive(Debug, Clone, Copy)]
pub struct SynthVoice {
    pub active: bool,
    pub gate: bool,
    pub note: u8,
    pub velocity: f32,
    /// Pattern pan `-1..=1` (`0` = unpanned, bit-identical mix).
    pub pan: f32,
    pub gain: f32,
    /// Gain target latched at the control tick (`gain_dezip.rs`).
    pub gain_target: f32,
    /// Smoothed gain applied when the de-zipper is on.
    pub gain_state: f32,
    pub freq_hz: f32,
    pub phase: f32,
    pub mod_phase: f32,
    pub op3_phase: f32,
    pub mod_feedback_state: f32,
    pub mod_env: f32,
    /// Target pitch for glide; `freq_hz` slews toward it.
    pub target_freq_hz: f32,
    /// Dedicated filter envelope level (`0..1`).
    pub fenv: f32,
    pub fenv_attack_done: bool,
    /// Oscillator envelope level (`0..1`).
    pub oenv: f32,
    pub oenv_attack_done: bool,
    /// Seconds since note-on (vibrato delay).
    pub age_sec: f32,
    /// Unison detuned oscillator phases (low, high).
    pub uni_phase: [f32; 4],
    pub uni_supersaw: [[f32; SUPERSAW_VOICES]; 4],
    pub supersaw_phases: [f32; SUPERSAW_VOICES],
    pub amp_env: f32,
    pub filter: ZdfFilter,
    pub filter_right: ZdfFilter,
    /// Firmware filter states (`FilterEngine::Firmware`), left and right.
    pub fw_filter: FwFilter,
    pub fw_filter_right: FwFilter,
    /// Firmware strip chain state (`SynthChain::FirmwareStrip`).
    pub strip: StripVoice,
    pub sample_zone_idx: Option<usize>,
    pub sample_pos: f64,
    pub sample_step: f64,
    pub sample_lp_state: f32,
    pub sample_lp_state_right: f32,
}

impl Default for SynthVoice {
    fn default() -> Self {
        Self {
            active: false,
            gate: false,
            note: 0,
            velocity: 0.8,
            pan: 0.0,
            gain: 1.0,
            gain_target: 1.0,
            gain_state: 0.0,
            freq_hz: 440.0,
            phase: 0.0,
            mod_phase: 0.0,
            op3_phase: 0.0,
            mod_feedback_state: 0.0,
            mod_env: 0.0,
            target_freq_hz: 440.0,
            fenv: 0.0,
            fenv_attack_done: false,
            oenv: 0.0,
            oenv_attack_done: false,
            age_sec: 0.0,
            uni_phase: UNISON_START_PHASES,
            uni_supersaw: [SUPERSAW_PHASE_INITS; 4],
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.0,
            filter: ZdfFilter::default(),
            filter_right: ZdfFilter::default(),
            fw_filter: FwFilter::default(),
            fw_filter_right: FwFilter::default(),
            strip: StripVoice::default(),
            sample_zone_idx: None,
            sample_pos: 0.0,
            sample_step: 1.0,
            sample_lp_state: 0.0,
            sample_lp_state_right: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_voice_is_idle_with_unit_sample_step() {
        let v = SynthVoice::default();
        assert!(!v.active);
        assert_eq!(v.sample_step, 1.0);
    }
}
