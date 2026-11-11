//! 3-Layer Nord Wave 2 Derived Synth Engine (`DSPS` — PolyBLEP VA + ZDF Ladder Filter).
//!
//! Spec: `docs/specs/synth-engine.md`

use std::f32::consts::{PI, TAU};

/// Oscillator synthesis modes in the Nord Stage 4 Synth Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthOscMode {
    Analog,
    Wavetable,
    Fm,
    Sample,
}

/// Virtual Analog waveform shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaWaveform {
    Saw,
    Pulse,
    Triangle,
    Sine,
}

/// Zero-Delay Feedback filter topologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterModel {
    Lp12,
    Lp24,
    Ladder24,
    BandPass,
    HighPass,
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

/// Single Synth Layer (`Synth A`, `Synth B`, or `Synth C`).
#[derive(Debug, Clone)]
pub struct SynthLayer {
    pub sample_rate: f32,
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    pub pulse_width: f32,
    pub fm_amount: f32,
    pub filter_model: FilterModel,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub filter_drive: f32,
    pub attack_sec: f32,
    pub release_sec: f32,
    active_note: Option<u8>,
    gate: bool,
    freq_hz: f32,
    phase: f32,
    mod_phase: f32,
    amp_env: f32,
    filter: ZdfFilter,
}

impl SynthLayer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(8000.0),
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::Saw,
            pulse_width: 0.5,
            fm_amount: 0.35,
            filter_model: FilterModel::Ladder24,
            cutoff_hz: 2400.0,
            resonance: 0.3,
            filter_drive: 0.2,
            attack_sec: 0.005,
            release_sec: 0.25,
            active_note: None,
            gate: false,
            freq_hz: 440.0,
            phase: 0.0,
            mod_phase: 0.0,
            amp_env: 0.0,
            filter: ZdfFilter::default(),
        }
    }

    pub fn note_on(&mut self, note: u8, _velocity: u8) {
        self.active_note = Some(note);
        self.gate = true;
        self.freq_hz = 440.0 * 2.0f32.powf((f32::from(note) - 69.0) / 12.0);
    }

    pub fn note_off(&mut self, note: u8) {
        if self.active_note == Some(note) {
            self.gate = false;
        }
    }

    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        if self.gate {
            let atk = self.attack_sec.max(0.001);
            self.amp_env += (1.0 - self.amp_env) * (inv_fs / atk).min(1.0);
        } else {
            let rel = self.release_sec.max(0.002);
            self.amp_env *= (-inv_fs / rel).exp();
            if self.amp_env < 1e-5 {
                self.amp_env = 0.0;
                return [0.0, 0.0];
            }
        }

        let dt = (self.freq_hz * inv_fs).clamp(1e-5, 0.49);
        let t = self.phase;

        let raw_osc = match self.osc_mode {
            SynthOscMode::Analog | SynthOscMode::Sample => match self.waveform {
                VaWaveform::Saw => (2.0 * t - 1.0) - poly_blep(t, dt),
                VaWaveform::Pulse => {
                    let pw = self.pulse_width.clamp(0.05, 0.95);
                    let base = if t < pw { 1.0 } else { -1.0 };
                    base + poly_blep(t, dt) - poly_blep((t + 1.0 - pw) % 1.0, dt)
                }
                VaWaveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
                VaWaveform::Sine => (TAU * t).sin(),
            },
            SynthOscMode::Wavetable => {
                (TAU * t).sin() + 0.5 * (2.0 * TAU * t).sin() + 0.25 * (3.0 * TAU * t).cos()
            }
            SynthOscMode::Fm => {
                let mod_sig = (TAU * self.mod_phase).sin();
                (TAU * t + 4.0 * self.fm_amount * mod_sig).sin()
            }
        };

        self.phase = (t + dt) % 1.0;
        self.mod_phase = (self.mod_phase + 2.0 * dt) % 1.0;

        let filtered = self.filter.process(
            raw_osc,
            self.filter_model,
            self.cutoff_hz,
            self.resonance,
            self.filter_drive,
            self.sample_rate,
        );

        let out = filtered * self.amp_env * 0.35;
        [out, out]
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
        // Feed a 12 kHz sine into a 400 Hz 24dB ZDF Ladder low-pass
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
    fn test_synth_layer_render_and_release() {
        let mut synth = SynthLayer::new(48_000.0);
        synth.note_on(69, 100);
        let mut active_energy = 0.0f32;
        for _ in 0..256 {
            let [l, r] = synth.render_frame();
            active_energy += l * l + r * r;
        }
        assert!(active_energy > 0.01);
        synth.note_off(69);
    }
}
