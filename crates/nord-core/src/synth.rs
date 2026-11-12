//! 3-Layer Nord Wave 2 Derived Synth Engine (`DSPS` — PolyBLEP VA + ZDF Ladder Filter + LFO + Arp).
//!
//! Spec: `docs/specs/synth-engine.md`

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

pub use crate::synth_sample::{LoadedSynthSample, SampleTimbre, SynthSampleSound};
use crate::synth_tables::{
    lfo_pitch_semitones, UNISON_VOICES, UNISON_VOICE_COUNT, UNISON_WEAK_CHANNEL_GAIN,
};
pub use crate::synth_types::{
    ArpMode, FilterModel, LfoDestination, LfoShape, NotePriority, SynthOscMode, VaWaveform,
    VibratoMode,
};
pub use crate::synth_wavetable::NUM_WAVETABLES;
use crate::synth_wavetable::{sample_wavetable, wavetable_bank};

mod arp;
mod sample_osc;
mod voice_alloc;

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;

    pub(crate) fn render_n(synth: &mut SynthLayer, n: usize) -> Vec<[f32; 2]> {
        (0..n).map(|_| synth.render_frame()).collect()
    }

    pub(crate) fn energy(frames: &[[f32; 2]]) -> f32 {
        frames.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum()
    }

    pub(crate) fn samples_until_freq(s: &mut SynthLayer, target: f32) -> usize {
        for n in 0..400_000 {
            s.render_frame();
            let v = s.voices.iter().find(|v| v.active).unwrap();
            if (v.freq_hz - target).abs() < target * 0.001 {
                return n;
            }
        }
        panic!("glide never reached target");
    }
}

/// Maximum polyphonic voices per Synth layer. Pre-allocated with zero heap allocations during playback.
pub const MAX_SYNTH_VOICES: usize = 16;

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

/// Left/right gains of a unison voice: full level in its strong channel, `weak` in the other.
#[inline]
fn unison_pan(strong_left: bool, weak: f32, norm: f32) -> (f32, f32) {
    if strong_left {
        (norm, norm * weak)
    } else {
        (norm * weak, norm)
    }
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
        let fc = cutoff_hz.clamp(14.0, 0.48 * sample_rate);
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

/// ASSUMED: start phases of the four unison voices; they only decorrelate the voices.
pub(crate) const UNISON_START_PHASES: [f32; 4] = [0.0, 0.31, 0.57, 0.83];

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
    pub uni_supersaw: [[f32; 7]; 4],
    pub supersaw_phases: [f32; 7],
    pub amp_env: f32,
    pub filter: ZdfFilter,
    pub filter_right: ZdfFilter,
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
            gain: 1.0,
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
            sample_zone_idx: None,
            sample_pos: 0.0,
            sample_step: 1.0,
            sample_lp_state: 0.0,
            sample_lp_state_right: 0.0,
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
    /// Keyboard cutoff tracking (`0` = Off, `1` = 1/3, `2` = 2/3, `3` = 1/1).
    pub kb_track: u8,
    pub attack_sec: f32,
    pub decay_sec: f32,
    pub sustain_level: f32,
    pub release_sec: f32,

    // Dedicated filter envelope (ADR). When `filter_env_dedicated` is false the
    // filter envelope mirrors the amp decay (legacy behaviour).
    pub filter_env_dedicated: bool,
    pub filter_env_attack_sec: f32,
    pub filter_env_decay_sec: f32,
    pub filter_env_release_sec: f32,
    /// Velocity -> filter envelope depth (`0..1`; `0` = velocity ignored).
    /// ASSUMED: peak = `1 - amount * (1 - velocity)` (linear); plausible for a
    /// velocity-scaled depth; confirm by comparing filter sweep depth at vel 40 vs 127 on hardware.
    pub filter_env_vel_amount: f32,

    // Voice mode
    /// Unison setting `0..=3`. EMPIRICAL + firmware-VERIFIED voices: `1` = -2/+2 cents, `2` =
    /// -4/-1.32/+1.33/+4, `3` = -6/-2/+2/+6, no centre voice, each voice mostly in one channel;
    /// see `synth_tables::UNISON_VOICES`. Applies to Analog and Wavetable modes; FM and Sample
    /// modes ignore it (Sample unison uses a different detune set, unmodelled).
    pub unison: u8,
    pub mono: bool,
    pub legato: bool,
    /// Glide (portamento) in seconds per octave; `0` = off. Constant-rate in pitch: the
    /// manual says "the greater the distance between two notes, the longer the glide".
    /// ASSUMED: linear-in-semitones slew; the seconds-per-octave scale is not measured.
    /// Confirm by recording a one-octave glide at several Glide knob values.
    pub glide_sec: f32,
    pub note_priority: NotePriority,
    /// Fine + coarse tuning in cents.
    pub pitch_cents: f32,
    /// Amp velocity sensitivity `0..1` (`0` = velocity ignored).
    pub amp_velocity: f32,
    /// `false` bypasses the filter.
    pub filter_on: bool,
    // Oscillator envelope (ADR) modulating Osc Ctrl, or pitch with `osc_env_to_pitch`.
    pub osc_env_attack_sec: f32,
    pub osc_env_decay_sec: f32,
    pub osc_env_release_sec: f32,
    /// Bipolar amount `-1..1`.
    pub osc_env_amount: f32,
    pub osc_env_to_pitch: bool,
    pub osc_env_velocity: bool,
    // Vibrato
    pub vibrato_mode: VibratoMode,
    pub vibrato_rate_hz: f32,
    pub vibrato_depth_cents: f32,
    pub vibrato_delay_sec: f32,
    /// Controller value `0..1` for `Wheel`/`Aftertouch`/`Pedal` vibrato modes.
    pub vibrato_ctrl: f32,
    vib_phase: f32,
    /// Octave offset in octaves (`-3..=3`).
    /// ASSUMED: range clamp of +/-3 octaves; confirm against the panel Octave Shift range.
    pub octave_offset: i8,
    last_freq_hz: f32,

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
            kb_track: 0,
            attack_sec: 0.005,
            decay_sec: 0.3,
            sustain_level: 0.7,
            release_sec: 0.25,

            filter_env_dedicated: false,
            filter_env_attack_sec: 0.005,
            filter_env_decay_sec: 0.3,
            filter_env_release_sec: 0.25,
            filter_env_vel_amount: 0.0,

            unison: 0,
            mono: false,
            legato: false,
            glide_sec: 0.0,
            note_priority: NotePriority::Off,
            pitch_cents: 0.0,
            amp_velocity: 1.0,
            filter_on: true,
            osc_env_attack_sec: 0.005,
            osc_env_decay_sec: 0.3,
            osc_env_release_sec: 0.25,
            osc_env_amount: 0.0,
            osc_env_to_pitch: false,
            osc_env_velocity: false,
            vibrato_mode: VibratoMode::Off,
            vibrato_rate_hz: 5.0,
            vibrato_depth_cents: 0.0,
            vibrato_delay_sec: 0.0,
            vibrato_ctrl: 0.0,
            vib_phase: 0.0,
            octave_offset: 0,
            last_freq_hz: 0.0,

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
        for v in &mut self.voices {
            v.active = false;
        }
        self.sample_sound = sound;
    }

    /// Returns the name of the currently loaded multisample sound, if any.
    pub fn loaded_sample_name(&self) -> Option<&str> {
        self.sample_sound.as_ref().map(|s| s.name.as_str())
    }

    /// Renders a single stereo frame `[left, right]` across all 16 polyphonic voices.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;

        // 1. Advance LFO
        self.lfo_phase = (self.lfo_phase + TAU * self.lfo_rate_hz * inv_fs) % TAU;
        let lfo_val = match self.lfo_shape {
            LfoShape::Triangle => 1.0 - 4.0 * ((self.lfo_phase / TAU) - 0.5).abs(),
            LfoShape::Saw => 2.0 * (self.lfo_phase / TAU) - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * (self.lfo_phase / TAU),
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
        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        let eff_cutoff = match self.lfo_dest {
            LfoDestination::FilterCutoff => (self.cutoff_hz
                * (1.0 + self.lfo_amount * lfo_val * 0.8))
                .clamp(14.0, 0.48 * self.sample_rate),
            _ => self.cutoff_hz,
        };

        let pitch_mod = match self.lfo_dest {
            // VERIFIED depth curve (firmware table, see `synth_tables.rs`): 0..48 semitones.
            LfoDestination::OscPitch => {
                2.0f32.powf(lfo_pitch_semitones(self.lfo_amount) * lfo_val / 12.0)
            }
            _ => 1.0,
        };

        let bank = wavetable_bank();
        let sample_sound = self.sample_sound.as_deref();
        let wt_pos_base = if self.lfo_dest == LfoDestination::OscCtrl {
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
        // Constant-rate glide: semitones travelled per sample (see `glide_sec`).
        let glide_step_semi = if self.glide_sec > 0.0 {
            12.0 * inv_fs / self.glide_sec.max(0.001)
        } else {
            0.0
        };
        self.vib_phase =
            (self.vib_phase + TAU * self.vibrato_rate_hz.clamp(0.1, 20.0) * inv_fs) % TAU;
        let vib_sin = self.vib_phase.sin();
        let uni_level = usize::from(self.unison.min(3));
        let uni_count = UNISON_VOICE_COUNT[uni_level];
        // Per voice gains: constant power over the partner voices, measured weak-channel leakage.
        let uni_voice_gain = if uni_count > 0 {
            1.0 / (uni_count as f32).sqrt()
        } else {
            0.0
        };
        let weak = UNISON_WEAK_CHANNEL_GAIN;
        let pan_norm = 1.0 / (1.0 + weak * weak).sqrt();

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            // Envelope advance (OS v1.66 hardware floors: 0.5 ms attack, 3.0 ms decay/release, >=44 s decay = Hold)
            if v.gate {
                if v.amp_env < 0.999 {
                    let atk = self.attack_sec.max(0.0005);
                    v.amp_env += (1.0 - v.amp_env) * (inv_fs / atk).min(1.0);
                } else if self.decay_sec < 44.0 {
                    let dec = self.decay_sec.max(0.003);
                    v.amp_env += (self.sustain_level - v.amp_env) * (inv_fs / dec).min(1.0);
                }
            } else {
                let rel = self.release_sec.max(0.003);
                v.amp_env *= (-inv_fs / rel).exp();
                if v.amp_env < 1e-4 {
                    v.amp_env = 0.0;
                    v.active = false;
                    continue;
                }
            }
            v.mod_env *= mod_decay_coeff;

            if glide_step_semi > 0.0 && v.freq_hz != v.target_freq_hz {
                let d = 12.0 * (v.target_freq_hz / v.freq_hz.max(1.0)).log2();
                if d.abs() <= glide_step_semi {
                    v.freq_hz = v.target_freq_hz;
                } else {
                    v.freq_hz *= 2.0f32.powf(d.signum() * glide_step_semi / 12.0);
                }
            } else {
                v.freq_hz = v.target_freq_hz;
            }
            v.age_sec += inv_fs;

            // Oscillator envelope (ADR); `osc_env_velocity` scales its depth by velocity.
            if v.gate {
                if !v.oenv_attack_done {
                    let atk = self.osc_env_attack_sec.max(0.0005);
                    v.oenv += (1.0 - v.oenv) * (inv_fs / atk).min(1.0);
                    if v.oenv >= 0.999 {
                        v.oenv = 1.0;
                        v.oenv_attack_done = true;
                    }
                } else if self.osc_env_decay_sec < 44.0 {
                    let dec = self.osc_env_decay_sec.max(0.003);
                    v.oenv -= v.oenv * (inv_fs / dec).min(1.0);
                }
            } else {
                let rel = self.osc_env_release_sec.max(0.003);
                v.oenv *= (-inv_fs / rel).exp();
            }
            let oenv_depth = if self.osc_env_velocity {
                self.osc_env_amount * v.velocity
            } else {
                self.osc_env_amount
            };
            let osc_env_mod = oenv_depth * v.oenv;
            let osc_ctrl_mod = if self.osc_env_to_pitch {
                0.0
            } else {
                osc_env_mod
            };
            // ASSUMED: +/-12 semitones of pitch at full Env Amt (Env To Pitch) and +/-0.5 of Osc Ctrl range.
            let env_pitch = if self.osc_env_to_pitch {
                2.0f32.powf(osc_env_mod)
            } else {
                1.0
            };
            let wt_pos = (wt_pos_base + osc_ctrl_mod * 0.5).clamp(0.0, 1.0);
            let vib_amount = match self.vibrato_mode {
                VibratoMode::Off => 0.0,
                VibratoMode::On => 1.0,
                VibratoMode::Delay => ((v.age_sec - self.vibrato_delay_sec) / 0.5).clamp(0.0, 1.0),
                VibratoMode::Wheel | VibratoMode::Aftertouch | VibratoMode::Pedal => {
                    self.vibrato_ctrl.clamp(0.0, 1.0)
                }
            };
            let vib_pitch = 2.0f32.powf(vib_amount * self.vibrato_depth_cents * vib_sin / 1200.0);

            // Filter envelope: dedicated ADR, or mirror the amp decay (legacy).
            let fenv_peak = 1.0 - self.filter_env_vel_amount.clamp(0.0, 1.0) * (1.0 - v.velocity);
            let filter_env_level = if self.filter_env_dedicated {
                if v.gate {
                    if !v.fenv_attack_done {
                        let atk = self.filter_env_attack_sec.max(0.0005);
                        v.fenv += (1.0 - v.fenv) * (inv_fs / atk).min(1.0);
                        if v.fenv >= 0.999 {
                            v.fenv = 1.0;
                            v.fenv_attack_done = true;
                        }
                    } else if self.filter_env_decay_sec < 44.0 {
                        let dec = self.filter_env_decay_sec.max(0.003);
                        v.fenv -= v.fenv * (inv_fs / dec).min(1.0);
                    }
                } else {
                    let rel = self.filter_env_release_sec.max(0.003);
                    v.fenv *= (-inv_fs / rel).exp();
                }
                v.fenv * fenv_peak
            } else {
                v.mod_env * fenv_peak
            };

            let eff_freq = v.freq_hz * pitch_mod * env_pitch * vib_pitch;
            let dt = (eff_freq * inv_fs).clamp(1e-5, 0.49);
            let t = v.phase;

            let (raw_osc_l, raw_osc_r) = match self.osc_mode {
                SynthOscMode::Analog => {
                    if uni_count > 0 {
                        let mut acc = (0.0f32, 0.0f32);
                        for (k, voice) in UNISON_VOICES[uni_level][..uni_count]
                            .iter()
                            .copied()
                            .enumerate()
                        {
                            let f_k = eff_freq * voice.ratio;
                            let dt_k = (f_k * inv_fs).clamp(1e-5, 0.49);
                            let t_k = v.uni_phase[k];
                            let s_k = Self::render_va(
                                self.waveform,
                                self.supersaw_detune,
                                self.pulse_width,
                                self.lfo_dest,
                                self.lfo_amount,
                                lfo_val,
                                t_k,
                                dt_k,
                                f_k,
                                inv_fs,
                                &mut v.uni_supersaw[k],
                            );
                            v.uni_phase[k] = (t_k + dt_k) % 1.0;
                            let (gl, gr) = unison_pan(voice.strong_left, weak, pan_norm);
                            acc.0 += s_k * uni_voice_gain * gl;
                            acc.1 += s_k * uni_voice_gain * gr;
                        }
                        (acc.0, Some(acc.1))
                    } else {
                        (
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
                            ),
                            None,
                        )
                    }
                }
                SynthOscMode::Wavetable => {
                    if uni_count > 0 {
                        let mut acc = (0.0f32, 0.0f32);
                        for (k, voice) in UNISON_VOICES[uni_level][..uni_count]
                            .iter()
                            .copied()
                            .enumerate()
                        {
                            let dt_k = (eff_freq * voice.ratio * inv_fs).clamp(1e-5, 0.49);
                            let t_k = v.uni_phase[k];
                            let s_k =
                                sample_wavetable(bank, self.wavetable_index, wt_pos, t_k, dt_k);
                            v.uni_phase[k] = (t_k + dt_k) % 1.0;
                            let (gl, gr) = unison_pan(voice.strong_left, weak, pan_norm);
                            acc.0 += s_k * uni_voice_gain * gl;
                            acc.1 += s_k * uni_voice_gain * gr;
                        }
                        (acc.0, Some(acc.1))
                    } else {
                        (
                            sample_wavetable(bank, self.wavetable_index, wt_pos, t, dt),
                            None,
                        )
                    }
                }
                SynthOscMode::Fm => {
                    let op3 = (TAU * v.op3_phase).sin();
                    let fb = fm_fb_amt * 1.8 * v.mod_feedback_state;
                    let mod_sig = (TAU * v.mod_phase + fb + fm_sub_amt * 3.5 * op3).sin();
                    v.mod_feedback_state = 0.5 * (v.mod_feedback_state + mod_sig);
                    let dyn_env = (1.0 - fm_env_amt) + fm_env_amt * v.mod_env;
                    let fm_amt = if self.lfo_dest == LfoDestination::OscCtrl {
                        (self.fm_amount + self.lfo_amount * lfo_val * 0.4 + osc_ctrl_mod * 0.4)
                            .clamp(0.0, 1.0)
                    } else {
                        (self.fm_amount + osc_ctrl_mod * 0.4).clamp(0.0, 1.0)
                    };
                    ((TAU * t + 4.0 * fm_amt * dyn_env * mod_sig).sin(), None)
                }
                SynthOscMode::Sample => {
                    if let Some((sound, z_idx)) = sample_sound.zip(v.sample_zone_idx) {
                        if let Some(zone) = sound.zones.get(z_idx) {
                            let (sl, sr) = Self::render_sample_zone_stereo(
                                zone,
                                &mut v.sample_pos,
                                v.sample_step * f64::from(pitch_mod),
                                &mut v.active,
                            );
                            let fl = Self::apply_sample_timbre(
                                self.sample_timbre,
                                sl,
                                &mut v.sample_lp_state,
                            );
                            let fr = sr.map(|r| {
                                Self::apply_sample_timbre(
                                    self.sample_timbre,
                                    r,
                                    &mut v.sample_lp_state_right,
                                )
                            });
                            (fl, fr)
                        } else {
                            (0.0, None)
                        }
                    } else {
                        let va = Self::render_va(
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
                        );
                        (va, None)
                    }
                }
            };

            v.phase = (t + dt) % 1.0;
            v.mod_phase = (v.mod_phase + fm_ratio * dt) % 1.0;
            v.op3_phase = (v.op3_phase + fm_sub_ratio * dt) % 1.0;

            let kb_track_scale = if self.kb_track > 0 {
                let frac = f32::from(self.kb_track.min(3)) / 3.0;
                2.0f32.powf(((f32::from(v.note) - 60.0) / 12.0) * frac)
            } else {
                1.0
            };
            let env_scale = if self.filter_env_amount.abs() > 1e-4 {
                // ASSUMED (inherited, no recorded source): +/-3 octaves of cutoff at full env amount.
                2.0f32.powf(self.filter_env_amount.clamp(-1.0, 1.0) * 3.0 * filter_env_level)
            } else {
                1.0
            };
            let voice_cutoff =
                (eff_cutoff * kb_track_scale * env_scale).clamp(14.0, 0.48 * self.sample_rate);

            let filtered_l = if self.filter_on {
                v.filter.process(
                    raw_osc_l,
                    self.filter_model,
                    voice_cutoff,
                    self.resonance,
                    self.filter_drive,
                    self.sample_rate,
                )
            } else {
                raw_osc_l
            };
            let amp_vel = (1.0 - self.amp_velocity.clamp(0.0, 1.0))
                + self.amp_velocity.clamp(0.0, 1.0) * v.velocity;
            let env_gain = v.amp_env * amp_vel * v.gain * 0.25;
            mix_l += filtered_l * env_gain;
            if let Some(osc_r) = raw_osc_r {
                let filtered_r = if self.filter_on {
                    v.filter_right.process(
                        osc_r,
                        self.filter_model,
                        voice_cutoff,
                        self.resonance,
                        self.filter_drive,
                        self.sample_rate,
                    )
                } else {
                    osc_r
                };
                mix_r += filtered_r * env_gain;
            } else {
                mix_r += filtered_l * env_gain;
            }
        }

        [mix_l, mix_r]
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
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
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
    fn dedicated_filter_envelope_uses_adr_and_velocity() {
        let mut s = SynthLayer::new(48_000.0);
        s.filter_env_dedicated = true;
        s.filter_env_amount = 1.0;
        s.filter_env_attack_sec = 0.001;
        s.filter_env_decay_sec = 0.01;
        s.filter_env_vel_amount = 1.0;
        s.note_on(60, 127);
        render_n(&mut s, 4800);
        let hi = s.voices.iter().find(|v| v.active).unwrap();
        assert!(hi.fenv_attack_done);
        assert!(
            hi.fenv < 0.05,
            "decay should fall toward 0, got {}",
            hi.fenv
        );
        s.note_off(60);
        let before = s.voices.iter().find(|v| v.active).unwrap().fenv;
        render_n(&mut s, 100);
        let after = s.voices.iter().find(|v| v.active).unwrap().fenv;
        assert!(after <= before);
    }

    #[test]
    fn amp_velocity_zero_ignores_velocity_and_one_follows_it() {
        let run = |sens: f32, vel: u8| {
            let mut s = SynthLayer::new(48_000.0);
            s.amp_velocity = sens;
            s.note_on(60, vel);
            energy(&render_n(&mut s, 4000))
        };
        assert!((run(0.0, 20) - run(0.0, 127)).abs() < 1e-3);
        assert!(run(1.0, 20) < 0.2 * run(1.0, 127));
    }

    #[test]
    fn filter_off_bypasses_a_closed_filter() {
        let run = |on: bool| {
            let mut s = SynthLayer::new(48_000.0);
            s.cutoff_hz = 40.0;
            s.filter_on = on;
            s.note_on(81, 100);
            energy(&render_n(&mut s, 4000))
        };
        assert!(run(false) > 10.0 * run(true));
    }

    #[test]
    fn vibrato_delay_keeps_output_identical_until_the_delay_elapses() {
        let run = |mode: VibratoMode| {
            let mut s = SynthLayer::new(48_000.0);
            s.vibrato_mode = mode;
            s.vibrato_depth_cents = 100.0;
            s.vibrato_delay_sec = 0.05;
            s.vibrato_rate_hz = 6.0;
            s.note_on(60, 100);
            render_n(&mut s, 12_000)
        };
        let off = run(VibratoMode::Off);
        let dly = run(VibratoMode::Delay);
        assert_eq!(off[..2000], dly[..2000]);
        assert_ne!(off[8000..], dly[8000..]);
        let on = run(VibratoMode::On);
        assert_ne!(off[1000..2000], on[1000..2000]);
    }

    #[test]
    fn oscillator_envelope_modulates_pitch_or_osc_ctrl_only_when_amount_set() {
        let run = |amount: f32, to_pitch: bool| {
            let mut s = SynthLayer::new(48_000.0);
            s.osc_mode = SynthOscMode::Wavetable;
            s.osc_env_amount = amount;
            s.osc_env_to_pitch = to_pitch;
            s.osc_env_attack_sec = 0.001;
            s.osc_env_decay_sec = 0.2;
            s.note_on(60, 100);
            render_n(&mut s, 3000)
        };
        assert_eq!(run(0.0, false), run(0.0, true));
        assert_ne!(run(1.0, true), run(0.0, true));
        assert_ne!(run(1.0, false), run(0.0, false));
        assert!(run(-1.0, true).iter().all(|f| f[0].is_finite()));
    }

    #[test]
    fn lfo_saw_down_differs_from_saw_up_and_off_destination_is_inert() {
        let run = |shape: LfoShape, dest: LfoDestination| {
            let mut s = SynthLayer::new(48_000.0);
            s.lfo_shape = shape;
            s.lfo_dest = dest;
            s.lfo_amount = 1.0;
            s.lfo_rate_hz = 5.0;
            s.note_on(60, 100);
            render_n(&mut s, 6000)
        };
        let up = run(LfoShape::Saw, LfoDestination::OscPitch);
        let down = run(LfoShape::SawDown, LfoDestination::OscPitch);
        assert_ne!(up, down);
        let off = run(LfoShape::Square, LfoDestination::Off);
        let tri_off = run(LfoShape::Triangle, LfoDestination::Off);
        assert_eq!(off, tri_off, "LFO off must not modulate anything");
    }

    #[test]
    fn lfo_pitch_depth_follows_the_firmware_curve_and_stays_finite_at_full_depth() {
        let run = |amount: f32| {
            let mut s = SynthLayer::new(48_000.0);
            s.lfo_dest = LfoDestination::OscPitch;
            s.lfo_shape = LfoShape::Square;
            s.lfo_rate_hz = 20.0;
            s.lfo_amount = amount;
            s.note_on(60, 100);
            render_n(&mut s, 4000)
        };
        let full = run(1.0);
        assert!(full.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        // 1 semitone (raw 27) differs from the old fixed 5% rule and from a full +/-48 semitone swing.
        assert_ne!(run(27.0 / 127.0), full);
        assert_ne!(run(27.0 / 127.0), run(0.0));
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
    fn test_synth_kb_track_and_14hz_floor() {
        let mut s_track = SynthLayer::new(48_000.0);
        s_track.cutoff_hz = 220.0;
        s_track.kb_track = 3; // 1/1 octave tracking
        s_track.note_on(84, 120); // +2 octaves above C4 -> 4x cutoff -> brighter than low note
        let mut rms_high = 0.0f32;
        for _ in 0..512 {
            let [l, _] = s_track.render_frame();
            rms_high += l * l;
        }

        let mut s_notrack = SynthLayer::new(48_000.0);
        s_notrack.cutoff_hz = 220.0;
        s_notrack.kb_track = 0;
        s_notrack.note_on(84, 120);
        let mut rms_flat = 0.0f32;
        for _ in 0..512 {
            let [l, _] = s_notrack.render_frame();
            rms_flat += l * l;
        }
        assert!(
            rms_high > rms_flat * 1.5,
            "1/1 kb_track must open filter on C6 ({rms_high} vs {rms_flat})"
        );

        let mut flt = ZdfFilter::default();
        let out = flt.process(1.0, FilterModel::Lp24, 14.0, 0.0, 0.0, 48_000.0);
        assert!(out.is_finite());
    }
}
