//! 3-Layer Nord Wave 2 Derived Synth Engine (`DSPS` — PolyBLEP VA + ZDF Ladder Filter + LFO + Arp).
//!
//! Spec: `docs/specs/synth-engine.md`

use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, OnceLock};

/// Maximum polyphonic voices per Synth layer. Pre-allocated with zero heap allocations during playback.
pub const MAX_SYNTH_VOICES: usize = 16;

/// Total number of single-cycle wavetables in the Nord Wave 2 derived bank (`0..=63`).
pub const NUM_WAVETABLES: usize = 64;

/// Number of octave mipmap levels per wavetable to prevent Nyquist aliasing.
const WAVETABLE_MIPS: usize = 8;

/// Single-cycle wavetable lookup resolution in samples.
const WAVETABLE_SIZE: usize = 256;

/// Maximum harmonic indices permitted at each mipmap level (2x oversampled guard).
const MIP_MAX_HARMONICS: [usize; WAVETABLE_MIPS] = [128, 64, 32, 16, 8, 4, 2, 1];

/// Classic FM harmonic and inharmonic carrier:modulator ratios.
pub const FM_RATIOS: [f32; 16] = [
    0.5,
    1.0,
    1.5,
    2.0,
    3.0,
    4.0,
    5.0,
    6.0,
    7.0,
    8.0,
    std::f32::consts::SQRT_2,
    std::f32::consts::E,
    PI,
    1.25,
    2.5,
    3.5,
];

/// Timbre brightness damping curve for `SAMPLES` oscillator playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SampleTimbre {
    #[default]
    Off,
    Soft,
    Bright,
}

/// Pre-loaded single-zone multisample waveform ready for lock-free real-time synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSynthSample {
    pub root_key: u8,
    pub key_low: u8,
    pub key_high: u8,
    pub sample_rate: f32,
    pub pcm: Arc<[f32]>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
    pub loop_crossfade: usize,
}

/// Pre-decoded playable multisample instrument for `SynthOscMode::Sample`.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthSampleSound {
    pub name: String,
    pub zones: Vec<LoadedSynthSample>,
}

impl SynthSampleSound {
    /// Creates a synthetic single-zone multisample fixture for deterministic rendering/tests.
    pub fn new_synthetic(
        name: &str,
        root_key: u8,
        pcm: &[f32],
        sample_rate: f32,
        loop_range: Option<(usize, usize)>,
        loop_crossfade: usize,
    ) -> Self {
        let (loop_start, loop_end) = match loop_range {
            Some((s, e)) if e > s && e <= pcm.len() => (Some(s), Some(e)),
            _ => (None, None),
        };
        Self {
            name: name.to_string(),
            zones: vec![LoadedSynthSample {
                root_key,
                key_low: 0,
                key_high: 127,
                sample_rate: sample_rate.max(8000.0),
                pcm: pcm.to_vec().into(),
                loop_start,
                loop_end,
                loop_crossfade,
            }],
        }
    }

    /// Creates a multi-zone sample sound from pre-decoded key/velocity zones.
    pub fn from_zones(name: &str, zones: Vec<LoadedSynthSample>) -> Self {
        Self {
            name: name.to_string(),
            zones,
        }
    }

    /// Resolves the best zone index for a given MIDI note (exact range first, nearest root fallback).
    fn zone_for_note(&self, note: u8) -> Option<usize> {
        if let Some(pos) = self
            .zones
            .iter()
            .position(|z| note >= z.key_low && note <= z.key_high && !z.pcm.is_empty())
        {
            return Some(pos);
        }
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| !z.pcm.is_empty())
            .min_by_key(|(_, z)| (i16::from(z.root_key) - i16::from(note)).unsigned_abs())
            .map(|(i, _)| i)
    }
}

/// Precomputed band-limited wavetable bank shared across all `SynthLayer` instances.
struct WavetableBank {
    tables: [[[f32; WAVETABLE_SIZE]; WAVETABLE_MIPS]; NUM_WAVETABLES],
}

static WAVETABLE_BANK: OnceLock<Box<WavetableBank>> = OnceLock::new();

fn wavetable_harmonic_amp(table_idx: usize, k: usize) -> (f32, f32) {
    let family = table_idx / 8;
    let variant = (table_idx % 8) as f32;
    let kf = k as f32;
    match family {
        // 0..7: Analog morphs (sine -> tri -> saw -> pulse/square)
        0 => {
            let tilt = 0.6 + 0.12 * variant;
            let amp = 1.0 / kf.powf(tilt);
            let odd_bias = if k % 2 == 1 {
                1.0
            } else {
                (variant / 7.0) * 0.95
            };
            (amp * odd_bias, 0.0)
        }
        // 8..15: Formant vocal peaks (A/E/I/O/U vowel formants)
        1 => {
            let f1 = 2.5 + variant * 0.85;
            let f2 = 7.0 + variant * 1.4;
            let g1 = (-0.5 * ((kf - f1) / 1.8).powi(2)).exp();
            let g2 = (-0.5 * ((kf - f2) / 2.8).powi(2)).exp();
            let amp = (0.35 / kf) + 0.9 * g1 + 0.55 * g2;
            (amp, 0.25)
        }
        // 16..23: Bell / metallic harmonic clusters
        2 => {
            let density = 1 + (variant as usize % 4);
            if k == 1 || k.is_multiple_of(density + 1) || k == 5 || k == 11 {
                (0.9 / kf.powf(0.55), 0.5 * (variant / 7.0))
            } else {
                (0.05 / kf, 0.0)
            }
        }
        // 24..31: Additive Organ / drawbar recipes
        3 => {
            let match_bar = matches!(k, 1 | 2 | 3 | 4 | 6 | 8 | 10 | 12 | 16);
            let weight = if match_bar {
                0.7 + 0.3 * ((kf + variant) * 0.7).sin().abs()
            } else {
                0.08 / kf
            };
            (weight / kf.powf(0.45), 0.0)
        }
        // 32..39: Hollow / Woodwind / Comb notch
        4 => {
            let notch = 2.0 + variant;
            let comb = ((PI * kf / notch).sin()).abs();
            let amp = (comb / kf.powf(0.85)) * if k % 2 == 1 { 1.0 } else { 0.35 };
            (amp, 0.0)
        }
        // 40..47: Brass / Reed bright edge
        5 => {
            let brightness = 0.55 + 0.07 * (7.0 - variant);
            let bump = (-0.5 * ((kf - (4.0 + variant)) / 2.2).powi(2)).exp();
            (1.0 / kf.powf(brightness) + 0.4 * bump, 0.15)
        }
        // 48..55: Resonant Sync-style harmonic sweep
        6 => {
            let center = 3.0 + variant * 2.2;
            let bw = 2.4 + 0.3 * variant;
            let env = (-0.5 * ((kf - center) / bw).powi(2)).exp();
            (0.25 / kf + 1.15 * env, 0.4)
        }
        // 56..63: Deep Bass / Rich fundamental + sub-harmonic weight
        _ => {
            let cutoff_harm = 3.0 + variant * 1.5;
            let lp = 1.0 / (1.0 + (kf / cutoff_harm).powi(2));
            let fund = if k == 1 { 1.5 } else { 1.0 };
            (fund * lp / kf.sqrt(), 0.0)
        }
    }
}

fn wavetable_bank() -> &'static WavetableBank {
    WAVETABLE_BANK.get_or_init(|| {
        let mut bank = Box::new(WavetableBank {
            tables: [[[0.0; WAVETABLE_SIZE]; WAVETABLE_MIPS]; NUM_WAVETABLES],
        });
        for (t_idx, mip_set) in bank.tables.iter_mut().enumerate() {
            let mut ref_norm = 1.0f32;
            for (mip_idx, &max_h) in MIP_MAX_HARMONICS.iter().enumerate() {
                let table = &mut mip_set[mip_idx];
                for (s_idx, sample) in table.iter_mut().enumerate() {
                    let phase = (s_idx as f32) / (WAVETABLE_SIZE as f32);
                    let mut sum = 0.0f32;
                    for k in 1..=max_h {
                        let (amp, phase_skew) = wavetable_harmonic_amp(t_idx, k);
                        // Lanczos sigma factor avoids Gibbs ringing on sharp truncations
                        let sigma = if max_h > 4 {
                            let x = PI * (k as f32) / ((max_h + 1) as f32);
                            x.sin() / x
                        } else {
                            1.0
                        };
                        sum += amp * sigma * (TAU * (k as f32) * phase + phase_skew).sin();
                    }
                    *sample = sum;
                }
                if mip_idx == 0 {
                    let peak = table.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
                    ref_norm = if peak > 1e-6 { 1.0 / peak } else { 1.0 };
                }
                for sample in table.iter_mut() {
                    *sample *= ref_norm;
                }
            }
        }
        bank
    })
}

/// Evaluates a band-limited mipmapped wavetable sample with position formant scan and interpolation.
#[inline]
fn sample_wavetable(
    bank: &WavetableBank,
    table_idx: u8,
    position: f32,
    phase: f32,
    dt: f32,
) -> f32 {
    let max_non_aliasing = (0.48 / dt.max(1e-5)).floor() as usize;
    let mip = MIP_MAX_HARMONICS
        .iter()
        .position(|&limit| limit <= max_non_aliasing)
        .unwrap_or(WAVETABLE_MIPS - 1);

    let t0 = (table_idx as usize) % NUM_WAVETABLES;
    let t1 = (t0 + 1) % NUM_WAVETABLES;
    let pos = position.clamp(0.0, 1.0);
    // Gentle phase warp provides continuous spectral formant motion even on a single table index
    let warped_phase = (phase + 0.16 * pos * (TAU * phase).sin()).rem_euclid(1.0);
    let idx_f = warped_phase * (WAVETABLE_SIZE as f32);
    let i0 = (idx_f.floor() as usize) % WAVETABLE_SIZE;
    let i1 = (i0 + 1) % WAVETABLE_SIZE;
    let frac = idx_f - idx_f.floor();

    let s0 = bank.tables[t0][mip][i0] * (1.0 - frac) + bank.tables[t0][mip][i1] * frac;
    let s1 = bank.tables[t1][mip][i0] * (1.0 - frac) + bank.tables[t1][mip][i1] * frac;
    (1.0 - pos) * s0 + pos * s1
}

/// Oscillator synthesis modes in the Nord Stage 4 Synth Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthOscMode {
    Analog,
    Wavetable,
    Fm,
    Sample,
}

/// Virtual Analog waveform shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaWaveform {
    Saw,
    SuperSaw,
    Pulse,
    Triangle,
    Sine,
}

/// Zero-Delay Feedback filter topologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterModel {
    Lp12,
    Lp24,
    Ladder24,
    BandPass,
    HighPass,
}

/// Low Frequency Oscillator (LFO) waveform shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LfoShape {
    #[default]
    Triangle,
    Saw,
    Square,
    SampleAndHold,
}

/// Destination parameter for LFO modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LfoDestination {
    #[default]
    FilterCutoff,
    OscPitch,
    OscCtrl,
}

/// Arpeggiator & Gate play patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArpMode {
    #[default]
    Off,
    Up,
    Down,
    UpDown,
    Random,
    Poly,
}

/// 2-point Polynomial Band-Limited Step (`PolyBLEP`) anti-aliasing residual.
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Topology-preserving Zero-Delay Feedback (`ZDF`) 4-pole Transistor Ladder & SVF filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZdfFilter {
    s: [f32; 4],
}

impl ZdfFilter {
    /// Processes one sample through the selected ZDF filter model.
    pub fn process(
        &mut self,
        input: f32,
        model: FilterModel,
        cutoff_hz: f32,
        resonance: f32,
        drive: f32,
        sample_rate: f32,
    ) -> f32 {
        let fc = cutoff_hz.clamp(20.0, 0.48 * sample_rate);
        let g = (PI * fc / sample_rate).tan();
        let g1 = g / (1.0 + g);

        match model {
            FilterModel::Ladder24 | FilterModel::Lp24 => {
                let k = (resonance.clamp(0.0, 1.0)) * 3.85;
                let big_g = g1 * g1 * g1 * g1;
                let s_sum = (g1 * g1 * g1) * (self.s[0] / (1.0 + g))
                    + (g1 * g1) * (self.s[1] / (1.0 + g))
                    + g1 * (self.s[2] / (1.0 + g))
                    + (self.s[3] / (1.0 + g));
                let driven = ((1.0 + 2.0 * drive) * input).tanh();
                let mut u = (driven - k * s_sum) / (1.0 + k * big_g);
                for st in &mut self.s {
                    let v = (u - *st) * g1;
                    let y = v + *st;
                    *st = y + v;
                    u = y;
                }
                u
            }
            FilterModel::Lp12 | FilterModel::BandPass | FilterModel::HighPass => {
                let r = (1.0 - 0.9 * resonance.clamp(0.0, 1.0)).max(0.08);
                let hp =
                    (input - (2.0 * r + g) * self.s[0] - self.s[1]) / (1.0 + 2.0 * r * g + g * g);
                let bp = g * hp + self.s[0];
                self.s[0] = g * hp + bp;
                let lp = g * bp + self.s[1];
                self.s[1] = g * bp + lp;
                match model {
                    FilterModel::Lp12 => lp,
                    FilterModel::BandPass => bp,
                    FilterModel::HighPass => hp,
                    _ => lp,
                }
            }
        }
    }
}

/// Roland/Nord 7-saw detune spread coefficients relative to fundamental.
const SUPERSAW_DETUNES: [f32; 7] = [
    -0.11002313,
    -0.06288439,
    -0.01952356,
    0.0,
    0.01991221,
    0.06216538,
    0.10745242,
];

/// Initial phase offsets for the 7 SuperSaw oscillators to prevent constructive crests.
const SUPERSAW_PHASE_INITS: [f32; 7] = [0.0, 0.24, 0.46, 0.68, 0.82, 0.15, 0.57];

/// Individual polyphonic synth voice.
#[derive(Debug, Clone, Copy)]
pub struct SynthVoice {
    pub active: bool,
    pub gate: bool,
    pub note: u8,
    pub velocity: f32,
    pub gain: f32,
    pub freq_hz: f32,
    pub phase: f32,
    pub mod_phase: f32,
    pub op3_phase: f32,
    pub mod_feedback_state: f32,
    pub mod_env: f32,
    pub supersaw_phases: [f32; 7],
    pub amp_env: f32,
    pub filter: ZdfFilter,
    pub sample_zone_idx: Option<usize>,
    pub sample_pos: f64,
    pub sample_step: f64,
    pub sample_lp_state: f32,
}

impl Default for SynthVoice {
    fn default() -> Self {
        Self {
            active: false,
            gate: false,
            note: 0,
            velocity: 0.8,
            gain: 1.0,
            freq_hz: 440.0,
            phase: 0.0,
            mod_phase: 0.0,
            op3_phase: 0.0,
            mod_feedback_state: 0.0,
            mod_env: 0.0,
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.0,
            filter: ZdfFilter::default(),
            sample_zone_idx: None,
            sample_pos: 0.0,
            sample_step: 1.0,
            sample_lp_state: 0.0,
        }
    }
}

/// Single 16-Voice Polyphonic Synth Layer (`Synth A`, `Synth B`, or `Synth C`) with LFO and Arpeggiator.
#[derive(Debug, Clone)]
pub struct SynthLayer {
    pub sample_rate: f32,
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    pub pulse_width: f32,
    pub fm_amount: f32,
    pub fm_ratio: f32,
    pub fm_feedback: f32,
    pub fm_sub_ratio: f32,
    pub fm_sub_amount: f32,
    pub fm_env_amount: f32,
    pub supersaw_detune: f32,
    pub wavetable_index: u8,
    pub wavetable_position: f32,
    pub sample_timbre: SampleTimbre,
    pub filter_model: FilterModel,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub filter_drive: f32,
    pub filter_env_amount: f32,
    pub attack_sec: f32,
    pub decay_sec: f32,
    pub sustain_level: f32,
    pub release_sec: f32,

    // LFO Modulation
    pub lfo_shape: LfoShape,
    pub lfo_dest: LfoDestination,
    pub lfo_rate_hz: f32,
    pub lfo_amount: f32,
    lfo_phase: f32,
    lfo_sh_val: f32,

    // Arpeggiator & Gate
    pub arp_mode: ArpMode,
    pub arp_octave_range: u8,
    pub arp_rate_bpm: f32,
    arp_step: usize,
    arp_sample_counter: f32,
    held_keys: [bool; 128],
    held_velocities: [u8; 128],
    held_gains: [f32; 128],
    arp_active_notes: [Option<u8>; MAX_SYNTH_VOICES],
    arp_rng: u32,

    // Pre-decoded multisample instrument (`SAMPLES` mode)
    sample_sound: Option<Arc<SynthSampleSound>>,

    // 16 Polyphonic Voices
    voices: [SynthVoice; MAX_SYNTH_VOICES],
}

impl SynthLayer {
    pub fn new(sample_rate: f32) -> Self {
        // Warm up the static mipmapped wavetable bank at construction time so render_frame never initializes it.
        let _ = wavetable_bank();
        Self {
            sample_rate: sample_rate.max(8000.0),
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::Saw,
            pulse_width: 0.5,
            fm_amount: 0.35,
            fm_ratio: 2.0,
            fm_feedback: 0.0,
            fm_sub_ratio: 3.0,
            fm_sub_amount: 0.0,
            fm_env_amount: 0.35,
            supersaw_detune: 0.3,
            wavetable_index: 0,
            wavetable_position: 0.0,
            sample_timbre: SampleTimbre::Off,
            filter_model: FilterModel::Ladder24,
            cutoff_hz: 2400.0,
            resonance: 0.3,
            filter_drive: 0.2,
            filter_env_amount: 0.0,
            attack_sec: 0.005,
            decay_sec: 0.3,
            sustain_level: 0.7,
            release_sec: 0.25,

            lfo_shape: LfoShape::Triangle,
            lfo_dest: LfoDestination::FilterCutoff,
            lfo_rate_hz: 3.5,
            lfo_amount: 0.0,
            lfo_phase: 0.0,
            lfo_sh_val: 0.0,

            arp_mode: ArpMode::Off,
            arp_octave_range: 1,
            arp_rate_bpm: 120.0,
            arp_step: 0,
            arp_sample_counter: 0.0,
            held_keys: [false; 128],
            held_velocities: [0; 128],
            held_gains: [1.0; 128],
            arp_active_notes: [None; MAX_SYNTH_VOICES],
            arp_rng: 0xA341_316C,

            sample_sound: None,

            voices: [SynthVoice::default(); MAX_SYNTH_VOICES],
        }
    }

    /// Loads or clears a pre-decoded multisample instrument for `SynthOscMode::Sample`.
    /// Call outside the audio thread; playback reads the shared immutable buffer without locking or allocating.
    pub fn load_sample(&mut self, sound: Option<Arc<SynthSampleSound>>) {
        self.sample_sound = sound;
    }

    /// Returns the name of the currently loaded multisample sound, if any.
    pub fn loaded_sample_name(&self) -> Option<&str> {
        self.sample_sound.as_ref().map(|s| s.name.as_str())
    }

    /// Triggers a note on across the polyphonic voice allocator or records in arpeggiator list.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on_with_gain(note, velocity, 1.0);
    }

    pub fn note_on_with_gain(&mut self, note: u8, velocity: u8, gain: f32) {
        let note_idx = (note as usize).min(127);
        self.held_keys[note_idx] = true;
        self.held_velocities[note_idx] = velocity.clamp(1, 127);
        self.held_gains[note_idx] = gain.clamp(0.0, 1.0);

        if self.arp_mode != ArpMode::Off {
            return;
        }

        self.trigger_voice(note, velocity, gain);
    }

    fn trigger_voice(&mut self, note: u8, velocity: u8, gain: f32) {
        let vel_norm = f32::from(velocity.clamp(1, 127)) / 127.0;
        let freq = 440.0 * 2.0f32.powf((f32::from(note) - 69.0) / 12.0);

        // Find existing note voice to re-trigger, or first inactive voice, or steal oldest/quietest
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or_else(|| {
                // Steal voice with lowest amp envelope
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.amp_env
                            .partial_cmp(&b.amp_env)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let (sample_zone_idx, sample_step) = if let Some(sound) = &self.sample_sound {
            if let Some(z_idx) = sound.zone_for_note(note) {
                let zone = &sound.zones[z_idx];
                let step = (f64::from(zone.sample_rate) / f64::from(self.sample_rate))
                    * 2.0f64.powf((f64::from(note) - f64::from(zone.root_key)) / 12.0);
                (Some(z_idx), step)
            } else {
                (None, 1.0)
            }
        } else {
            (None, 1.0)
        };

        self.voices[idx] = SynthVoice {
            active: true,
            gate: true,
            note,
            velocity: vel_norm,
            gain: gain.clamp(0.0, 1.0),
            freq_hz: freq,
            phase: 0.0,
            mod_phase: 0.0,
            op3_phase: 0.0,
            mod_feedback_state: 0.0,
            mod_env: 1.0,
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.01,
            filter: self.voices[idx].filter,
            sample_zone_idx,
            sample_pos: 0.0,
            sample_step,
            sample_lp_state: 0.0,
        };
    }

    /// Releases a held note across the polyphonic voices.
    pub fn note_off(&mut self, note: u8) {
        let note_idx = (note as usize).min(127);
        self.held_keys[note_idx] = false;

        if self.arp_mode != ArpMode::Off {
            return;
        }

        for v in &mut self.voices {
            if v.active && v.note == note {
                v.gate = false;
            }
        }
    }

    /// Advances the arpeggiator engine and generates notes when active.
    fn tick_arpeggiator(&mut self, inv_fs: f32) {
        if self.arp_mode == ArpMode::Off {
            return;
        }

        let mut held_list = [0u8; 128];
        let mut count = 0;
        for (n, &held) in self.held_keys.iter().enumerate() {
            if held {
                held_list[count] = n as u8;
                count += 1;
            }
        }

        if count == 0 {
            self.release_arp_notes();
            self.arp_step = 0;
            return;
        }

        // Arp speed: 16th note division at arp_rate_bpm
        let sixteenth_sec = 60.0 / (self.arp_rate_bpm.max(30.0) * 4.0);
        self.arp_sample_counter += inv_fs;

        if self.arp_sample_counter >= sixteenth_sec {
            self.arp_sample_counter -= sixteenth_sec;

            // Release previous arp note
            self.release_arp_notes();

            if self.arp_mode == ArpMode::Poly {
                for (slot, &note) in held_list
                    .iter()
                    .take(count.min(MAX_SYNTH_VOICES))
                    .enumerate()
                {
                    self.trigger_voice(
                        note,
                        self.held_velocities[note as usize],
                        self.held_gains[note as usize],
                    );
                    self.arp_active_notes[slot] = Some(note);
                }
                self.arp_step = (self.arp_step + 1) % count.max(1);
                return;
            }

            // Pick next note
            let oct_range = self.arp_octave_range.clamp(1, 4) as usize;
            let total_steps = count * oct_range;
            let step = self.arp_step % total_steps;

            let (note_in_chord, oct_offset) = match self.arp_mode {
                ArpMode::Up => (step % count, (step / count) as u8),
                ArpMode::Down => (
                    (count - 1) - (step % count),
                    (oct_range - 1 - (step / count)) as u8,
                ),
                ArpMode::UpDown => {
                    let cycle_len = (count * 2).saturating_sub(2).max(1);
                    let sub_step = step % cycle_len;
                    let n_idx = if sub_step < count {
                        sub_step
                    } else {
                        cycle_len - sub_step
                    };
                    (n_idx, (step / count) as u8)
                }
                ArpMode::Random => {
                    self.arp_rng = self
                        .arp_rng
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223);
                    (
                        (self.arp_rng as usize) % count,
                        ((self.arp_rng >> 16) as usize % oct_range) as u8,
                    )
                }
                ArpMode::Off | ArpMode::Poly => unreachable!(),
            };

            let next_note = (held_list[note_in_chord] + oct_offset * 12).min(127);
            self.arp_active_notes[0] = Some(next_note);
            self.arp_step = (self.arp_step + 1) % total_steps;

            let source_note = held_list[note_in_chord] as usize;
            self.trigger_voice(
                next_note,
                self.held_velocities[source_note],
                self.held_gains[source_note],
            );
        }
    }

    fn release_arp_notes(&mut self) {
        for active in &mut self.arp_active_notes {
            if let Some(note) = active.take() {
                for voice in &mut self.voices {
                    if voice.active && voice.note == note {
                        voice.gate = false;
                    }
                }
            }
        }
    }

    /// Renders a single stereo frame `[left, right]` across all 16 polyphonic voices.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;

        // 1. Advance LFO
        self.lfo_phase = (self.lfo_phase + TAU * self.lfo_rate_hz * inv_fs) % TAU;
        let lfo_val = match self.lfo_shape {
            LfoShape::Triangle => 1.0 - 4.0 * ((self.lfo_phase / TAU) - 0.5).abs(),
            LfoShape::Saw => 2.0 * (self.lfo_phase / TAU) - 1.0,
            LfoShape::Square => {
                if self.lfo_phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => {
                if self.lfo_phase < (TAU * self.lfo_rate_hz * inv_fs) {
                    self.lfo_sh_val = ((self.lfo_phase * 1234.56).sin() * 43_758.547).fract();
                }
                self.lfo_sh_val
            }
        };

        // 2. Advance Arpeggiator
        self.tick_arpeggiator(inv_fs);

        // 3. Render and sum polyphonic voices
        let mut mix = 0.0f32;

        let eff_cutoff = match self.lfo_dest {
            LfoDestination::FilterCutoff => {
                (self.cutoff_hz * (1.0 + self.lfo_amount * lfo_val * 0.8)).clamp(20.0, 18000.0)
            }
            _ => self.cutoff_hz,
        };

        let pitch_mod = match self.lfo_dest {
            LfoDestination::OscPitch => 1.0 + self.lfo_amount * lfo_val * 0.05,
            _ => 1.0,
        };

        let bank = wavetable_bank();
        let sample_sound = self.sample_sound.as_deref();
        let wt_pos = if self.lfo_dest == LfoDestination::OscCtrl {
            (self.wavetable_position + self.lfo_amount * lfo_val * 0.5).clamp(0.0, 1.0)
        } else {
            self.wavetable_position.clamp(0.0, 1.0)
        };
        let fm_ratio = self.fm_ratio.clamp(0.25, 16.0);
        let fm_sub_ratio = self.fm_sub_ratio.clamp(0.25, 16.0);
        let fm_fb_amt = self.fm_feedback.clamp(0.0, 1.0);
        let fm_sub_amt = self.fm_sub_amount.clamp(0.0, 1.0);
        let fm_env_amt = self.fm_env_amount.clamp(0.0, 1.0);
        let mod_decay_tau = self.decay_sec.max(0.015);
        let mod_decay_coeff = (-inv_fs / mod_decay_tau).exp();

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            // Envelope advance
            if v.gate {
                if v.amp_env < 0.999 {
                    let atk = self.attack_sec.max(0.001);
                    v.amp_env += (1.0 - v.amp_env) * (inv_fs / atk).min(1.0);
                } else {
                    let dec = self.decay_sec.max(0.005);
                    v.amp_env += (self.sustain_level - v.amp_env) * (inv_fs / dec).min(1.0);
                }
            } else {
                let rel = self.release_sec.max(0.002);
                v.amp_env *= (-inv_fs / rel).exp();
                if v.amp_env < 1e-4 {
                    v.amp_env = 0.0;
                    v.active = false;
                    continue;
                }
            }
            v.mod_env *= mod_decay_coeff;

            let eff_freq = v.freq_hz * pitch_mod;
            let dt = (eff_freq * inv_fs).clamp(1e-5, 0.49);
            let t = v.phase;

            let raw_osc = match self.osc_mode {
                SynthOscMode::Analog => Self::render_va(
                    self.waveform,
                    self.supersaw_detune,
                    self.pulse_width,
                    self.lfo_dest,
                    self.lfo_amount,
                    lfo_val,
                    t,
                    dt,
                    eff_freq,
                    inv_fs,
                    &mut v.supersaw_phases,
                ),
                SynthOscMode::Wavetable => {
                    sample_wavetable(bank, self.wavetable_index, wt_pos, t, dt)
                }
                SynthOscMode::Fm => {
                    let op3 = (TAU * v.op3_phase).sin();
                    let fb = fm_fb_amt * 1.8 * v.mod_feedback_state;
                    let mod_sig = (TAU * v.mod_phase + fb + fm_sub_amt * 3.5 * op3).sin();
                    v.mod_feedback_state = 0.5 * (v.mod_feedback_state + mod_sig);
                    let dyn_env = (1.0 - fm_env_amt) + fm_env_amt * v.mod_env;
                    let fm_amt = if self.lfo_dest == LfoDestination::OscCtrl {
                        (self.fm_amount + self.lfo_amount * lfo_val * 0.4).clamp(0.0, 1.0)
                    } else {
                        self.fm_amount.clamp(0.0, 1.0)
                    };
                    (TAU * t + 4.0 * fm_amt * dyn_env * mod_sig).sin()
                }
                SynthOscMode::Sample => {
                    if let Some((sound, z_idx)) = sample_sound.zip(v.sample_zone_idx) {
                        if let Some(zone) = sound.zones.get(z_idx) {
                            let s = Self::render_sample_zone(
                                zone,
                                &mut v.sample_pos,
                                v.sample_step * f64::from(pitch_mod),
                                &mut v.active,
                            );
                            Self::apply_sample_timbre(self.sample_timbre, s, &mut v.sample_lp_state)
                        } else {
                            0.0
                        }
                    } else {
                        Self::render_va(
                            self.waveform,
                            self.supersaw_detune,
                            self.pulse_width,
                            self.lfo_dest,
                            self.lfo_amount,
                            lfo_val,
                            t,
                            dt,
                            eff_freq,
                            inv_fs,
                            &mut v.supersaw_phases,
                        )
                    }
                }
            };

            v.phase = (t + dt) % 1.0;
            v.mod_phase = (v.mod_phase + fm_ratio * dt) % 1.0;
            v.op3_phase = (v.op3_phase + fm_sub_ratio * dt) % 1.0;

            let voice_cutoff = if self.filter_env_amount.abs() > 1e-4 {
                (eff_cutoff
                    * 2.0f32.powf(self.filter_env_amount.clamp(-1.0, 1.0) * 3.0 * v.mod_env))
                .clamp(20.0, 18000.0)
            } else {
                eff_cutoff
            };

            let filtered = v.filter.process(
                raw_osc,
                self.filter_model,
                voice_cutoff,
                self.resonance,
                self.filter_drive,
                self.sample_rate,
            );

            mix += filtered * v.amp_env * v.velocity * v.gain * 0.25;
        }

        [mix, mix]
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn render_va(
        waveform: VaWaveform,
        supersaw_detune: f32,
        pulse_width: f32,
        lfo_dest: LfoDestination,
        lfo_amount: f32,
        lfo_val: f32,
        t: f32,
        dt: f32,
        eff_freq: f32,
        inv_fs: f32,
        supersaw_phases: &mut [f32; 7],
    ) -> f32 {
        match waveform {
            VaWaveform::Saw => (2.0 * t - 1.0) - poly_blep(t, dt),
            VaWaveform::SuperSaw => {
                let mut sum = 0.0f32;
                let detune_scale = supersaw_detune.clamp(0.0, 1.0);
                for (i, &det) in SUPERSAW_DETUNES.iter().enumerate() {
                    let f_osc = eff_freq * (1.0 + det * detune_scale * 0.08);
                    let dt_i = (f_osc * inv_fs).clamp(1e-5, 0.49);
                    let p_i = supersaw_phases[i];
                    let saw = (2.0 * p_i - 1.0) - poly_blep(p_i, dt_i);
                    sum += saw;
                    supersaw_phases[i] = (p_i + dt_i) % 1.0;
                }
                sum * (1.0 / 7.0)
            }
            VaWaveform::Pulse => {
                let mut pw = pulse_width.clamp(0.05, 0.95);
                if lfo_dest == LfoDestination::OscCtrl {
                    pw = (pw + lfo_amount * lfo_val * 0.4).clamp(0.05, 0.95);
                }
                let base = if t < pw { 1.0 } else { -1.0 };
                base + poly_blep(t, dt) - poly_blep((t + 1.0 - pw) % 1.0, dt)
            }
            VaWaveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            VaWaveform::Sine => (TAU * t).sin(),
        }
    }

    #[inline]
    fn render_sample_zone(
        zone: &LoadedSynthSample,
        sample_pos: &mut f64,
        step: f64,
        active: &mut bool,
    ) -> f32 {
        let len = zone.pcm.len();
        if len < 2 {
            *active = false;
            return 0.0;
        }
        let pos = *sample_pos;
        if let (Some(ls), Some(le)) = (zone.loop_start, zone.loop_end) {
            let le_c = le.min(len);
            if le_c > ls {
                let loop_len = (le_c - ls) as f64;
                let wrapped = if pos >= le_c as f64 {
                    ls as f64 + (pos - le_c as f64) % loop_len
                } else {
                    pos
                };
                let primary = Self::lerp_pcm(&zone.pcm, wrapped);
                let out = if zone.loop_crossfade > 0 {
                    let xf = zone.loop_crossfade.min((le_c - ls) / 2) as f64;
                    let dist_to_end = le_c as f64 - wrapped;
                    if xf >= 1.0 && dist_to_end < xf && ls as f64 >= dist_to_end {
                        let fade = (xf - dist_to_end) / xf;
                        let pre_loop = Self::lerp_pcm(&zone.pcm, ls as f64 - dist_to_end);
                        primary * (1.0 - fade as f32) + pre_loop * (fade as f32)
                    } else {
                        primary
                    }
                } else {
                    primary
                };
                let next_pos = wrapped + step;
                *sample_pos = if next_pos >= le_c as f64 {
                    ls as f64 + (next_pos - le_c as f64) % loop_len
                } else {
                    next_pos
                };
                return out;
            }
        }

        if pos >= (len - 1) as f64 {
            *active = false;
            return 0.0;
        }
        let out = Self::lerp_pcm(&zone.pcm, pos);
        *sample_pos = pos + step;
        out
    }

    #[inline]
    fn lerp_pcm(pcm: &[f32], pos: f64) -> f32 {
        let max_idx = pcm.len() - 1;
        let idx = (pos.floor() as usize).min(max_idx);
        let next = (idx + 1).min(max_idx);
        let frac = (pos - (idx as f64)).clamp(0.0, 1.0) as f32;
        pcm[idx] * (1.0 - frac) + pcm[next] * frac
    }

    #[inline]
    fn apply_sample_timbre(timbre: SampleTimbre, sample: f32, lp_state: &mut f32) -> f32 {
        match timbre {
            SampleTimbre::Off => sample,
            SampleTimbre::Soft => {
                *lp_state += 0.25 * (sample - *lp_state);
                *lp_state
            }
            SampleTimbre::Bright => {
                *lp_state += 0.18 * (sample - *lp_state);
                sample + 0.55 * (sample - *lp_state)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polyblep_zero_outside_discontinuity() {
        assert_eq!(poly_blep(0.5, 0.01), 0.0);
        assert!(poly_blep(0.002, 0.01) < 0.0);
        assert!(poly_blep(0.998, 0.01) > 0.0);
    }

    #[test]
    fn test_zdf_ladder24_attenuates_high_frequencies() {
        let mut filter = ZdfFilter::default();
        let fs = 48_000.0;
        let mut energy = 0.0f32;
        for n in 0..512 {
            let x = (TAU * 12_000.0 * (n as f32) / fs).sin();
            let y = filter.process(x, FilterModel::Ladder24, 400.0, 0.2, 0.0, fs);
            assert!(y.is_finite());
            if n >= 128 {
                energy += y * y;
            }
        }
        let rms = (energy / 384.0).sqrt();
        assert!(
            rms < 0.02,
            "12 kHz signal must be strongly attenuated by 400 Hz 24dB Ladder (rms={rms})"
        );
    }

    #[test]
    fn test_synth_polyphonic_voices() {
        let mut synth = SynthLayer::new(48_000.0);
        // Play 3-note chord: C4 (60), E4 (64), G4 (67)
        synth.note_on(60, 100);
        synth.note_on(64, 100);
        synth.note_on(67, 100);

        let active_count = synth.voices.iter().filter(|v| v.active).count();
        assert_eq!(active_count, 3, "Chord must allocate 3 polyphonic voices");

        let mut chord_energy = 0.0f32;
        for _ in 0..512 {
            let [l, r] = synth.render_frame();
            chord_energy += l * l + r * r;
        }
        assert!(chord_energy > 0.05);

        // Release one note
        synth.note_off(60);
        assert!(!synth.voices.iter().find(|v| v.note == 60).unwrap().gate);
        assert!(synth.voices.iter().find(|v| v.note == 64).unwrap().gate);
    }

    #[test]
    fn test_arpeggiator_generation() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.arp_mode = ArpMode::Up;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 100);
        synth.note_on(64, 100);
        synth.note_on(67, 100);

        let mut energy = 0.0f32;
        for _ in 0..24000 {
            let [l, r] = synth.render_frame();
            energy += l * l + r * r;
        }
        assert!(energy > 0.05, "Arpeggiator must output active sound energy");
        assert!(synth.arp_step > 0, "Arpeggiator step must advance");
    }

    #[test]
    fn poly_arpeggiator_triggers_held_chord_simultaneously() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.arp_mode = ArpMode::Poly;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 70);
        synth.note_on(64, 90);
        synth.note_on(67, 110);
        for _ in 0..3_200 {
            synth.render_frame();
        }
        for note in [60, 64, 67] {
            assert!(synth
                .voices
                .iter()
                .any(|voice| voice.active && voice.gate && voice.note == note));
        }
    }

    #[test]
    fn arpeggiator_uses_allocator_without_overwriting_an_active_voice() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.note_on(40, 100);
        synth.note_off(40);
        synth.arp_mode = ArpMode::Up;
        synth.arp_rate_bpm = 240.0;
        synth.note_on(60, 100);
        synth.note_on(64, 100);
        for _ in 0..3_200 {
            synth.render_frame();
        }
        assert_eq!(synth.voices[0].note, 40);
        assert!(!synth.voices[0].gate);
        assert!(synth
            .voices
            .iter()
            .any(|voice| voice.active && voice.gate && voice.note == 60));
    }

    #[test]
    fn test_lfo_modulation() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.lfo_shape = LfoShape::Triangle;
        synth.lfo_amount = 0.8;
        synth.lfo_rate_hz = 10.0;
        synth.note_on(60, 100);

        for _ in 0..1024 {
            let [l, r] = synth.render_frame();
            assert!(l.is_finite() && r.is_finite());
        }
        assert!(synth.lfo_phase > 0.0);
    }

    #[test]
    fn test_64_mipmapped_wavetables_render_distinct_and_bandlimited() {
        let bank = wavetable_bank();
        assert_eq!(bank.tables.len(), NUM_WAVETABLES);

        let mut family_rms = Vec::new();
        for wt_idx in (0..NUM_WAVETABLES).step_by(8) {
            let mut synth = SynthLayer::new(48_000.0);
            synth.osc_mode = SynthOscMode::Wavetable;
            synth.wavetable_index = wt_idx as u8;
            synth.wavetable_position = 0.35;
            synth.cutoff_hz = 16_000.0;
            synth.note_on(48, 110);

            let mut e = 0.0f32;
            for _ in 0..512 {
                let [l, r] = synth.render_frame();
                assert!(l.is_finite() && r.is_finite());
                e += l * l;
            }
            assert!(e > 0.01, "wavetable {wt_idx} must render active audio");
            family_rms.push((e / 512.0).sqrt());
        }
        assert!(family_rms.iter().any(|&x| (x - family_rms[0]).abs() > 1e-3));

        // High MIDI note (C8 = 108) must select coarser mipmap without NaN or excessive Nyquist energy
        let mut high = SynthLayer::new(48_000.0);
        high.osc_mode = SynthOscMode::Wavetable;
        high.wavetable_index = 42;
        high.note_on(108, 100);
        for _ in 0..256 {
            let [l, r] = high.render_frame();
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() < 1.5);
        }
    }

    #[test]
    fn test_three_operator_fm_with_feedback_and_mod_env() {
        let mut s2 = SynthLayer::new(48_000.0);
        s2.osc_mode = SynthOscMode::Fm;
        s2.fm_amount = 0.7;
        s2.fm_ratio = 2.0;
        s2.fm_feedback = 0.0;
        s2.fm_sub_amount = 0.0;
        s2.cutoff_hz = 14_000.0;
        s2.note_on(60, 100);

        let mut s3 = s2.clone();
        s3.fm_ratio = std::f32::consts::SQRT_2;
        s3.fm_feedback = 0.75;
        s3.fm_sub_ratio = 5.0;
        s3.fm_sub_amount = 0.8;

        let mut diff_energy = 0.0f32;
        for _ in 0..1024 {
            let [a, _] = s2.render_frame();
            let [b, _] = s3.render_frame();
            assert!(a.is_finite() && b.is_finite());
            diff_energy += (a - b) * (a - b);
        }
        assert!(
            diff_energy > 0.05,
            "3-op ratio + feedback FM must produce distinct timbre"
        );
        assert!(
            s3.voices[0].mod_env < 1.0,
            "FM mod envelope must decay smoothly during note sustain"
        );
    }

    #[test]
    fn test_sample_mode_zone_playback_loop_crossfade_and_timbre() {
        let fs = 48_000.0;
        let len = 512;
        let pcm: Vec<f32> = (0..len)
            .map(|i| {
                let ph = (i as f32) / 32.0;
                (TAU * ph).sin() + 0.4 * (3.0 * TAU * ph).sin()
            })
            .collect();
        let sound = Arc::new(SynthSampleSound::new_synthetic(
            "TestHarmonicSample",
            60,
            &pcm,
            fs,
            Some((128, 384)),
            16,
        ));

        let mut s_soft = SynthLayer::new(fs);
        s_soft.osc_mode = SynthOscMode::Sample;
        s_soft.sample_timbre = SampleTimbre::Soft;
        s_soft.cutoff_hz = 16_000.0;
        s_soft.load_sample(Some(sound.clone()));
        assert_eq!(s_soft.loaded_sample_name(), Some("TestHarmonicSample"));
        s_soft.note_on(72, 110); // +12 semitones => 2.0x playback speed

        let mut s_bright = SynthLayer::new(fs);
        s_bright.osc_mode = SynthOscMode::Sample;
        s_bright.sample_timbre = SampleTimbre::Bright;
        s_bright.cutoff_hz = 16_000.0;
        s_bright.load_sample(Some(sound));
        s_bright.note_on(72, 110);

        assert!((s_soft.voices[0].sample_step - 2.0).abs() < 1e-4);

        let mut soft_energy = 0.0f32;
        let mut bright_energy = 0.0f32;
        for _ in 0..1024 {
            let [l_s, _] = s_soft.render_frame();
            let [l_b, _] = s_bright.render_frame();
            assert!(l_s.is_finite() && l_b.is_finite());
            soft_energy += l_s * l_s;
            bright_energy += l_b * l_b;
        }
        // Looped 1024 frames @2x speed crossed the loop boundary multiple times without stopping
        assert!(s_soft.voices[0].active);
        assert!(soft_energy > 0.02);
        assert!(
            bright_energy > soft_energy,
            "Bright timbre must retain more HF energy than Soft damping"
        );
    }
}
