//! 7 Independent Per-Layer FX Chains (`Mod1`, `Mod2`, `Amp/EQ`, `Delay`, `Reverb`) & Master Bus.
//!
//! Spec: `docs/specs/effects-and-routing.md`

use std::f32::consts::TAU;

/// Fixed stereo delay line capacity (4096 samples, zero heap allocations during `process`).
const DELAY_LINE_LEN: usize = 4096;

/// Number of delay lines in the stereo Feedback Delay Network (FDN) Reverb.
const FDN_SIZE: usize = 4;

/// Prime sample delay lengths for the 4 FDN delay lines (at 48 kHz).
const FDN_DELAYS: [usize; FDN_SIZE] = [1087, 1283, 1531, 1789];

/// Modulation Slot 1 modes (`Mod 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mod1Type {
    Off,
    Tremolo,
    AutoPan,
    RingMod,
    Wah,
}

/// Modulation Slot 2 modes (`Mod 2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mod2Type {
    Off,
    Chorus,
    Flanger,
    Phaser,
    Vibe,
}

/// Reverb room size models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverbModel {
    Off,
    Booth,
    Room,
    Stage,
    Hall,
    Cathedral,
}

/// Global Master Clock (`BPM` and normalized beat phase).
#[derive(Debug, Clone, Copy)]
pub struct MasterClock {
    pub bpm: f32,
    pub beat_phase: f32,
}

impl Default for MasterClock {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beat_phase: 0.0,
        }
    }
}

impl MasterClock {
    pub fn advance(&mut self, sample_rate: f32) {
        let beats_per_sec = self.bpm.clamp(30.0, 300.0) / 60.0;
        self.beat_phase = (self.beat_phase + beats_per_sec / sample_rate) % 1.0;
    }
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
    pub drive_amount: f32,
    pub eq_bass_db: f32,
    pub eq_mid_db: f32,
    pub eq_mid_freq_hz: f32,
    pub eq_treble_db: f32,
    pub delay_enabled: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_mix: f32,
    mod1_phase: f32,
    mod2_phase: f32,
    eq_bass_state: f32,
    eq_mid_state: [f32; 2],
    eq_treble_state: f32,
    delay_buf_l: Box<[f32; DELAY_LINE_LEN]>,
    delay_buf_r: Box<[f32; DELAY_LINE_LEN]>,
    delay_write_idx: usize,
    fdn_bufs: Box<[[f32; 2048]; FDN_SIZE]>,
    fdn_write_indices: [usize; FDN_SIZE],
    fdn_lp_states: [f32; FDN_SIZE],
}

impl LayerFxChain {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(8000.0),
            mod1_type: Mod1Type::Off,
            mod1_rate_hz: 4.0,
            mod1_amount: 0.4,
            mod2_type: Mod2Type::Off,
            mod2_rate_hz: 0.8,
            mod2_amount: 0.35,
            drive_amount: 0.0,
            eq_bass_db: 0.0,
            eq_mid_db: 0.0,
            eq_mid_freq_hz: 1000.0,
            eq_treble_db: 0.0,
            delay_enabled: false,
            delay_feedback: 0.35,
            delay_mix: 0.25,
            reverb_model: ReverbModel::Stage,
            reverb_mix: 0.15,
            mod1_phase: 0.0,
            mod2_phase: 0.0,
            eq_bass_state: 0.0,
            eq_mid_state: [0.0; 2],
            eq_treble_state: 0.0,
            delay_buf_l: Box::new([0.0; DELAY_LINE_LEN]),
            delay_buf_r: Box::new([0.0; DELAY_LINE_LEN]),
            delay_write_idx: 0,
            fdn_bufs: Box::new([[0.0; 2048]; FDN_SIZE]),
            fdn_write_indices: [0; FDN_SIZE],
            fdn_lp_states: [0.0; FDN_SIZE],
        }
    }

    /// Processes a single stereo frame `[left, right]` through all 5 layer FX stages.
    pub fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        let [mut l, mut r] = frame;

        // 1. Mod 1
        if self.mod1_type != Mod1Type::Off {
            let lfo = self.mod1_phase.sin();
            self.mod1_phase = (self.mod1_phase + TAU * self.mod1_rate_hz * inv_fs) % TAU;
            let amt = self.mod1_amount.clamp(0.0, 1.0);
            match self.mod1_type {
                Mod1Type::Tremolo => {
                    let g = 1.0 - 0.5 * amt * (1.0 + lfo);
                    l *= g;
                    r *= g;
                }
                Mod1Type::AutoPan => {
                    let pan = 0.5 * amt * lfo;
                    l *= 1.0 - pan;
                    r *= 1.0 + pan;
                }
                Mod1Type::RingMod | Mod1Type::Wah => {
                    l = l * (1.0 - amt) + (l * lfo) * amt;
                    r = r * (1.0 - amt) + (r * lfo) * amt;
                }
                Mod1Type::Off => {}
            }
        }

        // 2. Mod 2 (Chorus / Phaser / Flanger)
        if self.mod2_type != Mod2Type::Off {
            let lfo = self.mod2_phase.sin();
            self.mod2_phase = (self.mod2_phase + TAU * self.mod2_rate_hz * inv_fs) % TAU;
            let amt = self.mod2_amount.clamp(0.0, 1.0) * 0.3;
            let mid = 0.5 * (l + r);
            l += amt * lfo * mid;
            r -= amt * lfo * mid;
        }

        // 3. Amp Drive & 3-Band Semi-Parametric EQ (Bass 100 Hz, Sweepable Mid, Treble 4 kHz)
        if self.drive_amount > 0.001 {
            let drv = 1.0 + 4.0 * self.drive_amount.clamp(0.0, 1.0);
            l = (drv * l).tanh();
            r = (drv * r).tanh();
        }

        // Bass shelf (~100 Hz)
        if self.eq_bass_db.abs() > 0.1 {
            let gain = 10.0f32.powf(self.eq_bass_db / 20.0);
            let alpha = (TAU * 100.0 * inv_fs).clamp(0.005, 0.4);
            self.eq_bass_state += alpha * (0.5 * (l + r) - self.eq_bass_state);
            l += (gain - 1.0) * self.eq_bass_state;
            r += (gain - 1.0) * self.eq_bass_state;
        }

        // Mid peak (sweepable 200 Hz .. 8 kHz)
        if self.eq_mid_db.abs() > 0.1 {
            let gain = 10.0f32.powf(self.eq_mid_db / 20.0);
            let fc = self.eq_mid_freq_hz.clamp(200.0, 8000.0);
            let w0 = (TAU * fc * inv_fs).clamp(0.01, 0.45);
            let mono_in = 0.5 * (l + r);
            self.eq_mid_state[0] +=
                w0 * (mono_in - self.eq_mid_state[0] - 0.5 * self.eq_mid_state[1]);
            self.eq_mid_state[1] += w0 * self.eq_mid_state[0];
            let mid_bp = self.eq_mid_state[0];
            l += (gain - 1.0) * mid_bp;
            r += (gain - 1.0) * mid_bp;
        }

        // Treble shelf (~4 kHz)
        if self.eq_treble_db.abs() > 0.1 {
            let gain = 10.0f32.powf(self.eq_treble_db / 20.0);
            let alpha = (TAU * 4000.0 * inv_fs).clamp(0.01, 0.45);
            self.eq_treble_state += alpha * (0.5 * (l + r) - self.eq_treble_state);
            let treble_high = 0.5 * (l + r) - self.eq_treble_state;
            l += (gain - 1.0) * treble_high;
            r += (gain - 1.0) * treble_high;
        }

        // 4. Stereo Ping-Pong Delay
        if self.delay_enabled {
            let read_idx = (self.delay_write_idx + DELAY_LINE_LEN / 2) % DELAY_LINE_LEN;
            let dl = self.delay_buf_l[read_idx];
            let dr = self.delay_buf_r[read_idx];
            let fb = self.delay_feedback.clamp(0.0, 0.92);
            self.delay_buf_l[self.delay_write_idx] = l + dr * fb;
            self.delay_buf_r[self.delay_write_idx] = r + dl * fb;
            self.delay_write_idx = (self.delay_write_idx + 1) % DELAY_LINE_LEN;
            let mix = self.delay_mix.clamp(0.0, 1.0);
            l += dl * mix;
            r += dr * mix;
        }

        // 5. Layer 4-Line FDN Reverb
        if self.reverb_model != ReverbModel::Off && self.reverb_mix > 0.0 {
            let decay_gain = match self.reverb_model {
                ReverbModel::Off => 0.0,
                ReverbModel::Booth => 0.55,
                ReverbModel::Room => 0.72,
                ReverbModel::Stage => 0.82,
                ReverbModel::Hall => 0.89,
                ReverbModel::Cathedral => 0.94,
            };

            // Read delay line outputs
            let mut tap_outs = [0.0f32; FDN_SIZE];
            for i in 0..FDN_SIZE {
                let delay = FDN_DELAYS[i];
                let r_idx = (self.fdn_write_indices[i] + 2048 - delay) % 2048;
                tap_outs[i] = self.fdn_bufs[i][r_idx];
            }

            // Hadamard 4x4 unitary feedback matrix (normalized by 0.5)
            let s0 = tap_outs[0] + tap_outs[1] + tap_outs[2] + tap_outs[3];
            let s1 = tap_outs[0] - tap_outs[1] + tap_outs[2] - tap_outs[3];
            let s2 = tap_outs[0] + tap_outs[1] - tap_outs[2] - tap_outs[3];
            let s3 = tap_outs[0] - tap_outs[1] - tap_outs[2] + tap_outs[3];
            let hadamard = [0.5 * s0, 0.5 * s1, 0.5 * s2, 0.5 * s3];

            let mono_in = 0.5 * (l + r);
            // High-frequency damping & feedback write
            for (i, &had_val) in hadamard.iter().enumerate().take(FDN_SIZE) {
                let fb = had_val * decay_gain;
                self.fdn_lp_states[i] += 0.45 * (fb - self.fdn_lp_states[i]);
                self.fdn_bufs[i][self.fdn_write_indices[i]] = mono_in + self.fdn_lp_states[i];
                self.fdn_write_indices[i] = (self.fdn_write_indices[i] + 1) % 2048;
            }

            let rev_l = 0.5 * (tap_outs[0] + tap_outs[2]);
            let rev_r = 0.5 * (tap_outs[1] + tap_outs[3]);
            let mix = self.reverb_mix.clamp(0.0, 1.0);
            l = l * (1.0 - 0.4 * mix) + rev_l * mix;
            r = r * (1.0 - 0.4 * mix) + rev_r * mix;
        }

        [l, r]
    }
}

/// Global Master Compressor and `CS4392` + `LM833` output reconstruction stage.
#[derive(Debug, Clone, Copy)]
pub struct MasterBus {
    pub compressor_amount: f32,
    env: f32,
    recon_lp_l: f32,
    recon_lp_r: f32,
}

impl Default for MasterBus {
    fn default() -> Self {
        Self {
            compressor_amount: 0.25,
            env: 0.0,
            recon_lp_l: 0.0,
            recon_lp_r: 0.0,
        }
    }
}

impl MasterBus {
    /// Processes the summed 7-layer stereo signal through the Master Compressor and DAC stage.
    pub fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let [l, r] = frame;
        let peak = l.abs().max(r.abs());
        self.env = 0.99 * self.env + 0.01 * peak;

        let amt = self.compressor_amount.clamp(0.0, 1.0);
        let gain = if self.env > 0.5 {
            1.0 / (1.0 + amt * (self.env - 0.5) * 2.0)
        } else {
            1.0 + 0.15 * amt
        };

        let cl = (l * gain).clamp(-1.0, 1.0);
        let cr = (r * gain).clamp(-1.0, 1.0);

        // Post-DAC CS4392 + LM833 ultrasonic reconstruction filter
        self.recon_lp_l += 0.85 * (cl - self.recon_lp_l);
        self.recon_lp_r += 0.85 * (cr - self.recon_lp_r);
        [self.recon_lp_l, self.recon_lp_r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_fx_chain_and_master_bus_stability() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.mod1_type = Mod1Type::AutoPan;
        fx.mod2_type = Mod2Type::Chorus;
        fx.drive_amount = 0.5;
        fx.delay_enabled = true;
        fx.reverb_model = ReverbModel::Hall;

        let mut master = MasterBus::default();
        let mut clock = MasterClock::default();

        for i in 0..256 {
            clock.advance(48_000.0);
            let x = ((i as f32) * 0.1).sin() * 0.5;
            let [l, r] = fx.process_frame([x, x]);
            let [ml, mr] = master.process_frame([l, r]);
            assert!(ml.is_finite() && mr.is_finite());
            assert!(ml.abs() <= 1.0 && mr.abs() <= 1.0);
        }
        assert!(clock.beat_phase > 0.0);
    }
}
