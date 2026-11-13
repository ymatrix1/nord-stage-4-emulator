//! 7 Independent Per-Layer FX Chains (`Mod1`, `Mod2`, `Amp/EQ`, `Delay`, `Reverb`) & Master Bus.
//!
//! Spec: `docs/specs/effects-and-routing.md`

use crate::morph::{MorphControllers, MorphParameter};
use std::f32::consts::TAU;

mod amp_eq;
mod biquad;
mod delay;
mod firmware_fx;
mod fw_allpass;
mod fw_amp_drive;
mod fw_amp_stage;
mod fw_amp_types;
mod fw_coeffs;
mod fw_compressor;
mod fw_delay;
mod fw_delay_table;
pub mod fw_delay_time;
mod fw_drive_tables;
pub mod fw_map;
mod fw_params;
mod fw_short_float;
mod fw_stage;
mod fw_tone_stack;
mod fw_words;
mod master;
mod master_iir;
mod modulation;
mod reverb;
#[cfg(test)]
mod support;
mod tables;
#[cfg(test)]
mod tests_allpass_golden;
#[cfg(test)]
mod tests_amp_golden;
#[cfg(test)]
mod tests_biquad_golden;
#[cfg(test)]
mod tests_chain;
#[cfg(test)]
mod tests_compressor_golden;
#[cfg(test)]
mod tests_delay;
#[cfg(test)]
mod tests_delay_golden;
#[cfg(test)]
mod tests_firmware_fx;
#[cfg(test)]
mod tests_fw_amp_chain;
#[cfg(test)]
mod tests_fw_delay_time;
#[cfg(test)]
mod tests_fw_map;
#[cfg(test)]
mod tests_fw_reverb_mix;
#[cfg(test)]
mod tests_fw_series;
#[cfg(test)]
mod tests_lfo_golden;
#[cfg(test)]
mod tests_master_golden;
#[cfg(test)]
mod tests_resonant_golden;
mod types;

pub use biquad::*;
pub use firmware_fx::FirmwareFx;
pub use fw_allpass::{ctx as fw_allpass_ctx, FwAllpass};
pub use fw_amp_drive::{ctx as fw_amp_drive_ctx, DriveTables, FwAmpDrive};
pub use fw_compressor::{ctx as fw_compressor_ctx, FwCompressor};
pub use fw_delay::{ctx as fw_delay_ctx, DelayIo, FwDelay};
pub use fw_map::AllpassType;
pub use fw_params::{
    compressor_for_amount, set_allpass, set_amp_drive, set_compressor_amount, set_delay,
    set_tone_stack, CMP_SLOPE,
};
pub use fw_tone_stack::{ctx as fw_tone_stack_ctx, FwToneStack2, FwToneStack4};
pub use master::MasterBus;
pub use tables::*;
pub use types::*;

/// Fixed stereo delay line capacity (131,072 samples ≈ 2.73 s at 48 kHz; zero heap allocations during `process`).
const DELAY_LINE_LEN: usize = 131_072;

/// Fixed stereo modulation buffer capacity for Chorus / Vibe / Flanger (2048 samples ≈ 42 ms at 48 kHz).
const MOD2_LINE_LEN: usize = 2048;

/// Number of delay lines in the stereo Feedback Delay Network (FDN) Reverb.
const FDN_SIZE: usize = 4;

/// Fixed FDN circular buffer capacity (8192 samples ≈ 170 ms at 48 kHz; zero runtime allocs).
const FDN_BUF_LEN: usize = 8192;

/// Base prime sample delay lengths for the 4 FDN delay lines (calibrated for Room @ 48 kHz).
const FDN_DELAYS: [usize; FDN_SIZE] = [1399, 1747, 2113, 2551];

/// Prime stereo input-diffuser allpass lengths (`[L0, L1, R0, R1]`) to eliminate metallic slapback.
const REV_AP_DELAYS: [usize; FDN_SIZE] = [149, 331, 179, 389];

/// Reads a linearly interpolated delayed sample from a circular buffer.
#[inline]
fn read_ring_interpolated(buf: &[f32], write_idx: usize, delay_samples: f32) -> f32 {
    let cap = buf.len();
    let d = delay_samples.clamp(1.0, (cap - 2) as f32);
    let read_pos = (write_idx as f32 - d).rem_euclid(cap as f32);
    let idx0 = (read_pos as usize) % cap;
    let idx1 = (idx0 + 1) % cap;
    let frac = read_pos - read_pos.floor();
    buf[idx0] + frac * (buf[idx1] - buf[idx0])
}

#[inline]
fn morph_active(p: &MorphParameter) -> bool {
    p.delta_wheel.abs() > 1e-5 || p.delta_pedal.abs() > 1e-5 || p.delta_aftertouch.abs() > 1e-5
}

/// Dedicated 5-slot stereo FX pipeline instantiated independently inside each of the 7 layers.
#[derive(Debug, Clone)]
pub struct LayerFxChain {
    pub sample_rate: f32,
    pub mod1_type: Mod1Type,
    pub mod1_rate_hz: f32,
    pub mod1_amount: f32,
    pub mod2_type: Mod2Type,
    pub mod2_rate_hz: f32,
    pub mod2_amount: f32,
    pub amp_cabinet: AmpCabinet,
    /// `Type II` / `Very Small Speaker` variation flag (stored; not yet modelled, ASSUMED no DSP difference).
    pub amp_variation: bool,
    /// Use the firmware biquad sets of the ARM Amp type switch where a mode is mapped to one
    /// (`fw_coeffs::amp_fw_case_for`, ASSUMED: `Small Speaker` only). Default `false`: RBJ voicing.
    pub amp_fw_coeffs: bool,
    /// Resonance (`0.0..=10.0`) of the `LP24` / `HP24` modes (replaces the Mid gain knob).
    pub eq_filter_res: f32,
    pub drive_amount: f32,
    pub eq_bass_db: f32,
    pub eq_mid_db: f32,
    pub eq_mid_freq_hz: f32,
    pub eq_treble_db: f32,
    pub comp_enabled: bool,
    pub comp_amount: f32,
    pub comp_fast: bool,
    pub delay_enabled: bool,
    pub delay_time_ms: f32,
    pub delay_clock_sync: bool,
    pub delay_subdivision: DelaySubdivision,
    /// Hardware master-clock fraction in beats; overrides `delay_subdivision` when `Some`.
    pub delay_sync_beats: Option<f32>,
    pub delay_analog: bool,
    pub delay_ping_pong: bool,
    pub delay_filter: DelayFilterMode,
    pub delay_fx: DelayFeedbackFx,
    /// Variation of the feedback effect (mono Ensemble/Vibe, wide Chorus, 40 ms Flam, 100 % wet Space).
    pub delay_fx_variation: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_color: ReverbColor,
    pub reverb_mix: f32,
    pub morph: FxMorphSet,
    /// Opt-in firmware closed forms (delay mix law, firmware compressor). Default all off.
    pub firmware_fx: FirmwareFx,
    /// 14 bit `kDlyRateClkDiv` value (`0..=16383`) of the program; read only with
    /// `firmware_fx.delay_time` (the engine sets it from the preset). `None`: engine time.
    pub delay_rate_raw: Option<u16>,
    fw_comp: Option<fw_stage::FwCompressorStage>,
    fw_amp: Option<fw_amp_stage::FwAmpStage>,
    fw_comp_amount: f32,
    mod1_phase: f32,
    wah_state_l: [f32; 2],
    wah_state_r: [f32; 2],
    mod2_phase: f32,
    mod2_buf_l: Box<[f32]>,
    mod2_buf_r: Box<[f32]>,
    mod2_write_idx: usize,
    phaser_ap_l: [f32; 4],
    phaser_ap_r: [f32; 4],
    phaser_fb: [f32; 2],
    cab_lp: Biquad,
    cab_hp: Biquad,
    cab_mid: Biquad,
    cab_shelf: Biquad,
    cab_cache: Option<(AmpCabinet, bool)>,
    flt_a: Biquad,
    flt_b: Biquad,
    /// Last designed `[mode, freq_hz, res]` for LP24/HP24 (`NaN` forces first design).
    flt_cache: [f32; 3],
    eq_bass: Biquad,
    eq_mid: Biquad,
    eq_treble: Biquad,
    /// Last designed `[bass_db, mid_db, mid_hz, treble_db]` (`NaN` forces first design).
    eq_cache: [f32; 4],
    comp_env: f32,
    delay_buf_l: Box<[f32]>,
    delay_buf_r: Box<[f32]>,
    delay_write_idx: usize,
    delay_smoothed_samples: f32,
    dfx_phase: f32,
    dfx_buf_l: Box<[f32]>,
    dfx_buf_r: Box<[f32]>,
    dfx_idx: usize,
    dfx_ap: Box<[[f32; 512]; 4]>,
    dfx_ap_idx: [usize; 4],
    delay_lp_state: [f32; 2],
    delay_hp_state: [f32; 2],
    fdn_bufs: Box<[[f32; FDN_BUF_LEN]; FDN_SIZE]>,
    fdn_write_indices: [usize; FDN_SIZE],
    fdn_lp_states: [f32; FDN_SIZE],
    rev_ap_bufs: Box<[[f32; 512]; FDN_SIZE]>,
    rev_ap_indices: [usize; FDN_SIZE],
    reverb_chorale_phase: f32,
}

impl LayerFxChain {
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        let default_delay_ms = 375.0;
        Self {
            sample_rate: fs,
            mod1_type: Mod1Type::Off,
            mod1_rate_hz: 4.0,
            mod1_amount: 0.4,
            mod2_type: Mod2Type::Off,
            mod2_rate_hz: 0.8,
            mod2_amount: 0.35,
            amp_cabinet: AmpCabinet::Off,
            amp_variation: false,
            amp_fw_coeffs: false,
            eq_filter_res: 0.0,
            drive_amount: 0.0,
            eq_bass_db: 0.0,
            eq_mid_db: 0.0,
            eq_mid_freq_hz: 1000.0,
            eq_treble_db: 0.0,
            comp_enabled: false,
            comp_amount: 0.0,
            comp_fast: false,
            delay_enabled: false,
            delay_time_ms: default_delay_ms,
            delay_clock_sync: false,
            delay_subdivision: DelaySubdivision::DottedEighth,
            delay_sync_beats: None,
            delay_analog: false,
            delay_ping_pong: false,
            delay_filter: DelayFilterMode::Off,
            delay_fx: DelayFeedbackFx::Off,
            delay_fx_variation: false,
            delay_feedback: 0.35,
            delay_mix: 0.25,
            reverb_model: ReverbModel::Stage,
            reverb_color: ReverbColor::Normal,
            reverb_mix: 0.15,
            morph: FxMorphSet::default(),
            firmware_fx: FirmwareFx::OFF,
            delay_rate_raw: None,
            fw_comp: None,
            fw_amp: None,
            fw_comp_amount: 0.0,
            mod1_phase: 0.0,
            wah_state_l: [0.0; 2],
            wah_state_r: [0.0; 2],
            mod2_phase: 0.0,
            mod2_buf_l: vec![0.0; MOD2_LINE_LEN].into_boxed_slice(),
            mod2_buf_r: vec![0.0; MOD2_LINE_LEN].into_boxed_slice(),
            mod2_write_idx: 0,
            phaser_ap_l: [0.0; 4],
            phaser_ap_r: [0.0; 4],
            phaser_fb: [0.0; 2],
            cab_lp: Biquad::identity(),
            cab_hp: Biquad::identity(),
            cab_mid: Biquad::identity(),
            cab_shelf: Biquad::identity(),
            cab_cache: None,
            flt_a: Biquad::identity(),
            flt_b: Biquad::identity(),
            flt_cache: [f32::NAN; 3],
            eq_bass: Biquad::identity(),
            eq_mid: Biquad::identity(),
            eq_treble: Biquad::identity(),
            eq_cache: [f32::NAN; 4],
            comp_env: 0.0,
            delay_buf_l: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_buf_r: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_write_idx: 0,
            delay_smoothed_samples: default_delay_ms * 0.001 * fs,
            dfx_phase: 0.0,
            dfx_buf_l: vec![0.0; DFX_BUF_LEN].into_boxed_slice(),
            dfx_buf_r: vec![0.0; DFX_BUF_LEN].into_boxed_slice(),
            dfx_idx: 0,
            dfx_ap: Box::new([[0.0; 512]; 4]),
            dfx_ap_idx: [0; 4],
            delay_lp_state: [0.0; 2],
            delay_hp_state: [0.0; 2],
            fdn_bufs: Box::new([[0.0; FDN_BUF_LEN]; FDN_SIZE]),
            fdn_write_indices: [0; FDN_SIZE],
            fdn_lp_states: [0.0; FDN_SIZE],
            rev_ap_bufs: Box::new([[0.0; 512]; FDN_SIZE]),
            rev_ap_indices: [0; FDN_SIZE],
            reverb_chorale_phase: 0.0,
        }
    }

    /// Drives Morph-assigned parameters from the live `Wheel` / `A.T.` / `CtrlPed` state.
    ///
    /// Allocation-free; parameters with all-zero deltas are left untouched.
    pub fn apply_morph(&mut self, ctrl: &MorphControllers) {
        let m = self.morph;
        if morph_active(&m.mod1_rate) {
            self.mod1_rate_hz = 0.1 + m.mod1_rate.evaluate(ctrl) * 14.9;
        }
        if morph_active(&m.mod1_amount) {
            self.mod1_amount = m.mod1_amount.evaluate(ctrl);
        }
        if morph_active(&m.mod2_rate) {
            self.mod2_rate_hz = 0.1 + m.mod2_rate.evaluate(ctrl) * 9.9;
        }
        if morph_active(&m.mod2_amount) {
            self.mod2_amount = m.mod2_amount.evaluate(ctrl);
        }
        if morph_active(&m.eq_mid_freq) {
            self.eq_mid_freq_hz = eq_freq_hz_from_norm(m.eq_mid_freq.evaluate(ctrl));
        }
        if morph_active(&m.amp_drive) {
            self.drive_amount = m.amp_drive.evaluate(ctrl);
        }
        if morph_active(&m.delay_mix) {
            self.delay_mix = m.delay_mix.evaluate(ctrl);
        }
        if morph_active(&m.delay_feedback) {
            self.delay_feedback = m.delay_feedback.evaluate(ctrl).min(0.95);
        }
        if morph_active(&m.reverb_amount) {
            self.reverb_mix = m.reverb_amount.evaluate(ctrl);
        }
    }

    /// Processes a single stereo frame `[left, right]` through all 5 layer FX stages at `bpm`.
    pub fn process_frame_with_bpm(&mut self, frame: [f32; 2], bpm: f32) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        let [l1, r1] = self.process_mod1(frame[0], frame[1], inv_fs);
        let [l2, r2] = self.process_mod2(l1, r1, inv_fs);
        let [l3, r3] = self.process_amp_eq_comp(l2, r2, inv_fs);
        let [l4, r4] = self.process_delay(l3, r3, bpm, inv_fs);
        self.process_reverb(l4, r4, inv_fs)
    }

    /// Processes a single stereo frame `[left, right]` through all 5 layer FX stages (120 BPM default).
    pub fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        self.process_frame_with_bpm(frame, 120.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chain_passes_finite_audio() {
        let mut fx = LayerFxChain::new(48_000.0);
        for n in 0..512 {
            let x = (n as f32 * 0.03).sin() * 0.5;
            let [l, r] = fx.process_frame([x, x]);
            assert!(l.is_finite() && r.is_finite());
        }
    }
}
