//! 3-Layer Nord Wave 2 Derived Synth Engine (`DSPS` — PolyBLEP VA + ZDF Ladder Filter + LFO + Arp).
//!
//! Spec: `docs/specs/synth-engine.md`

use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// Maximum polyphonic voices per Synth layer. Pre-allocated with zero heap allocations during playback.
pub const MAX_SYNTH_VOICES: usize = 16;

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
    pub freq_hz: f32,
    pub phase: f32,
    pub mod_phase: f32,
    pub supersaw_phases: [f32; 7],
    pub amp_env: f32,
    pub filter: ZdfFilter,
}

impl Default for SynthVoice {
    fn default() -> Self {
        Self {
            active: false,
            gate: false,
            note: 0,
            velocity: 0.8,
            freq_hz: 440.0,
            phase: 0.0,
            mod_phase: 0.0,
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.0,
            filter: ZdfFilter::default(),
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
    pub supersaw_detune: f32,
    pub wavetable_position: f32,
    pub filter_model: FilterModel,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub filter_drive: f32,
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
    arp_active_note: Option<u8>,

    // 16 Polyphonic Voices
    voices: [SynthVoice; MAX_SYNTH_VOICES],
}

impl SynthLayer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(8000.0),
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::Saw,
            pulse_width: 0.5,
            fm_amount: 0.35,
            supersaw_detune: 0.3,
            wavetable_position: 0.0,
            filter_model: FilterModel::Ladder24,
            cutoff_hz: 2400.0,
            resonance: 0.3,
            filter_drive: 0.2,
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
            arp_active_note: None,

            voices: [SynthVoice::default(); MAX_SYNTH_VOICES],
        }
    }

    /// Triggers a note on across the polyphonic voice allocator or records in arpeggiator list.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let note_idx = (note as usize).min(127);
        self.held_keys[note_idx] = true;

        if self.arp_mode != ArpMode::Off {
            return;
        }

        let vel_norm = (f32::from(velocity.clamp(1, 127))) / 127.0;
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

        self.voices[idx] = SynthVoice {
            active: true,
            gate: true,
            note,
            velocity: vel_norm,
            freq_hz: freq,
            phase: 0.0,
            mod_phase: 0.0,
            supersaw_phases: SUPERSAW_PHASE_INITS,
            amp_env: 0.01,
            filter: self.voices[idx].filter,
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
            if let Some(prev) = self.arp_active_note.take() {
                for v in &mut self.voices {
                    if v.active && v.note == prev {
                        v.gate = false;
                    }
                }
            }
            self.arp_step = 0;
            return;
        }

        // Arp speed: 16th note division at arp_rate_bpm
        let sixteenth_sec = 60.0 / (self.arp_rate_bpm.max(30.0) * 4.0);
        self.arp_sample_counter += inv_fs;

        if self.arp_sample_counter >= sixteenth_sec {
            self.arp_sample_counter -= sixteenth_sec;

            // Release previous arp note
            if let Some(prev) = self.arp_active_note.take() {
                for v in &mut self.voices {
                    if v.active && v.note == prev {
                        v.gate = false;
                    }
                }
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
                ArpMode::Random | ArpMode::Poly | ArpMode::Off => {
                    ((step * 7) % count, ((step * 3) % oct_range) as u8)
                }
            };

            let next_note = (held_list[note_in_chord] + oct_offset * 12).min(127);
            self.arp_active_note = Some(next_note);
            self.arp_step = (self.arp_step + 1) % total_steps;

            // Trigger internal voice
            let freq = 440.0 * 2.0f32.powf((f32::from(next_note) - 69.0) / 12.0);
            self.voices[0] = SynthVoice {
                active: true,
                gate: true,
                note: next_note,
                velocity: 0.9,
                freq_hz: freq,
                phase: 0.0,
                mod_phase: 0.0,
                supersaw_phases: SUPERSAW_PHASE_INITS,
                amp_env: 0.05,
                filter: self.voices[0].filter,
            };
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

            let eff_freq = v.freq_hz * pitch_mod;
            let dt = (eff_freq * inv_fs).clamp(1e-5, 0.49);
            let t = v.phase;

            let raw_osc = match self.osc_mode {
                SynthOscMode::Analog | SynthOscMode::Sample => match self.waveform {
                    VaWaveform::Saw => (2.0 * t - 1.0) - poly_blep(t, dt),
                    VaWaveform::SuperSaw => {
                        let mut sum = 0.0f32;
                        let detune_scale = self.supersaw_detune.clamp(0.0, 1.0);
                        for (i, &det) in SUPERSAW_DETUNES.iter().enumerate() {
                            let f_osc = eff_freq * (1.0 + det * detune_scale * 0.08);
                            let dt_i = (f_osc * inv_fs).clamp(1e-5, 0.49);
                            let p_i = v.supersaw_phases[i];
                            let saw = (2.0 * p_i - 1.0) - poly_blep(p_i, dt_i);
                            sum += saw;
                            v.supersaw_phases[i] = (p_i + dt_i) % 1.0;
                        }
                        sum * (1.0 / 7.0)
                    }
                    VaWaveform::Pulse => {
                        let mut pw = self.pulse_width.clamp(0.05, 0.95);
                        if self.lfo_dest == LfoDestination::OscCtrl {
                            pw = (pw + self.lfo_amount * lfo_val * 0.4).clamp(0.05, 0.95);
                        }
                        let base = if t < pw { 1.0 } else { -1.0 };
                        base + poly_blep(t, dt) - poly_blep((t + 1.0 - pw) % 1.0, dt)
                    }
                    VaWaveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
                    VaWaveform::Sine => (TAU * t).sin(),
                },
                SynthOscMode::Wavetable => {
                    let pos = self.wavetable_position.clamp(0.0, 1.0);
                    let wave1 = (TAU * t).sin()
                        + 0.5 * (2.0 * TAU * t).sin()
                        + 0.25 * (3.0 * TAU * t).cos();
                    let wave2 = 0.6 * (TAU * t).sin()
                        + 0.4 * (4.0 * TAU * t).sin()
                        + 0.3 * (7.0 * TAU * t).sin();
                    (1.0 - pos) * wave1 + pos * wave2
                }
                SynthOscMode::Fm => {
                    let mod_sig = (TAU * v.mod_phase).sin();
                    (TAU * t + 4.0 * self.fm_amount * mod_sig).sin()
                }
            };

            v.phase = (t + dt) % 1.0;
            v.mod_phase = (v.mod_phase + 2.0 * dt) % 1.0;

            let filtered = v.filter.process(
                raw_osc,
                self.filter_model,
                eff_cutoff,
                self.resonance,
                self.filter_drive,
                self.sample_rate,
            );

            mix += filtered * v.amp_env * v.velocity * 0.25;
        }

        [mix, mix]
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
}
