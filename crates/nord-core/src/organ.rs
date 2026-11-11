//! 2-Layer Physical Modeling Organ Engine (`DSPO` — 91 Tonewheels + Model 122 Rotary Speaker).
//!
//! Spec: `docs/specs/organ-engine.md`

use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Number of physical tonewheels in a Hammond B3 generator (`1..=91`).
pub const NUM_TONEWHEELS: usize = 91;

/// Maximum delay buffer capacity in samples for Hammond scanner vibrato (~10 ms at 192 kHz).
const SCANNER_BUFFER_SIZE: usize = 2048;

/// Supported Organ models in the Nord Stage 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganModel {
    B3,
    Vox,
    Farfisa,
    Pipe1,
    Pipe2,
}

/// Hammond mechanical scanner vibrato and chorus modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScannerMode {
    #[default]
    Off,
    V1,
    V2,
    V3,
    C1,
    C2,
    C3,
}

/// Rotary speaker speed state (`Slow` Chorale vs `Fast` Tremolo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotarySpeed {
    Stop,
    Slow,
    Fast,
}

/// Semitone offsets relative to fundamental (`8'`) for the 9 drawbar buses.
const DRAWBAR_OFFSETS: [i32; 9] = [-12, 7, 0, 12, 19, 24, 28, 31, 36];

/// Physical modeling B3 & Transistor/Pipe Organ layer with integrated Model 122 Rotary Speaker.
#[derive(Debug, Clone)]
pub struct OrganLayer {
    pub sample_rate: f32,
    pub model: OrganModel,
    /// 9 drawbar settings (`0..=8`). Default `88 8000 000`.
    pub drawbars: [u8; 9],
    pub percussion_enabled: bool,
    pub percussion_third_harmonic: bool,
    pub percussion_fast_decay: bool,
    pub percussion_soft: bool,
    pub rotary_enabled: bool,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    pub scanner_mode: ScannerMode,
    tonewheel_freqs: [f32; NUM_TONEWHEELS],
    tonewheel_phases: [f32; NUM_TONEWHEELS],
    held_notes: [bool; 128],
    active_key_count: usize,
    percussion_env: f32,
    click_env: f32,
    horn_phase: f32,
    drum_phase: f32,
    horn_rate_hz: f32,
    drum_rate_hz: f32,
    lp_state: f32,
    scanner_phase: f32,
    scanner_buffer: Box<[f32; SCANNER_BUFFER_SIZE]>,
    scanner_write_idx: usize,
    farf_bp1_state: [f32; 2],
    farf_bp2_state: [f32; 2],
}

impl OrganLayer {
    /// Initializes the 91 tonewheel frequencies and pre-allocated state for `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        // Authentic Hammond gear ratios relative to A4 = 440.0 Hz (tonewheel #58 = A4)
        let mut freqs = [0.0f32; NUM_TONEWHEELS];
        for (i, freq) in freqs.iter_mut().enumerate() {
            // Tonewheel 1 corresponds to C1 (MIDI note 24, 32.703 Hz)
            let semitones_from_a4 = (i as f32 + 24.0) - 69.0;
            *freq = 440.0 * 2.0f32.powf(semitones_from_a4 / 12.0);
        }

        Self {
            sample_rate: sample_rate.max(8000.0),
            model: OrganModel::B3,
            drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            percussion_enabled: true,
            percussion_third_harmonic: false,
            percussion_fast_decay: true,
            percussion_soft: false,
            rotary_enabled: true,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.2,
            scanner_mode: ScannerMode::Off,
            tonewheel_freqs: freqs,
            tonewheel_phases: [0.0; NUM_TONEWHEELS],
            held_notes: [false; 128],
            active_key_count: 0,
            percussion_env: 0.0,
            click_env: 0.0,
            horn_phase: 0.0,
            drum_phase: 0.0,
            horn_rate_hz: 0.80,
            drum_rate_hz: 0.67,
            lp_state: 0.0,
            scanner_phase: 0.0,
            scanner_buffer: Box::new([0.0; SCANNER_BUFFER_SIZE]),
            scanner_write_idx: 0,
            farf_bp1_state: [0.0; 2],
            farf_bp2_state: [0.0; 2],
        }
    }

    /// Triggers a key-down event with single-trigger B3 percussion and key-click transient.
    pub fn note_on(&mut self, note: u8) {
        let idx = (note as usize).min(127);
        if !self.held_notes[idx] {
            if self.active_key_count == 0 && self.percussion_enabled {
                self.percussion_env = 1.0;
            }
            self.held_notes[idx] = true;
            self.active_key_count += 1;
            self.click_env = 0.25;
        }
    }

    /// Releases a held key.
    pub fn note_off(&mut self, note: u8) {
        let idx = (note as usize).min(127);
        if self.held_notes[idx] {
            self.held_notes[idx] = false;
            self.active_key_count = self.active_key_count.saturating_sub(1);
            self.click_env = 0.08;
        }
    }

    /// Maps a target MIDI pitch with B3 top/bottom octave foldback into tonewheel `0..90`.
    fn foldback_tonewheel(target_midi: i32) -> usize {
        let mut m = target_midi;
        while m < 24 {
            m += 12;
        }
        while m > 24 + (NUM_TONEWHEELS as i32 - 1) {
            m -= 12;
        }
        (m - 24).clamp(0, NUM_TONEWHEELS as i32 - 1) as usize
    }

    /// Renders a single stereo frame `[left, right]` with zero heap allocations.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;

        // 1. Advance all 91 spinning tonewheels continuously
        let mut tw_signals = [0.0f32; NUM_TONEWHEELS];
        for (i, sig) in tw_signals.iter_mut().enumerate() {
            let p = self.tonewheel_phases[i];
            *sig = match self.model {
                OrganModel::B3 | OrganModel::Pipe1 => p.sin(),
                OrganModel::Vox | OrganModel::Farfisa | OrganModel::Pipe2 => {
                    0.7 * p.sin() + 0.3 * (2.0 * p).sin()
                }
            };
            let next_p = p + TAU * self.tonewheel_freqs[i] * inv_fs;
            self.tonewheel_phases[i] = if next_p >= TAU { next_p - TAU } else { next_p };
        }

        if self.active_key_count == 0 && self.click_env < 1e-5 {
            return [0.0, 0.0];
        }

        // 2. Sum active drawbars across held notes
        let mut dry = 0.0f32;
        for note in 24u8..=108u8 {
            if !self.held_notes[note as usize] {
                continue;
            }
            for (db_idx, &level) in self.drawbars.iter().enumerate() {
                if level == 0 {
                    continue;
                }
                let gain = f32::from(level.min(8)) / 8.0;
                let tw = Self::foldback_tonewheel(i32::from(note) + DRAWBAR_OFFSETS[db_idx]);
                dry += gain * tw_signals[tw] * 0.08;
            }
            if self.percussion_env > 1e-4 {
                let perc_offset = if self.percussion_third_harmonic {
                    19
                } else {
                    12
                };
                let tw = Self::foldback_tonewheel(i32::from(note) + perc_offset);
                let perc_gain = if self.percussion_soft { 0.06 } else { 0.12 };
                dry += self.percussion_env * tw_signals[tw] * perc_gain;
            }
        }

        // 3. Model filter shaping (Vox Continental Tone, Farfisa Formants, Pipe Chiff)
        match self.model {
            OrganModel::B3 => {}
            OrganModel::Vox => {
                // Vox drawbar 9 acts as Wave/Tone control morphing between flute and reed
                let tone = f32::from(self.drawbars[8].min(8)) / 8.0;
                dry = (1.0 - 0.5 * tone) * dry + tone * dry.signum() * dry.abs().powf(0.8);
            }
            OrganModel::Farfisa => {
                // Farfisa dual resonant bandpass formant peaks (750 Hz and 2.5 kHz)
                let w1 = (TAU * 750.0 * inv_fs).clamp(0.01, 0.45);
                let w2 = (TAU * 2500.0 * inv_fs).clamp(0.01, 0.45);
                self.farf_bp1_state[0] +=
                    w1 * (dry - self.farf_bp1_state[0] - 0.2 * self.farf_bp1_state[1]);
                self.farf_bp1_state[1] += w1 * self.farf_bp1_state[0];
                self.farf_bp2_state[0] +=
                    w2 * (dry - self.farf_bp2_state[0] - 0.3 * self.farf_bp2_state[1]);
                self.farf_bp2_state[1] += w2 * self.farf_bp2_state[0];
                dry = 0.6 * dry + 0.3 * self.farf_bp1_state[0] + 0.25 * self.farf_bp2_state[0];
            }
            OrganModel::Pipe1 | OrganModel::Pipe2 => {
                // Pipe gentle high-frequency damping
                dry *= 0.92;
            }
        }

        // 4. Key click transient & percussion decay
        dry += self.click_env;
        self.click_env *= (-inv_fs / 0.0025).exp();
        let perc_tau = if self.percussion_fast_decay {
            0.18
        } else {
            0.55
        };
        self.percussion_env *= (-inv_fs / perc_tau).exp();

        // 5. Hammond Scanner Vibrato & Chorus (V1-V3, C1-C3)
        let scanner_out = if self.scanner_mode == ScannerMode::Off {
            dry
        } else {
            let (depth_ms, mix_wet) = match self.scanner_mode {
                ScannerMode::Off => (0.0, 0.0),
                ScannerMode::V1 => (0.75, 1.0),
                ScannerMode::V2 => (1.50, 1.0),
                ScannerMode::V3 => (2.50, 1.0),
                ScannerMode::C1 => (0.75, 0.5),
                ScannerMode::C2 => (1.50, 0.5),
                ScannerMode::C3 => (2.50, 0.5),
            };

            // Scanner rotary rate is ~6.85 Hz
            self.scanner_phase = (self.scanner_phase + TAU * 6.85 * inv_fs) % TAU;
            let mod_delay_samples =
                (depth_ms * 0.001 * self.sample_rate) * (0.5 + 0.5 * self.scanner_phase.sin());

            // Write into ring buffer
            self.scanner_buffer[self.scanner_write_idx] = dry;
            let read_pos = (self.scanner_write_idx as f32 - mod_delay_samples
                + SCANNER_BUFFER_SIZE as f32)
                % (SCANNER_BUFFER_SIZE as f32);
            self.scanner_write_idx = (self.scanner_write_idx + 1) % SCANNER_BUFFER_SIZE;

            // Linear interpolation
            let idx0 = read_pos.floor() as usize % SCANNER_BUFFER_SIZE;
            let idx1 = (idx0 + 1) % SCANNER_BUFFER_SIZE;
            let frac = read_pos - read_pos.floor();
            let delayed =
                self.scanner_buffer[idx0] * (1.0 - frac) + self.scanner_buffer[idx1] * frac;

            (1.0 - mix_wet) * dry + mix_wet * delayed
        };

        if !self.rotary_enabled {
            return [scanner_out, scanner_out];
        }

        // 6. Model 122 Rotary Speaker (Tube drive + 800 Hz crossover + Horn/Drum Doppler)
        let driven = ((1.0 + 3.0 * self.rotary_drive) * scanner_out).tanh();
        let alpha = (TAU * 800.0 * inv_fs).clamp(0.01, 0.99);
        self.lp_state += alpha * (driven - self.lp_state);
        let bass = self.lp_state;
        let treble = driven - bass;

        let (target_horn, target_drum) = match self.rotary_speed {
            RotarySpeed::Stop => (0.0, 0.0),
            RotarySpeed::Slow => (0.80, 0.67),
            RotarySpeed::Fast => (6.85, 5.70),
        };
        self.horn_rate_hz += (target_horn - self.horn_rate_hz) * (inv_fs / 0.7).min(1.0);
        self.drum_rate_hz += (target_drum - self.drum_rate_hz) * (inv_fs / 4.2).min(1.0);

        self.horn_phase = (self.horn_phase + TAU * self.horn_rate_hz * inv_fs) % TAU;
        self.drum_phase = (self.drum_phase + TAU * self.drum_rate_hz * inv_fs) % TAU;

        let left = treble * (0.7 + 0.3 * self.horn_phase.sin())
            + bass * (0.75 + 0.25 * self.drum_phase.sin());
        let right = treble * (0.7 + 0.3 * self.horn_phase.cos())
            + bass * (0.75 + 0.25 * self.drum_phase.cos());

        [left, right]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tonewheel_a4_frequency_and_foldback() {
        let organ = OrganLayer::new(48_000.0);
        // MIDI 69 (A4) maps to tonewheel index 69 - 24 = 45
        assert!((organ.tonewheel_freqs[45] - 440.0).abs() < 1e-3);
        // Bottom foldback: MIDI 12 folds up to MIDI 24 (index 0)
        assert_eq!(OrganLayer::foldback_tonewheel(12), 0);
        // Top foldback: MIDI 120 folds down into valid tonewheel range
        assert!(OrganLayer::foldback_tonewheel(120) < NUM_TONEWHEELS);
    }

    #[test]
    fn test_organ_single_trigger_percussion_and_stereo_rotary() {
        let mut organ = OrganLayer::new(48_000.0);
        organ.rotary_speed = RotarySpeed::Fast;
        organ.note_on(60);
        assert!((organ.percussion_env - 1.0).abs() < 1e-5);

        let mut energy = 0.0f32;
        for _ in 0..512 {
            let [l, r] = organ.render_frame();
            assert!(l.is_finite() && r.is_finite());
            energy += l * l + r * r;
        }
        assert!(energy > 0.01, "active organ note must produce audio energy");
        let decayed_perc = organ.percussion_env;
        assert!(decayed_perc < 1.0);

        // Legato second note must NOT reset percussion_env to 1.0
        organ.note_on(64);
        assert!((organ.percussion_env - decayed_perc).abs() < 1e-5);
    }

    #[test]
    fn test_scanner_vibrato_and_models() {
        let mut organ = OrganLayer::new(48_000.0);
        organ.scanner_mode = ScannerMode::C3;
        organ.model = OrganModel::Farfisa;
        organ.rotary_enabled = false;
        organ.note_on(60);

        for _ in 0..1024 {
            let [l, r] = organ.render_frame();
            assert!(l.is_finite() && r.is_finite());
            assert_eq!(l, r); // Without rotary, scanner output is dual-mono
        }
    }
}
