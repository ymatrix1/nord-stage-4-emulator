//! 7 Independent Per-Layer FX Chains (`Mod1`, `Mod2`, `Amp/EQ`, `Delay`, `Reverb`) & Master Bus.
//!
//! Spec: `docs/specs/effects-and-routing.md`

use std::f32::consts::TAU;

/// Fixed stereo delay line capacity (4096 samples, zero heap allocations during `process`).
const DELAY_LINE_LEN: usize = 4096;

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
    pub eq_treble_db: f32,
    pub delay_enabled: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_mix: f32,
    mod1_phase: f32,
    mod2_phase: f32,
    delay_buf_l: [f32; DELAY_LINE_LEN],
    delay_buf_r: [f32; DELAY_LINE_LEN],
    delay_write_idx: usize,
    reverb_state_l: f32,
    reverb_state_r: f32,
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
            eq_treble_db: 0.0,
            delay_enabled: false,
            delay_feedback: 0.35,
            delay_mix: 0.25,
            reverb_model: ReverbModel::Stage,
            reverb_mix: 0.15,
            mod1_phase: 0.0,
            mod2_phase: 0.0,
            delay_buf_l: [0.0; DELAY_LINE_LEN],
            delay_buf_r: [0.0; DELAY_LINE_LEN],
            delay_write_idx: 0,
            reverb_state_l: 0.0,
            reverb_state_r: 0.0,
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

        // 3. Amp Drive
        if self.drive_amount > 0.001 {
            let drv = 1.0 + 4.0 * self.drive_amount.clamp(0.0, 1.0);
            l = (drv * l).tanh();
            r = (drv * r).tanh();
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

        // 5. Layer Reverb
        if self.reverb_model != ReverbModel::Off && self.reverb_mix > 0.0 {
            let decay = match self.reverb_model {
                ReverbModel::Off => 0.0,
                ReverbModel::Booth => 0.65,
                ReverbModel::Room => 0.78,
                ReverbModel::Stage => 0.85,
                ReverbModel::Hall => 0.91,
                ReverbModel::Cathedral => 0.95,
            };
            self.reverb_state_l = decay * self.reverb_state_l + (1.0 - decay) * l;
            self.reverb_state_r = decay * self.reverb_state_r + (1.0 - decay) * r;
            let mix = self.reverb_mix.clamp(0.0, 1.0);
            l = l * (1.0 - 0.3 * mix) + self.reverb_state_l * mix;
            r = r * (1.0 - 0.3 * mix) + self.reverb_state_r * mix;
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
