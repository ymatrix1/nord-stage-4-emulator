//! 2-Layer Physical Modeling Organ Engine (`DSPO` — 91 Tonewheels + Model 122 Rotary Speaker).
//!
//! Spec: `docs/specs/organ-engine.md`. Submodules: `types` (enums, constants), `drawbar` (bus
//! mapping, firmware level curve), `firmware_bank` (PNO Core1 oscillator-bank arithmetic),
//! `vibrato_bank` (12-TET ladder, detune table, group gains), `tonewheel` (91-wheel
//! generator), `keyclick`, `scanner`, `rotary`.

mod drawbar;
mod firmware_bank;
mod keyclick;
mod rotary;
mod scanner;
mod tonewheel;
mod types;
mod vibrato_bank;

#[cfg(test)]
mod bank_golden;

pub use drawbar::{drawbar_gain, drawbar_gain_law, DrawbarLaw, FIRMWARE_DRAWBAR_CURVE};
pub use firmware_bank::{build_x_table, firmware_wave_x, fracmul, phase_step, ToneBank};
pub use types::{
    OrganModel, RotarySpeed, ScannerMode, ToneWheelSource, HAMMOND_GEAR_RATIOS, NUM_TONEWHEELS,
    ROTARY_BASS_HORN_BALANCES, ROTARY_CROSSOVER_HZ,
};
pub use vibrato_bank::{wheel_hz, DETUNE_CENTS, GAINS_DETUNED, GAINS_EXACT};

use drawbar::{foldback_tonewheel, DRAWBAR_OFFSETS};
use std::f32::consts::TAU;
use types::{ROTARY_BUFFER_SIZE, SCANNER_BUFFER_SIZE};

/// Physical modeling B3 & Transistor/Pipe Organ layer with integrated Model 122 Rotary Speaker.
#[derive(Debug, Clone)]
pub struct OrganLayer {
    pub sample_rate: f32,
    pub model: OrganModel,
    /// When `true` on `OrganModel::B3`, engages Nord's B3 Bass pickup calibration curve.
    pub b3_bass_mode: bool,
    /// Keyboard octave transposition (`-2..=+2`).
    pub octave_shift: i8,
    /// 9 drawbar settings (`0..=8`). Default `88 8000 000`.
    pub drawbars: [u8; 9],
    pub percussion_enabled: bool,
    pub percussion_third_harmonic: bool,
    pub percussion_fast_decay: bool,
    pub percussion_soft: bool,
    /// Transformer compartment crosstalk intensity (`1.0` = nominal -42 dB B3 bleed).
    pub crosstalk_level: f32,
    pub rotary_enabled: bool,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    /// Rotary microphone placement (`false` = Normal distance with cabinet reflections, `true` = Close mic).
    pub rotary_close_mic: bool,
    /// Stereo virtual microphone separation angle in degrees (`0.0..=180.0`, default `120.0`).
    pub rotary_mic_angle_deg: f32,
    /// When `true`, engages brake/stop mode regardless of `rotary_speed`.
    pub rotary_stop_mode: bool,
    pub scanner_mode: ScannerMode,
    /// `B3` tonewheel waveform and phase arithmetic (default `Firmware`, role ASSUMED, about 70 %;
    /// `Legacy` stays selectable).
    pub tonewheel_source: ToneWheelSource,
    /// Organ Vibrato-Enable level 1: the firmware bank runs the detuned table
    /// (`DETUNE_CENTS`, +-1.9 cent rms ensemble copies) instead of the exact 12-TET one. VERIFIED
    /// table selection, default off.
    pub detuned_copies: bool,
    /// Fine Tune in cents applied to the firmware bank (clamped to -400..+381, default 0).
    pub fine_tune_cents: f32,
    /// Drawbar level law (default: firmware curve; `Linear` is the old `level / 8`).
    pub drawbar_law: DrawbarLaw,
    tonewheel_freqs: [f32; NUM_TONEWHEELS],
    tonewheel_freqs_hz: [f64; NUM_TONEWHEELS],
    synced_fine_cents: f32,
    tone_bank: ToneBank,
    tonewheel_phases: [f32; NUM_TONEWHEELS],
    pickup_b3: [f32; NUM_TONEWHEELS],
    pickup_b3_bass: [f32; NUM_TONEWHEELS],
    held_notes: [bool; 128],
    note_gains: [f32; 128],
    active_key_count: usize,
    percussion_env: f32,
    click_env: f32,
    click_bp_state: [f32; 2],
    rng_state: u32,
    horn_phase: f32,
    drum_phase: f32,
    horn_rate_hz: f32,
    drum_rate_hz: f32,
    lp_state: f32,
    rotary_horn_buf: Box<[f32; ROTARY_BUFFER_SIZE]>,
    rotary_drum_buf: Box<[f32; ROTARY_BUFFER_SIZE]>,
    rotary_write_idx: usize,
    scanner_phase: f32,
    scanner_buffer: Box<[f32; SCANNER_BUFFER_SIZE]>,
    scanner_write_idx: usize,
    farf_bp1_state: [f32; 2],
    farf_bp2_state: [f32; 2],
}

impl OrganLayer {
    /// Initializes the 91 tonewheel frequencies and pre-allocated state for `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let mut freqs = [0.0f32; NUM_TONEWHEELS];
        let mut freqs_hz = [0.0f64; NUM_TONEWHEELS];
        for (i, (f, f64hz)) in freqs.iter_mut().zip(freqs_hz.iter_mut()).enumerate() {
            *f = Self::tonewheel_gear_frequency(i);
            *f64hz = wheel_hz(i, 0.0);
        }
        let (pickup_b3, pickup_b3_bass) = Self::pickup_tables();
        let sample_rate = sample_rate.max(8000.0);
        let mut tone_bank = ToneBank::new();
        tone_bank.retune(&freqs_hz, sample_rate, false);

        Self {
            sample_rate,
            model: OrganModel::B3,
            b3_bass_mode: false,
            octave_shift: 0,
            drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            percussion_enabled: true,
            percussion_third_harmonic: false,
            percussion_fast_decay: true,
            percussion_soft: false,
            crosstalk_level: 1.0,
            rotary_enabled: true,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.2,
            rotary_close_mic: false,
            rotary_mic_angle_deg: 120.0,
            rotary_stop_mode: false,
            scanner_mode: ScannerMode::Off,
            tonewheel_source: ToneWheelSource::default(),
            detuned_copies: false,
            fine_tune_cents: 0.0,
            drawbar_law: DrawbarLaw::default(),
            tonewheel_freqs: freqs,
            tonewheel_freqs_hz: freqs_hz,
            synced_fine_cents: 0.0,
            tone_bank,
            tonewheel_phases: [0.0; NUM_TONEWHEELS],
            pickup_b3,
            pickup_b3_bass,
            held_notes: [false; 128],
            note_gains: [1.0; 128],
            active_key_count: 0,
            percussion_env: 0.0,
            click_env: 0.0,
            click_bp_state: [0.0; 2],
            rng_state: 0x6D2B_79F5,
            horn_phase: 0.0,
            drum_phase: 0.0,
            horn_rate_hz: 0.80,
            drum_rate_hz: 0.67,
            lp_state: 0.0,
            rotary_horn_buf: Box::new([0.0; ROTARY_BUFFER_SIZE]),
            rotary_drum_buf: Box::new([0.0; ROTARY_BUFFER_SIZE]),
            rotary_write_idx: 0,
            scanner_phase: 0.0,
            scanner_buffer: Box::new([0.0; SCANNER_BUFFER_SIZE]),
            scanner_write_idx: 0,
            farf_bp1_state: [0.0; 2],
            farf_bp2_state: [0.0; 2],
        }
    }

    /// Triggers a key-down event with single-trigger B3 percussion and key-click transient.
    pub fn note_on(&mut self, note: u8) {
        self.note_on_with_gain(note, 1.0);
    }

    pub fn note_on_with_gain(&mut self, note: u8, gain: f32) {
        let idx = (note as usize).min(127);
        if !self.held_notes[idx] {
            if self.active_key_count == 0 && self.percussion_enabled {
                self.percussion_env = 1.0;
            }
            self.held_notes[idx] = true;
            self.note_gains[idx] = gain.clamp(0.0, 1.0);
            self.active_key_count += 1;
            let click_scale = match self.model {
                OrganModel::B3 | OrganModel::Vox => 0.45,
                OrganModel::Farfisa => 0.18,
                OrganModel::Pipe1 | OrganModel::Pipe2 => 0.25,
            };
            self.click_env = click_scale * self.note_gains[idx].max(0.25);
        }
    }

    /// Releases a held key.
    pub fn note_off(&mut self, note: u8) {
        let idx = (note as usize).min(127);
        if self.held_notes[idx] {
            self.held_notes[idx] = false;
            self.active_key_count = self.active_key_count.saturating_sub(1);
            let click_scale = match self.model {
                OrganModel::B3 | OrganModel::Vox => 0.16,
                OrganModel::Farfisa => 0.06,
                OrganModel::Pipe1 | OrganModel::Pipe2 => 0.05,
            };
            self.click_env = self.click_env.max(click_scale);
        }
    }

    /// Renders a single stereo frame `[left, right]` with zero heap allocations.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;

        // 1. Advance all 91 spinning tonewheels continuously
        let tw_signals = self.render_tonewheels(inv_fs);

        if self.active_key_count == 0
            && self.click_env < 1e-6
            && self.click_bp_state[0].abs() < 1e-6
        {
            return [0.0, 0.0];
        }

        // 2. Sum active drawbars across held notes (with B3 Normal percussion -3 dB drawbar dip)
        let oct_semis = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        let db_bus_scale = if self.model == OrganModel::B3
            && self.percussion_enabled
            && !self.percussion_soft
            && self.percussion_env > 1e-4
        {
            0.708
        } else {
            1.0
        };

        let mut dry = 0.0f32;
        for note in 0u8..=127u8 {
            if !self.held_notes[note as usize] {
                continue;
            }
            let base_midi = i32::from(note) + oct_semis;
            let key_gain = self.note_gains[note as usize];
            for (db_idx, &level) in self.drawbars.iter().enumerate() {
                if level == 0 {
                    continue;
                }
                let gain = drawbar_gain_law(level, self.drawbar_law);
                let tw = foldback_tonewheel(base_midi + DRAWBAR_OFFSETS[db_idx]);
                dry += gain * tw_signals[tw] * key_gain * db_bus_scale * 0.08;
            }
            if self.percussion_env > 1e-4 {
                let perc_offset = if self.percussion_third_harmonic {
                    19
                } else {
                    12
                };
                let tw = foldback_tonewheel(base_midi + perc_offset);
                let perc_gain = if self.percussion_soft { 0.06 } else { 0.12 };
                dry += self.percussion_env * tw_signals[tw] * perc_gain * key_gain;
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

        // 4. Palladium key-click bandpass transient & percussion decay
        dry += self.process_key_click(inv_fs);
        let perc_tau = if self.percussion_fast_decay {
            0.18
        } else {
            0.55
        };
        self.percussion_env *= (-inv_fs / perc_tau).exp();

        // 5. Hammond Scanner Vibrato & Chorus (V1-V3, C1-C3)
        let scanner_out = self.process_scanner(dry, inv_fs);

        if !self.rotary_enabled {
            return [scanner_out, scanner_out];
        }

        // 6. Model 122 Rotary Speaker
        self.process_rotary(scanner_out, inv_fs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_trigger_percussion_and_stereo_rotary() {
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
    fn scanner_vibrato_and_models_stay_dual_mono_without_rotary() {
        let mut organ = OrganLayer::new(48_000.0);
        organ.scanner_mode = ScannerMode::C3;
        organ.model = OrganModel::Farfisa;
        organ.rotary_enabled = false;
        organ.note_on(60);
        for _ in 0..1024 {
            let [l, r] = organ.render_frame();
            assert!(l.is_finite() && r.is_finite());
            assert_eq!(l, r);
        }
    }

    #[test]
    fn firmware_source_is_default_and_all_sources_render_bounded_audio() {
        for (source, copies) in [
            (ToneWheelSource::Firmware, false),
            (ToneWheelSource::Firmware, true),
            (ToneWheelSource::Legacy, false),
        ] {
            let mut o = OrganLayer::new(48_000.0);
            o.tonewheel_source = source;
            o.detuned_copies = copies;
            o.note_on(48);
            o.note_on(55);
            let mut peak = 0.0f32;
            for _ in 0..24_000 {
                let [l, r] = o.render_frame();
                assert!(l.is_finite() && r.is_finite());
                peak = peak.max(l.abs()).max(r.abs());
            }
            assert!(
                peak > 0.01 && peak < 2.0,
                "{source:?} {copies}: peak {peak}"
            );
        }
        assert_eq!(
            OrganLayer::new(48_000.0).tonewheel_source,
            ToneWheelSource::Firmware
        );
    }

    #[test]
    fn drawbar_law_changes_level_of_a_half_open_bar() {
        let render = |law: DrawbarLaw| {
            let mut o = OrganLayer::new(48_000.0);
            o.drawbar_law = law;
            o.drawbars = [0, 0, 4, 0, 0, 0, 0, 0, 0];
            o.percussion_enabled = false;
            o.rotary_enabled = false;
            o.note_on(60);
            let mut e = 0.0f32;
            for _ in 0..9600 {
                let [l, _] = o.render_frame();
                e += l * l;
            }
            e
        };
        let (fw, lin) = (render(DrawbarLaw::Firmware), render(DrawbarLaw::Linear));
        // 22/127 against 4/8: -9.2 dB amplitude (power ratio about 0.12, key click aside)
        assert!(fw < 0.3 * lin && fw > 0.0, "{fw} vs {lin}");
    }
}
