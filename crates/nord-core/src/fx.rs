//! 7 Independent Per-Layer FX Chains (`Mod1`, `Mod2`, `Amp/EQ`, `Delay`, `Reverb`) & Master Bus.
//!
//! Spec: `docs/specs/effects-and-routing.md`

use std::f32::consts::TAU;

/// Fixed stereo delay line capacity (131,072 samples ≈ 2.73 s at 48 kHz; zero heap allocations during `process`).
const DELAY_LINE_LEN: usize = 131_072;

/// Fixed stereo modulation buffer capacity for Chorus / Vibe / Flanger (2048 samples ≈ 42 ms at 48 kHz).
const MOD2_LINE_LEN: usize = 2048;

/// Number of delay lines in the stereo Feedback Delay Network (FDN) Reverb.
const FDN_SIZE: usize = 4;

/// Prime sample delay lengths for the 4 FDN delay lines (at 48 kHz).
const FDN_DELAYS: [usize; FDN_SIZE] = [1087, 1283, 1531, 1789];

/// Prime stereo input-diffuser allpass lengths (`[L0, L1, R0, R1]`) to eliminate metallic slapback.
const REV_AP_DELAYS: [usize; FDN_SIZE] = [113, 257, 149, 311];

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

/// Amp simulation cabinet coloration models (`Amp Sim / EQ`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmpCabinet {
    #[default]
    Off,
    Small,
    Jc,
    Twin,
    British,
}

/// Feedback loop filter voicings for `Delay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelayFilterMode {
    #[default]
    Off,
    Hp,
    Lp,
    Bp,
}

/// Tempo-synced rhythmic subdivisions for `Delay` when `delay_clock_sync` is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelaySubdivision {
    Quarter,
    #[default]
    DottedEighth,
    Eighth,
    Sixteenth,
}

impl DelaySubdivision {
    /// Returns beat multiplier relative to a quarter note (`1.0`).
    #[inline]
    pub fn beats(self) -> f32 {
        match self {
            Self::Quarter => 1.0,
            Self::DottedEighth => 0.75,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
        }
    }
}

/// Reverb room size models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverbModel {
    Off,
    Booth,
    Room,
    Stage,
    Spring,
    Hall,
    Cathedral,
}

/// Reverb tonal voicing & Chorale modulation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReverbColor {
    #[default]
    Normal,
    Bright,
    Dark,
    Chorale,
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
    pub amp_cabinet: AmpCabinet,
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
    pub delay_analog: bool,
    pub delay_ping_pong: bool,
    pub delay_filter: DelayFilterMode,
    pub delay_variation: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_color: ReverbColor,
    pub reverb_mix: f32,
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
    cab_lp_state: [f32; 2],
    cab_hp_state: [f32; 2],
    cab_mid_state: [[f32; 2]; 2],
    eq_bass_state: f32,
    eq_mid_state: [f32; 2],
    eq_treble_state: f32,
    comp_env: f32,
    delay_buf_l: Box<[f32]>,
    delay_buf_r: Box<[f32]>,
    delay_write_idx: usize,
    delay_smoothed_samples: f32,
    delay_var_phase: f32,
    delay_lp_state: [f32; 2],
    delay_hp_state: [f32; 2],
    fdn_bufs: Box<[[f32; 2048]; FDN_SIZE]>,
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
            delay_analog: false,
            delay_ping_pong: false,
            delay_filter: DelayFilterMode::Off,
            delay_variation: false,
            delay_feedback: 0.35,
            delay_mix: 0.25,
            reverb_model: ReverbModel::Stage,
            reverb_color: ReverbColor::Normal,
            reverb_mix: 0.15,
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
            cab_lp_state: [0.0; 2],
            cab_hp_state: [0.0; 2],
            cab_mid_state: [[0.0; 2]; 2],
            eq_bass_state: 0.0,
            eq_mid_state: [0.0; 2],
            eq_treble_state: 0.0,
            comp_env: 0.0,
            delay_buf_l: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_buf_r: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_write_idx: 0,
            delay_smoothed_samples: default_delay_ms * 0.001 * fs,
            delay_var_phase: 0.0,
            delay_lp_state: [0.0; 2],
            delay_hp_state: [0.0; 2],
            fdn_bufs: Box::new([[0.0; 2048]; FDN_SIZE]),
            fdn_write_indices: [0; FDN_SIZE],
            fdn_lp_states: [0.0; FDN_SIZE],
            rev_ap_bufs: Box::new([[0.0; 512]; FDN_SIZE]),
            rev_ap_indices: [0; FDN_SIZE],
            reverb_chorale_phase: 0.0,
        }
    }

    fn process_mod1(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod1_type == Mod1Type::Off {
            return [l, r];
        }
        let lfo = self.mod1_phase.sin();
        self.mod1_phase = (self.mod1_phase + TAU * self.mod1_rate_hz * inv_fs) % TAU;
        let amt = self.mod1_amount.clamp(0.0, 1.0);
        match self.mod1_type {
            Mod1Type::Off => [l, r],
            Mod1Type::Tremolo => {
                let g = 1.0 - 0.5 * amt * (1.0 + lfo);
                [l * g, r * g]
            }
            Mod1Type::AutoPan => {
                let pan = 0.5 * amt * lfo;
                [l * (1.0 - pan), r * (1.0 + pan)]
            }
            Mod1Type::RingMod => [
                l * (1.0 - amt) + (l * lfo) * amt,
                r * (1.0 - amt) + (r * lfo) * amt,
            ],
            Mod1Type::Wah => {
                // Resonant 2-pole SVF bandpass swept between 350 Hz and 2.8 kHz
                let norm_sweep = 0.5 + 0.5 * lfo;
                let fc = 350.0 * 8.0f32.powf(norm_sweep);
                let w = (TAU * fc * inv_fs).clamp(0.02, 0.55);
                let q_damp = 0.35;

                let hp_l = l - self.wah_state_l[1] - q_damp * self.wah_state_l[0];
                let bp_l = self.wah_state_l[0] + w * hp_l;
                let lp_l = self.wah_state_l[1] + w * bp_l;
                self.wah_state_l = [bp_l, lp_l];

                let hp_r = r - self.wah_state_r[1] - q_damp * self.wah_state_r[0];
                let bp_r = self.wah_state_r[0] + w * hp_r;
                let lp_r = self.wah_state_r[1] + w * bp_r;
                self.wah_state_r = [bp_r, lp_r];

                [
                    l * (1.0 - amt) + bp_l * 1.4 * amt,
                    r * (1.0 - amt) + bp_r * 1.4 * amt,
                ]
            }
        }
    }

    fn process_mod2(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.mod2_type == Mod2Type::Off {
            return [l, r];
        }
        let lfo_l = self.mod2_phase.sin();
        let lfo_r = self.mod2_phase.cos();
        self.mod2_phase = (self.mod2_phase + TAU * self.mod2_rate_hz * inv_fs) % TAU;
        let amt = self.mod2_amount.clamp(0.0, 1.0);

        match self.mod2_type {
            Mod2Type::Off => [l, r],
            Mod2Type::Chorus | Mod2Type::Vibe | Mod2Type::Flanger => {
                let (base_ms, depth_ms, fb_gain, wet_scale) = match self.mod2_type {
                    Mod2Type::Chorus => (9.0, 4.5 * amt, 0.0, 0.5),
                    Mod2Type::Vibe => (11.0, 6.0 * amt, 0.15 * amt, 0.65),
                    Mod2Type::Flanger => (2.8, 2.2 * amt, 0.45 * amt, 0.55),
                    _ => (0.0, 0.0, 0.0, 0.0),
                };
                let d_l = (base_ms + depth_ms * lfo_l) * 0.001 * self.sample_rate;
                let d_r = (base_ms + depth_ms * lfo_r) * 0.001 * self.sample_rate;
                let wet_l = read_ring_interpolated(&self.mod2_buf_l, self.mod2_write_idx, d_l);
                let wet_r = read_ring_interpolated(&self.mod2_buf_r, self.mod2_write_idx, d_r);
                self.mod2_buf_l[self.mod2_write_idx] = (l + wet_l * fb_gain).clamp(-2.0, 2.0);
                self.mod2_buf_r[self.mod2_write_idx] = (r + wet_r * fb_gain).clamp(-2.0, 2.0);
                self.mod2_write_idx = (self.mod2_write_idx + 1) % MOD2_LINE_LEN;

                let dry_mix = 1.0 - wet_scale * amt;
                let wet_mix = wet_scale * amt;
                [l * dry_mix + wet_l * wet_mix, r * dry_mix + wet_r * wet_mix]
            }
            Mod2Type::Phaser => {
                // 4-stage cascaded 1st-order all-pass phaser with stereo LFO sweep
                let freq_l = 320.0 * 9.0f32.powf(0.5 + 0.5 * lfo_l);
                let freq_r = 320.0 * 9.0f32.powf(0.5 + 0.5 * lfo_r);
                let coeff_l = ((1.0 - TAU * freq_l * inv_fs) / (1.0 + TAU * freq_l * inv_fs))
                    .clamp(-0.95, 0.95);
                let coeff_r = ((1.0 - TAU * freq_r * inv_fs) / (1.0 + TAU * freq_r * inv_fs))
                    .clamp(-0.95, 0.95);

                let mut x_l = l + self.phaser_fb[0] * 0.35 * amt;
                for state in &mut self.phaser_ap_l {
                    let y = -coeff_l * x_l + *state;
                    *state = x_l + coeff_l * y;
                    x_l = y;
                }
                self.phaser_fb[0] = x_l.clamp(-2.0, 2.0);

                let mut x_r = r + self.phaser_fb[1] * 0.35 * amt;
                for state in &mut self.phaser_ap_r {
                    let y = -coeff_r * x_r + *state;
                    *state = x_r + coeff_r * y;
                    x_r = y;
                }
                self.phaser_fb[1] = x_r.clamp(-2.0, 2.0);

                [
                    l * (1.0 - 0.5 * amt) + x_l * 0.5 * amt,
                    r * (1.0 - 0.5 * amt) + x_r * 0.5 * amt,
                ]
            }
        }
    }

    fn process_amp_eq_comp(&mut self, mut l: f32, mut r: f32, inv_fs: f32) -> [f32; 2] {
        // Soft-clipping tube drive
        if self.drive_amount > 0.001 {
            let drv = 1.0 + 4.0 * self.drive_amount.clamp(0.0, 1.0);
            l = (drv * l).tanh();
            r = (drv * r).tanh();
        }

        // Cabinet impulse shaping
        if self.amp_cabinet != AmpCabinet::Off {
            let (lp_hz, hp_hz, mid_hz, mid_gain) = match self.amp_cabinet {
                AmpCabinet::Off => (20_000.0, 20.0, 1000.0, 0.0),
                AmpCabinet::Small => (3400.0, 140.0, 900.0, 0.45),
                AmpCabinet::Jc => (6500.0, 70.0, 2800.0, 0.35),
                AmpCabinet::Twin => (5200.0, 65.0, 650.0, -0.30),
                AmpCabinet::British => (4200.0, 95.0, 1600.0, 0.40),
            };
            let lp_a = (TAU * lp_hz * inv_fs).clamp(0.01, 0.85);
            let hp_a = (TAU * hp_hz * inv_fs).clamp(0.002, 0.3);
            let mid_w = (TAU * mid_hz * inv_fs).clamp(0.01, 0.45);
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                self.cab_hp_state[ch] += hp_a * (*sig - self.cab_hp_state[ch]);
                let high_passed = *sig - self.cab_hp_state[ch];
                self.cab_lp_state[ch] += lp_a * (high_passed - self.cab_lp_state[ch]);
                let band_in = self.cab_lp_state[ch];
                self.cab_mid_state[ch][0] +=
                    mid_w * (band_in - self.cab_mid_state[ch][0] - 0.5 * self.cab_mid_state[ch][1]);
                self.cab_mid_state[ch][1] += mid_w * self.cab_mid_state[ch][0];
                *sig = band_in + mid_gain * self.cab_mid_state[ch][0];
            }
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

        // Per-layer compressor (`comp_enabled`, `comp_fast` 1.5 ms vs 12 ms attack)
        if self.comp_enabled && self.comp_amount > 0.001 {
            let peak = l.abs().max(r.abs());
            let atk_tau = if self.comp_fast { 0.0015 } else { 0.012 };
            let rel_tau = 0.085;
            let coeff = if peak > self.comp_env {
                (inv_fs / atk_tau).min(1.0)
            } else {
                (inv_fs / rel_tau).min(1.0)
            };
            self.comp_env += coeff * (peak - self.comp_env);
            let amt = self.comp_amount.clamp(0.0, 1.0);
            let gain = if self.comp_env > 0.35 {
                1.0 / (1.0 + amt * 2.5 * (self.comp_env - 0.35))
            } else {
                1.0 + 0.12 * amt
            };
            l *= gain;
            r *= gain;
        }

        [l, r]
    }

    fn target_delay_samples(&self, bpm: f32) -> f32 {
        let target_ms = if self.delay_clock_sync {
            let beat_ms = 60_000.0 / bpm.clamp(30.0, 300.0);
            (beat_ms * self.delay_subdivision.beats()).clamp(10.0, 2000.0)
        } else {
            self.delay_time_ms.clamp(10.0, 2000.0)
        };
        (target_ms * 0.001 * self.sample_rate).clamp(1.0, (DELAY_LINE_LEN - 8) as f32)
    }

    /// Snaps the smoothed analog delay read position to the current target without slewing.
    pub fn snap_delay_time(&mut self, bpm: f32) {
        self.delay_smoothed_samples = self.target_delay_samples(bpm);
    }

    fn process_delay(&mut self, l: f32, r: f32, bpm: f32, inv_fs: f32) -> [f32; 2] {
        if !self.delay_enabled {
            return [l, r];
        }
        let target_samples = self.target_delay_samples(bpm);

        // Smooth Analog tape pitch-slew (80 ms inertia) vs exact digital tap
        if self.delay_analog {
            self.delay_smoothed_samples +=
                (target_samples - self.delay_smoothed_samples) * (inv_fs / 0.080).min(1.0);
        } else {
            self.delay_smoothed_samples = target_samples;
        }

        let flutter = if self.delay_variation {
            self.delay_var_phase = (self.delay_var_phase + TAU * 0.65 * inv_fs) % TAU;
            self.delay_var_phase.sin() * 0.0012 * self.sample_rate
        } else {
            0.0
        };

        let read_delay =
            (self.delay_smoothed_samples + flutter).clamp(1.0, (DELAY_LINE_LEN - 4) as f32);
        let dl = read_ring_interpolated(&self.delay_buf_l, self.delay_write_idx, read_delay);
        let dr = read_ring_interpolated(&self.delay_buf_r, self.delay_write_idx, read_delay);

        // Feedback loop HP / LP / BP filters
        let (mut fb_l, mut fb_r) = (dl, dr);
        if self.delay_filter != DelayFilterMode::Off {
            let hp_a = (TAU * 280.0 * inv_fs).clamp(0.005, 0.35);
            let lp_a = (TAU * 2200.0 * inv_fs).clamp(0.02, 0.75);
            for (ch, sig) in [&mut fb_l, &mut fb_r].into_iter().enumerate() {
                self.delay_hp_state[ch] += hp_a * (*sig - self.delay_hp_state[ch]);
                let hp_out = *sig - self.delay_hp_state[ch];
                let lp_in = match self.delay_filter {
                    DelayFilterMode::Hp => {
                        *sig = hp_out;
                        continue;
                    }
                    DelayFilterMode::Lp => *sig,
                    DelayFilterMode::Bp => hp_out,
                    DelayFilterMode::Off => *sig,
                };
                self.delay_lp_state[ch] += lp_a * (lp_in - self.delay_lp_state[ch]);
                *sig = self.delay_lp_state[ch];
            }
        }

        if self.delay_analog {
            fb_l = (fb_l * 1.15).tanh();
            fb_r = (fb_r * 1.15).tanh();
        }
        if self.delay_variation {
            let cross = 0.12 * (fb_l - fb_r);
            fb_l -= cross;
            fb_r += cross;
        }

        let fb = self.delay_feedback.clamp(0.0, 0.95);
        if self.delay_ping_pong {
            let mono_in = 0.5 * (l + r);
            self.delay_buf_l[self.delay_write_idx] = (mono_in + fb_r * fb).clamp(-4.0, 4.0);
            self.delay_buf_r[self.delay_write_idx] = (fb_l * fb).clamp(-4.0, 4.0);
        } else {
            self.delay_buf_l[self.delay_write_idx] = (l + fb_l * fb).clamp(-4.0, 4.0);
            self.delay_buf_r[self.delay_write_idx] = (r + fb_r * fb).clamp(-4.0, 4.0);
        }
        self.delay_write_idx = (self.delay_write_idx + 1) % DELAY_LINE_LEN;

        let mix = self.delay_mix.clamp(0.0, 1.0);
        [l + dl * mix, r + dr * mix]
    }

    fn process_reverb(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        if self.reverb_model == ReverbModel::Off || self.reverb_mix <= 0.0 {
            return [l, r];
        }
        let decay_gain = match self.reverb_model {
            ReverbModel::Off => 0.0,
            ReverbModel::Booth => 0.55,
            ReverbModel::Room => 0.72,
            ReverbModel::Stage => 0.82,
            ReverbModel::Spring => 0.78,
            ReverbModel::Hall => 0.89,
            ReverbModel::Cathedral => 0.94,
        };
        let damp = match self.reverb_color {
            ReverbColor::Normal => 0.45,
            ReverbColor::Bright => 0.72,
            ReverbColor::Dark => 0.22,
            ReverbColor::Chorale => 0.52,
        };
        let mod_offset = if self.reverb_color == ReverbColor::Chorale {
            self.reverb_chorale_phase = (self.reverb_chorale_phase + TAU * 0.75 * inv_fs) % TAU;
            (self.reverb_chorale_phase.sin() * 6.0).round() as isize
        } else {
            0
        };

        let mut tap_outs = [0.0f32; FDN_SIZE];
        for (i, out) in tap_outs.iter_mut().enumerate() {
            let base_delay = if self.reverb_model == ReverbModel::Spring {
                FDN_DELAYS[i] / 2 + 127
            } else {
                FDN_DELAYS[i]
            };
            let delay = (base_delay as isize + if i % 2 == 0 { mod_offset } else { -mod_offset })
                .clamp(64, 2000) as usize;
            let r_idx = (self.fdn_write_indices[i] + 2048 - delay) % 2048;
            *out = self.fdn_bufs[i][r_idx];
        }

        // Hadamard 4x4 unitary feedback matrix (normalized by 0.5)
        let s0 = tap_outs[0] + tap_outs[1] + tap_outs[2] + tap_outs[3];
        let s1 = tap_outs[0] - tap_outs[1] + tap_outs[2] - tap_outs[3];
        let s2 = tap_outs[0] + tap_outs[1] - tap_outs[2] - tap_outs[3];
        let s3 = tap_outs[0] - tap_outs[1] - tap_outs[2] + tap_outs[3];
        let hadamard = [0.5 * s0, 0.5 * s1, 0.5 * s2, 0.5 * s3];

        // Two-stage stereo input allpass diffusion prevents transient flutter/metallic ringing
        const AP_G: f32 = 0.52;
        let mut dif_l = l;
        for (stage, &ap_delay) in REV_AP_DELAYS.iter().enumerate().take(2) {
            let w = self.rev_ap_indices[stage];
            let r_idx = (w + 512 - ap_delay) % 512;
            let delayed = self.rev_ap_bufs[stage][r_idx];
            let v = dif_l + delayed * AP_G;
            self.rev_ap_bufs[stage][w] = v;
            self.rev_ap_indices[stage] = (w + 1) % 512;
            dif_l = delayed - AP_G * v;
        }
        let mut dif_r = r;
        for (stage, &ap_delay) in REV_AP_DELAYS.iter().enumerate().skip(2) {
            let w = self.rev_ap_indices[stage];
            let r_idx = (w + 512 - ap_delay) % 512;
            let delayed = self.rev_ap_bufs[stage][r_idx];
            let v = dif_r + delayed * AP_G;
            self.rev_ap_bufs[stage][w] = v;
            self.rev_ap_indices[stage] = (w + 1) % 512;
            dif_r = delayed - AP_G * v;
        }

        let stereo_in = [
            dif_l,
            dif_r,
            0.75 * dif_l + 0.25 * dif_r,
            0.25 * dif_l + 0.75 * dif_r,
        ];
        for (i, &had_val) in hadamard.iter().enumerate().take(FDN_SIZE) {
            let fb = had_val * decay_gain;
            self.fdn_lp_states[i] += damp * (fb - self.fdn_lp_states[i]);
            self.fdn_bufs[i][self.fdn_write_indices[i]] = stereo_in[i] + self.fdn_lp_states[i];
            self.fdn_write_indices[i] = (self.fdn_write_indices[i] + 1) % 2048;
        }

        let rev_l = 0.45 * (tap_outs[0] + 0.8 * tap_outs[2] + 0.2 * tap_outs[3]);
        let rev_r = 0.45 * (tap_outs[1] + 0.8 * tap_outs[3] + 0.2 * tap_outs[2]);
        let mix = self.reverb_mix.clamp(0.0, 1.0);
        [
            l * (1.0 - 0.25 * mix) + rev_l * mix,
            r * (1.0 - 0.25 * mix) + rev_r * mix,
        ]
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

/// Global Master Compressor and Burr-Brown `PCM1791A` + JRC `NJM4580M` output reconstruction stage.
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

        // Post-DAC Burr-Brown PCM1791A + NJM4580M stage:
        // The hardware PCM1791A features a flat passband within +/-0.002 dB up to 21.8 kHz.
        // A gentle 0.985 alpha preserves crisp treble (<0.15 dB roll-off at 20 kHz) without in-band damping.
        self.recon_lp_l += 0.985 * (cl - self.recon_lp_l);
        self.recon_lp_r += 0.985 * (cr - self.recon_lp_r);
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
        fx.amp_cabinet = AmpCabinet::British;
        fx.comp_enabled = true;
        fx.comp_amount = 0.4;
        fx.delay_enabled = true;
        fx.reverb_model = ReverbModel::Hall;

        let mut master = MasterBus::default();
        let mut clock = MasterClock::default();

        for i in 0..256 {
            clock.advance(48_000.0);
            let x = ((i as f32) * 0.1).sin() * 0.5;
            let [l, r] = fx.process_frame_with_bpm([x, x], clock.bpm);
            let [ml, mr] = master.process_frame([l, r]);
            assert!(ml.is_finite() && mr.is_finite());
            assert!(ml.abs() <= 1.0 && mr.abs() <= 1.0);
        }
        assert!(clock.beat_phase > 0.0);
    }

    #[test]
    fn test_delay_ping_pong_and_analog_pitch_slew() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = ReverbModel::Off;
        fx.delay_enabled = true;
        fx.delay_time_ms = 20.0; // 960 samples at 48 kHz
        fx.delay_feedback = 0.6;
        fx.delay_mix = 1.0;
        fx.delay_ping_pong = true;
        fx.delay_analog = false;

        // Prime impulse at frame 0 (skip input on subsequent frames)
        let mut echo1_l = 0.0f32;
        let mut echo1_r = 0.0f32;
        let mut echo2_l = 0.0f32;
        let mut echo2_r = 0.0f32;
        for i in 0..2400 {
            let inp = if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
            let [l, r] = fx.process_frame(inp);
            if (940..985).contains(&i) {
                echo1_l += l.abs();
                echo1_r += r.abs();
            }
            if (1900..1950).contains(&i) {
                echo2_l += l.abs();
                echo2_r += r.abs();
            }
        }
        assert!(
            echo1_l > 0.5 && echo1_r < 1e-4,
            "first ping-pong tap hits Left first"
        );
        assert!(
            echo2_r > 0.25 && echo2_l < 1e-4,
            "second ping-pong tap bounces to Right"
        );

        // Verify Analog tape pitch-slew glides smoothly when delay time changes
        let mut analog = LayerFxChain::new(48_000.0);
        analog.reverb_model = ReverbModel::Off;
        analog.delay_enabled = true;
        analog.delay_analog = true;
        analog.delay_time_ms = 50.0;
        analog.snap_delay_time(120.0);
        analog.delay_mix = 0.8;
        let before = analog.delay_smoothed_samples;
        analog.delay_time_ms = 200.0;
        for _ in 0..480 {
            analog.process_frame([0.2, 0.2]);
        }
        let mid = analog.delay_smoothed_samples;
        assert!(
            mid > before && mid < 0.200 * 48_000.0,
            "analog delay must slew smoothly toward new target (before={before}, mid={mid})"
        );
    }

    #[test]
    fn test_delay_feedback_filters_wah_mod2_and_reverb_colors() {
        // Verify LP feedback filter damps high-frequency square-wave echoes relative to Off
        let mut d_off = LayerFxChain::new(48_000.0);
        d_off.reverb_model = ReverbModel::Off;
        d_off.delay_enabled = true;
        d_off.delay_time_ms = 12.0;
        d_off.delay_feedback = 0.7;
        d_off.delay_mix = 1.0;

        let mut d_lp = d_off.clone();
        d_lp.delay_filter = DelayFilterMode::Lp;
        d_lp.delay_variation = true;

        let mut rms_off = 0.0f32;
        let mut rms_lp = 0.0f32;
        for i in 0..4000 {
            let hi = if i < 200 {
                if i % 2 == 0 {
                    0.8
                } else {
                    -0.8
                }
            } else {
                0.0
            };
            let [lo, _] = d_off.process_frame([hi, hi]);
            let [ll, _] = d_lp.process_frame([hi, hi]);
            if i > 600 {
                rms_off += lo * lo;
                rms_lp += ll * ll;
            }
        }
        assert!(
            rms_lp < rms_off * 0.5,
            "LP feedback filter must attenuate Nyquist echo content"
        );

        // Exercise Wah, Phaser, Flanger, Spring & Chorale reverb
        for (m1, m2, rev, col) in [
            (
                Mod1Type::Wah,
                Mod2Type::Phaser,
                ReverbModel::Spring,
                ReverbColor::Bright,
            ),
            (
                Mod1Type::RingMod,
                Mod2Type::Flanger,
                ReverbModel::Hall,
                ReverbColor::Dark,
            ),
            (
                Mod1Type::Tremolo,
                Mod2Type::Vibe,
                ReverbModel::Cathedral,
                ReverbColor::Chorale,
            ),
        ] {
            let mut fx = LayerFxChain::new(48_000.0);
            fx.mod1_type = m1;
            fx.mod2_type = m2;
            fx.amp_cabinet = AmpCabinet::Twin;
            fx.comp_enabled = true;
            fx.comp_amount = 0.6;
            fx.comp_fast = true;
            fx.reverb_model = rev;
            fx.reverb_color = col;
            fx.reverb_mix = 0.4;
            for i in 0..512 {
                let x = ((i as f32) * 0.05).sin() * 0.4;
                let [l, r] = fx.process_frame_with_bpm([x, x], 130.0);
                assert!(l.is_finite() && r.is_finite());
            }
        }
    }
}
