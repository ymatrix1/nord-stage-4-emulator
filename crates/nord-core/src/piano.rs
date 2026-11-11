//! 2-Layer Piano Engine & Modal Sympathetic String Resonance (`DSPF`).
//!
//! Spec: `docs/specs/piano-engine.md`

use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Maximum simultaneous polyphonic voices per Piano layer (`16` pre-allocated in core; scalable to 120).
pub const MAX_PIANO_VOICES: usize = 24;

/// Number of chromatic resonant sympathetic strings (12 octave-distributed strings across the keyboard).
pub const NUM_SYMPATHETIC_STRINGS: usize = 12;

/// Piano instrument categories on the Nord Stage 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PianoCategory {
    Grand,
    Upright,
    Electric,
    Clav,
    Digital,
    Misc,
}

/// Keyboard velocity curve (`KBD TOUCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VelocityTouch {
    Light,
    Medium,
    Heavy,
}

impl VelocityTouch {
    /// Maps incoming MIDI velocity (`1..=127`) to normalized excitation (`0.0..=1.0`).
    pub fn map_velocity(self, velocity: u8) -> f32 {
        let v = (f32::from(velocity.clamp(1, 127))) / 127.0;
        match self {
            Self::Light => v.powf(0.65),
            Self::Medium => v,
            Self::Heavy => v.powf(1.55),
        }
    }
}

/// Timbre spectral shaping presets (`PIANO TIMBRE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PianoTimbre {
    Off,
    Soft,
    Mid,
    Bright,
    Dyno1,
    Dyno2,
}

#[derive(Debug, Clone, Copy, Default)]
struct PianoVoice {
    active: bool,
    sustained: bool,
    note: u8,
    freq: f32,
    phase: f32,
    amp: f32,
    env: f32,
    gain: f32,
    released: bool,
}

/// 2-pole modal harmonic string resonator for Advanced String Resonance.
#[derive(Debug, Clone, Copy, Default)]
struct ModalResonator {
    y1: f32,
    y2: f32,
    cos_w: f32,
    sin_w: f32,
    r: f32,
}

impl ModalResonator {
    fn configure(&mut self, freq_hz: f32, t60_sec: f32, sample_rate: f32) {
        let w = TAU * (freq_hz / sample_rate).clamp(0.001, 0.45);
        self.cos_w = w.cos();
        self.sin_w = w.sin();
        self.r = (-6.907755 / (t60_sec * sample_rate))
            .exp()
            .clamp(0.9, 0.99995);
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = (1.0 - self.r * self.r) * self.sin_w * x + 2.0 * self.r * self.cos_w * self.y1
            - self.r * self.r * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Single Piano Layer (`Piano A` or `Piano B`) with modal sympathetic string resonance and pedal noise.
#[derive(Debug, Clone)]
pub struct PianoLayer {
    pub sample_rate: f32,
    pub category: PianoCategory,
    pub touch: VelocityTouch,
    pub timbre: PianoTimbre,
    pub string_resonance_enabled: bool,
    pub pedal_noise_enabled: bool,
    pub damper_pedal_down: bool,
    voices: [PianoVoice; MAX_PIANO_VOICES],
    resonators: [ModalResonator; NUM_SYMPATHETIC_STRINGS],
    undamped_keys: [bool; 128],
    pedal_noise_env: f32,
    timbre_lp: f32,
}

impl PianoLayer {
    #[cfg(test)]
    pub(crate) fn voice_excitation_for_test(&self, note: u8) -> Option<(f32, f32)> {
        self.voices
            .iter()
            .find(|v| v.active && v.note == note)
            .map(|v| (v.amp, v.gain))
    }

    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        let mut resonators = [ModalResonator::default(); NUM_SYMPATHETIC_STRINGS];
        // Tune 12 sympathetic resonance modes across the keyboard (C2 up to B7)
        for (i, res) in resonators.iter_mut().enumerate() {
            let midi_note = 24.0 + (i as f32) * 7.0;
            let f = 440.0 * 2.0f32.powf((midi_note - 69.0) / 12.0);
            let t60 = (5.0 - 0.15 * i as f32).clamp(1.5, 5.0);
            res.configure(f, t60, fs);
        }
        Self {
            sample_rate: fs,
            category: PianoCategory::Grand,
            touch: VelocityTouch::Medium,
            timbre: PianoTimbre::Off,
            string_resonance_enabled: true,
            pedal_noise_enabled: true,
            damper_pedal_down: false,
            voices: [PianoVoice::default(); MAX_PIANO_VOICES],
            resonators,
            undamped_keys: [false; 128],
            pedal_noise_env: 0.0,
            timbre_lp: 0.0,
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on_with_gain(note, velocity, 1.0);
    }

    pub fn note_on_with_gain(&mut self, note: u8, velocity: u8, gain: f32) {
        let amp = self.touch.map_velocity(velocity);
        let freq = 440.0 * 2.0f32.powf((f32::from(note) - 69.0) / 12.0);
        let note_idx = (note as usize).min(127);
        self.undamped_keys[note_idx] = true;
        // Reuse existing note or find first free slot (else voice 0)
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or(0);
        self.voices[idx] = PianoVoice {
            active: true,
            sustained: false,
            note,
            freq,
            phase: 0.0,
            amp,
            env: 1.0,
            gain: gain.clamp(0.0, 1.0),
            released: false,
        };
    }

    pub fn note_off(&mut self, note: u8) {
        let note_idx = (note as usize).min(127);
        self.undamped_keys[note_idx] = false;
        for v in &mut self.voices {
            if v.active && v.note == note {
                if self.damper_pedal_down {
                    v.sustained = true;
                } else {
                    v.released = true;
                }
            }
        }
    }

    pub fn set_damper_pedal(&mut self, down: bool, velocity: u8) {
        if down != self.damper_pedal_down {
            self.damper_pedal_down = down;
            if self.pedal_noise_enabled {
                self.pedal_noise_env = (f32::from(velocity.max(32)) / 127.0) * 0.12;
            }
            if !down {
                for v in &mut self.voices {
                    if v.sustained {
                        v.released = true;
                        v.sustained = false;
                    }
                }
            }
        }
    }

    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        let mut sum = 0.0f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            // Harmonic piano wave model (fundamental + 2nd + 3rd partials with velocity brightness)
            let p = v.phase;
            let sig =
                p.sin() + 0.45 * v.amp * (2.0 * p).sin() + 0.20 * v.amp * v.amp * (3.0 * p).sin();
            sum += sig * v.amp * v.env * v.gain * 0.2;

            v.phase = (p + TAU * v.freq * inv_fs) % TAU;
            let decay_sec = if v.released { 0.12 } else { 1.8 };
            v.env *= (-inv_fs / decay_sec).exp();
            if v.env < 1e-4 {
                v.active = false;
            }
        }

        // Dynamic pedal thump transient
        if self.pedal_noise_env > 1e-5 {
            sum += (self.pedal_noise_env * 280.0).sin() * self.pedal_noise_env;
            self.pedal_noise_env *= (-inv_fs / 0.045).exp();
        }

        // Timbre spectral tilt
        self.timbre_lp += 0.18 * (sum - self.timbre_lp);
        let high = sum - self.timbre_lp;
        let shaped = match self.timbre {
            PianoTimbre::Off => sum,
            PianoTimbre::Soft => self.timbre_lp + 0.4 * high,
            PianoTimbre::Mid => sum + 0.25 * self.timbre_lp,
            PianoTimbre::Bright | PianoTimbre::Dyno1 | PianoTimbre::Dyno2 => sum + 0.65 * high,
        };

        // Sympathetic string resonance: fires either across all 12 strings when damper pedal is down,
        // or specifically across undamped held keys (Nord Advanced String Resonance Gen 2)
        let mut res_sum = 0.0f32;
        if self.string_resonance_enabled {
            if self.damper_pedal_down {
                for r in &mut self.resonators {
                    res_sum += r.process(shaped * 0.008);
                }
            } else {
                for (i, r) in self.resonators.iter_mut().enumerate() {
                    let midi_note = 24 + i * 7;
                    if midi_note < 128 && self.undamped_keys[midi_note] {
                        res_sum += r.process(shaped * 0.015);
                    }
                }
            }
        }

        let left = shaped + res_sum;
        let right = shaped - res_sum * 0.5;
        [left, right]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_touch_ordering() {
        let light = VelocityTouch::Light.map_velocity(64);
        let medium = VelocityTouch::Medium.map_velocity(64);
        let heavy = VelocityTouch::Heavy.map_velocity(64);
        assert!(heavy < medium && medium < light);
    }

    #[test]
    fn test_piano_string_resonance_and_sustain_pedal() {
        let mut piano = PianoLayer::new(48_000.0);
        piano.set_damper_pedal(true, 100);
        piano.note_on(60, 110);
        piano.note_off(60);
        // Voice stays sustained while damper pedal is down
        let mut energy = 0.0f32;
        for _ in 0..256 {
            let [l, r] = piano.render_frame();
            assert!(l.is_finite() && r.is_finite());
            energy += l * l + r * r;
        }
        assert!(energy > 0.005);

        piano.set_damper_pedal(false, 64);
        assert!(piano.voices.iter().any(|v| v.active && v.released));
    }

    #[test]
    fn note_off_starts_release_instead_of_cutting_voice() {
        let mut piano = PianoLayer::new(48_000.0);
        piano.note_on(60, 100);
        piano.note_off(60);
        assert!(piano.voices.iter().any(|v| v.active && v.released));
        for _ in 0..96_000 {
            piano.render_frame();
        }
        assert!(piano.voices.iter().all(|v| !v.active));
    }
}
