//! 2-Layer Physical Modeling Organ Engine (`DSPO` — 91 Tonewheels + Model 122 Rotary Speaker).
//!
//! Spec: `docs/specs/organ-engine.md`

use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Number of physical tonewheels in a Hammond B3 generator (`1..=91`).
pub const NUM_TONEWHEELS: usize = 91;

/// Maximum delay buffer capacity in samples for Hammond scanner vibrato (~10 ms at 192 kHz).
const SCANNER_BUFFER_SIZE: usize = 2048;

/// Fixed stereo delay line capacity in samples for Model 122 Doppler & cabinet reflections (~10 ms at 48 kHz).
const ROTARY_BUFFER_SIZE: usize = 1024;

/// Synchronous AC drive motor reference frequency (1200 RPM = 20 rev/sec).
const HAMMOND_MOTOR_HZ: f32 = 20.0;

/// Authentic Hammond 12-tone drive/driven integer gear tooth ratios (`C` through `B`).
///
/// Top-octave tonewheels (`tw >= 84`) use physical 192-tooth wheels paired with a 4/3 step-up gear
/// (effective `256` pulses per motor-ratio cycle).
pub const HAMMOND_GEAR_RATIOS: [(u32, u32); 12] = [
    (85, 104),  // C
    (71, 82),   // C#
    (67, 73),   // D
    (105, 108), // D# (35/36)
    (103, 100), // E
    (84, 77),   // F (12/11)
    (74, 64),   // F# (37/32)
    (98, 80),   // G (49/40)
    (96, 74),   // G# (48/37)
    (88, 64),   // A (11/8 => 440.000 Hz at A4)
    (67, 46),   // A#
    (108, 70),  // B (54/35)
];

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

/// Hardware 800 Hz rotary bass/horn crossover verified from NS4 OS v1.66 (`ARM0 @ 0x0db130`).
pub const ROTARY_CROSSOVER_HZ: f32 = 800.0;

/// Hardware Rotary Bass/Horn balance menu steps (`70/30 .. 30/70`) verified from OS v1.66 (`0x2c20d8`).
pub const ROTARY_BASS_HORN_BALANCES: [(f32, f32); 5] = [
    (0.70, 0.30),
    (0.60, 0.40),
    (0.50, 0.50),
    (0.40, 0.60),
    (0.30, 0.70),
];

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
    tonewheel_freqs: [f32; NUM_TONEWHEELS],
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
    /// Returns `(drive_teeth, driven_teeth, physical_wheel_teeth)` for tonewheel index `0..91`.
    pub fn tonewheel_gear_spec(tw_idx: usize) -> (u32, u32, u32) {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        let (drive, driven) = HAMMOND_GEAR_RATIOS[clamped % 12];
        let octave = clamped / 12;
        let wheel_teeth = if clamped >= 84 { 192 } else { 2u32 << octave };
        (drive, driven, wheel_teeth)
    }

    /// Computes exact Hammond synchronous-shaft frequency for tonewheel index `0..91`.
    pub fn tonewheel_gear_frequency(tw_idx: usize) -> f32 {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        let (drive, driven, wheel_teeth) = Self::tonewheel_gear_spec(clamped);
        let step_up = if clamped >= 84 { 4.0 / 3.0 } else { 1.0 };
        HAMMOND_MOTOR_HZ * (drive as f32 / driven as f32) * (wheel_teeth as f32) * step_up
    }

    /// Returns the active pickup calibration gain for tonewheel `tw_idx` in `B3` vs `B3 Bass` mode.
    pub fn pickup_calibration(&self, tw_idx: usize) -> f32 {
        let clamped = tw_idx.min(NUM_TONEWHEELS - 1);
        if self.b3_bass_mode {
            self.pickup_b3_bass[clamped]
        } else {
            self.pickup_b3[clamped]
        }
    }

    /// Initializes the 91 tonewheel frequencies and pre-allocated state for `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let mut freqs = [0.0f32; NUM_TONEWHEELS];
        let mut pickup_b3 = [1.0f32; NUM_TONEWHEELS];
        let mut pickup_b3_bass = [1.0f32; NUM_TONEWHEELS];

        for i in 0..NUM_TONEWHEELS {
            freqs[i] = Self::tonewheel_gear_frequency(i);
            pickup_b3[i] = if i < 12 {
                0.74 + 0.22 * (i as f32 / 12.0)
            } else if i < 60 {
                1.0 + 0.02 * ((i as f32) * 1.7).sin()
            } else {
                (1.0 - 0.14 * ((i - 60) as f32 / 30.0)).max(0.84)
            };
            pickup_b3_bass[i] = if i < 24 {
                1.32 - 0.28 * (i as f32 / 24.0)
            } else if i < 48 {
                1.04 - 0.08 * ((i - 24) as f32 / 24.0)
            } else {
                (0.95 - 0.20 * ((i - 48) as f32 / 42.0)).max(0.75)
            };
        }

        Self {
            sample_rate: sample_rate.max(8000.0),
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
            tonewheel_freqs: freqs,
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

    /// Reads a linearly interpolated delayed sample from a circular ring buffer.
    #[inline]
    fn read_ring_interpolated(buf: &[f32], write_idx: usize, delay_samples: f32) -> f32 {
        let cap = buf.len();
        let d = delay_samples.clamp(0.0, (cap - 2) as f32);
        let read_pos = (write_idx as f32 - d + cap as f32) % (cap as f32);
        let idx0 = (read_pos.floor() as usize) % cap;
        let idx1 = (idx0 + 1) % cap;
        let frac = read_pos - read_pos.floor();
        buf[idx0] * (1.0 - frac) + buf[idx1] * frac
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

    /// Advances all 91 spinning tonewheels and returns calibrated + crosstalk-coupled bus waveforms.
    fn render_tonewheels(&mut self, inv_fs: f32) -> [f32; NUM_TONEWHEELS] {
        let mut raw = [0.0f32; NUM_TONEWHEELS];
        for (i, sig) in raw.iter_mut().enumerate() {
            let p = self.tonewheel_phases[i];
            *sig = match self.model {
                OrganModel::B3 => {
                    // Bottom 12 Hammond complex wheels carry 2nd & 3rd harmonic warmth
                    if i < 12 {
                        (p.sin() + 0.18 * (2.0 * p).sin() + 0.09 * (3.0 * p).sin()) * 0.85
                    } else {
                        p.sin()
                    }
                }
                OrganModel::Pipe1 => p.sin(),
                OrganModel::Vox | OrganModel::Farfisa | OrganModel::Pipe2 => {
                    0.7 * p.sin() + 0.3 * (2.0 * p).sin()
                }
            };
            let next_p = p + TAU * self.tonewheel_freqs[i] * inv_fs;
            self.tonewheel_phases[i] = if next_p >= TAU { next_p - TAU } else { next_p };
        }

        if self.model != OrganModel::B3 {
            return raw;
        }

        // Apply per-tonewheel pickup calibration and -42 dB generator bin crosstalk matrix.
        let mut calibrated = [0.0f32; NUM_TONEWHEELS];
        let bleed = 0.008 * self.crosstalk_level.clamp(0.0, 2.0);
        for (i, dst) in calibrated.iter_mut().enumerate() {
            let pickup = self.pickup_calibration(i);
            let partner_4oct = (i + 48) % NUM_TONEWHEELS;
            let partner_oct = (i + 12) % NUM_TONEWHEELS;
            *dst = raw[i] * pickup + bleed * (0.75 * raw[partner_4oct] + 0.40 * raw[partner_oct]);
        }
        calibrated
    }

    /// Synthesizes palladium contact-bounce key-click impulses through a resonant bandpass filter.
    fn process_key_click(&mut self, inv_fs: f32) -> f32 {
        if self.click_env < 1e-6 && self.click_bp_state[0].abs() < 1e-6 {
            self.click_env = 0.0;
            return 0.0;
        }
        // Deterministic zero-alloc xorshift32 noise
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        let white = ((x as i32) as f32) * (1.0 / i32::MAX as f32);

        let center_hz = match self.model {
            OrganModel::Pipe1 | OrganModel::Pipe2 => 2400.0,
            _ => 1800.0,
        };
        let w = (TAU * center_hz * inv_fs).clamp(0.02, 0.55);
        let q_damp = 0.45;
        let excite = white * self.click_env;
        // 2-pole State-Variable Bandpass filter (zero DC offset)
        let hp = excite - self.click_bp_state[1] - q_damp * self.click_bp_state[0];
        let bp = self.click_bp_state[0] + w * hp;
        let lp = self.click_bp_state[1] + w * bp;
        self.click_bp_state[0] = bp;
        self.click_bp_state[1] = lp;

        self.click_env *= (-inv_fs / 0.0025).exp();
        bp * 0.85
    }

    /// Processes Hammond 6.85 Hz LC Scanner Vibrato / Chorus (`V1..V3`, `C1..C3`).
    fn process_scanner(&mut self, dry: f32, inv_fs: f32) -> f32 {
        if self.scanner_mode == ScannerMode::Off {
            return dry;
        }
        let (depth_ms, mix_wet) = match self.scanner_mode {
            ScannerMode::Off => (0.0, 0.0),
            ScannerMode::V1 => (0.75, 1.0),
            ScannerMode::V2 => (1.50, 1.0),
            ScannerMode::V3 => (2.50, 1.0),
            ScannerMode::C1 => (0.75, 0.5),
            ScannerMode::C2 => (1.50, 0.5),
            ScannerMode::C3 => (2.50, 0.5),
        };

        self.scanner_phase = (self.scanner_phase + TAU * 6.85 * inv_fs) % TAU;
        let mod_delay_samples =
            (depth_ms * 0.001 * self.sample_rate) * (0.5 + 0.5 * self.scanner_phase.sin());

        self.scanner_buffer[self.scanner_write_idx] = dry;
        let delayed = Self::read_ring_interpolated(
            self.scanner_buffer.as_ref(),
            self.scanner_write_idx,
            mod_delay_samples,
        );
        self.scanner_write_idx = (self.scanner_write_idx + 1) % SCANNER_BUFFER_SIZE;

        (1.0 - mix_wet) * dry + mix_wet * delayed
    }

    /// Processes Model 122 Rotary Speaker with Doppler delay, Close Mic, Angle, Stop Mode, and cabinet reflections.
    fn process_rotary(&mut self, scanner_out: f32, inv_fs: f32) -> [f32; 2] {
        let driven = ((1.0 + 3.0 * self.rotary_drive) * scanner_out).tanh();
        let alpha = (TAU * ROTARY_CROSSOVER_HZ * inv_fs).clamp(0.01, 0.99);
        self.lp_state += alpha * (driven - self.lp_state);
        let bass = self.lp_state;
        let treble = driven - bass;

        let effective_speed = if self.rotary_stop_mode {
            RotarySpeed::Stop
        } else {
            self.rotary_speed
        };
        let (target_horn, target_drum) = match effective_speed {
            RotarySpeed::Stop => (0.0, 0.0),
            RotarySpeed::Slow => (0.80, 0.67),
            RotarySpeed::Fast => (6.85, 5.70),
        };
        self.horn_rate_hz += (target_horn - self.horn_rate_hz) * (inv_fs / 0.7).min(1.0);
        self.drum_rate_hz += (target_drum - self.drum_rate_hz) * (inv_fs / 4.2).min(1.0);

        if effective_speed == RotarySpeed::Stop && self.horn_rate_hz < 0.05 {
            // Brake horn cleanly toward front cabinet position
            self.horn_phase *= (1.0 - inv_fs * 6.0).max(0.0);
        } else {
            self.horn_phase = (self.horn_phase + TAU * self.horn_rate_hz * inv_fs) % TAU;
        }
        if effective_speed == RotarySpeed::Stop && self.drum_rate_hz < 0.05 {
            self.drum_phase *= (1.0 - inv_fs * 3.0).max(0.0);
        } else {
            self.drum_phase = (self.drum_phase + TAU * self.drum_rate_hz * inv_fs) % TAU;
        }

        // Write crossover bands into pre-allocated Doppler / reflection buffers
        self.rotary_horn_buf[self.rotary_write_idx] = treble;
        self.rotary_drum_buf[self.rotary_write_idx] = bass;

        let angle_rad = self.rotary_mic_angle_deg.clamp(0.0, 180.0).to_radians();
        let (doppler_ms, beam_depth, refl_gain) = if self.rotary_close_mic {
            (0.55, 0.44, 0.05)
        } else {
            (0.35, 0.26, 0.18)
        };
        let base_delay = 0.002 * self.sample_rate;
        let exc_horn = doppler_ms * 0.001 * self.sample_rate;
        let exc_drum = (doppler_ms * 0.35) * 0.001 * self.sample_rate;

        let delay_hl = base_delay + exc_horn * (1.0 - self.horn_phase.cos());
        let delay_hr = base_delay + exc_horn * (1.0 - (self.horn_phase + angle_rad).cos());
        let delay_dl = base_delay + exc_drum * (1.0 - self.drum_phase.cos());
        let delay_dr = base_delay + exc_drum * (1.0 - (self.drum_phase + angle_rad).cos());

        let horn_l_raw = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            delay_hl,
        );
        let horn_r_raw = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            delay_hr,
        );
        let drum_l_raw = Self::read_ring_interpolated(
            self.rotary_drum_buf.as_ref(),
            self.rotary_write_idx,
            delay_dl,
        );
        let drum_r_raw = Self::read_ring_interpolated(
            self.rotary_drum_buf.as_ref(),
            self.rotary_write_idx,
            delay_dr,
        );

        // Cabinet early reflections (louder Horn cross-reflections in Normal mic distance)
        let refl_l = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            0.0034 * self.sample_rate,
        ) * refl_gain;
        let refl_r = Self::read_ring_interpolated(
            self.rotary_horn_buf.as_ref(),
            self.rotary_write_idx,
            0.0046 * self.sample_rate,
        ) * refl_gain;

        self.rotary_write_idx = (self.rotary_write_idx + 1) % ROTARY_BUFFER_SIZE;

        let beam_l = (1.0 - 0.5 * beam_depth) + 0.5 * beam_depth * self.horn_phase.cos();
        let beam_r =
            (1.0 - 0.5 * beam_depth) + 0.5 * beam_depth * (self.horn_phase + angle_rad).cos();
        let drum_beam_l = 0.85 + 0.15 * self.drum_phase.cos();
        let drum_beam_r = 0.85 + 0.15 * (self.drum_phase + angle_rad).cos();

        [
            horn_l_raw * beam_l + drum_l_raw * drum_beam_l + refl_r,
            horn_r_raw * beam_r + drum_r_raw * drum_beam_r + refl_l,
        ]
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
                let gain = f32::from(level.min(8)) / 8.0;
                let tw = Self::foldback_tonewheel(base_midi + DRAWBAR_OFFSETS[db_idx]);
                dry += gain * tw_signals[tw] * key_gain * db_bus_scale * 0.08;
            }
            if self.percussion_env > 1e-4 {
                let perc_offset = if self.percussion_third_harmonic {
                    19
                } else {
                    12
                };
                let tw = Self::foldback_tonewheel(base_midi + perc_offset);
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
    fn test_tonewheel_a4_frequency_and_foldback() {
        let organ = OrganLayer::new(48_000.0);
        // MIDI 69 (A4) maps to tonewheel index 69 - 24 = 45 (88/64 * 16 * 20 = 440.0 Hz exact)
        assert!((organ.tonewheel_freqs[45] - 440.0).abs() < 1e-3);
        // C1 (index 0) uses 85/104 gear ratio -> 32.6923 Hz (distinct from ideal ET 32.7032 Hz)
        assert!((organ.tonewheel_freqs[0] - 32.692307).abs() < 1e-3);
        // Top-octave tonewheels >= 84 use physical 192-tooth wheel
        assert_eq!(OrganLayer::tonewheel_gear_spec(84), (85, 104, 192));
        // Bottom foldback: MIDI 12 folds up to MIDI 24 (index 0)
        assert_eq!(OrganLayer::foldback_tonewheel(12), 0);
        // Top foldback: MIDI 120 folds down into valid tonewheel range
        assert!(OrganLayer::foldback_tonewheel(120) < NUM_TONEWHEELS);
    }

    #[test]
    fn test_b3_vs_b3_bass_pickup_calibration_and_crosstalk() {
        let mut standard = OrganLayer::new(48_000.0);
        let mut bass = OrganLayer::new(48_000.0);
        bass.b3_bass_mode = true;

        // B3 Bass boosts low tonewheel pickup calibration compared to standard B3
        assert!(bass.pickup_calibration(0) > standard.pickup_calibration(0) * 1.5);
        assert!(standard.pickup_calibration(75) > bass.pickup_calibration(75));

        // Verify crosstalk couples energy into partner tonewheels even on a silent drawbar slot
        standard.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
        standard.percussion_enabled = false;
        standard.rotary_enabled = false;
        standard.crosstalk_level = 1.0;
        let tw_with_bleed = standard.render_tonewheels(1.0 / 48_000.0);

        let mut clean = OrganLayer::new(48_000.0);
        clean.crosstalk_level = 0.0;
        let tw_clean = clean.render_tonewheels(1.0 / 48_000.0);

        // Advance phase to avoid sin(0) == 0 at first sample
        for _ in 0..32 {
            standard.render_tonewheels(1.0 / 48_000.0);
            clean.render_tonewheels(1.0 / 48_000.0);
        }
        let a = standard.render_tonewheels(1.0 / 48_000.0);
        let b = clean.render_tonewheels(1.0 / 48_000.0);
        let max_diff = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_diff > 1e-4 && tw_with_bleed.len() == tw_clean.len(),
            "crosstalk matrix must measurably couple adjacent/bin tonewheels (diff={max_diff})"
        );
    }

    #[test]
    fn test_key_click_bandpass_zero_dc_and_rotary_mic_modes() {
        let mut click_test = OrganLayer::new(48_000.0);
        click_test.drawbars = [0; 9]; // Isolate pure key-click transient
        click_test.percussion_enabled = false;
        click_test.rotary_enabled = false;
        click_test.note_on(60);

        let mut mean = 0.0f32;
        let mut energy = 0.0f32;
        for _ in 0..512 {
            let [l, _] = click_test.render_frame();
            assert!(l.is_finite());
            mean += l;
            energy += l * l;
        }
        assert!(
            energy > 1e-5,
            "bandpass key-click must produce transient energy"
        );
        assert!(
            (mean / 512.0).abs() < 5e-3,
            "bandpass key-click must have negligible DC offset (mean={})",
            mean / 512.0
        );

        // Compare Close Mic / 180 deg Angle vs Normal Mic / Stop Mode
        let mut close_mic = OrganLayer::new(48_000.0);
        close_mic.rotary_speed = RotarySpeed::Fast;
        close_mic.rotary_close_mic = true;
        close_mic.rotary_mic_angle_deg = 180.0;
        close_mic.note_on(60);

        let mut stopped = OrganLayer::new(48_000.0);
        stopped.rotary_speed = RotarySpeed::Fast;
        stopped.rotary_stop_mode = true;
        stopped.note_on(60);

        let mut diff_lr_close = 0.0f32;
        for i in 0..96_000 {
            let [l, r] = close_mic.render_frame();
            if i < 4096 {
                diff_lr_close += (l - r).abs();
            }
            stopped.render_frame();
        }
        assert!(
            diff_lr_close > 0.5,
            "wide-angle close-mic rotary must yield strong stereo separation"
        );
        assert!(
            stopped.horn_rate_hz < 0.06 && stopped.drum_rate_hz < 0.45,
            "rotary_stop_mode must brake horn & drum speeds toward zero (horn={}, drum={})",
            stopped.horn_rate_hz,
            stopped.drum_rate_hz
        );
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
        assert_eq!(ROTARY_CROSSOVER_HZ, 800.0);
        assert_eq!(ROTARY_BASS_HORN_BALANCES[0], (0.70, 0.30));
        assert_eq!(ROTARY_BASS_HORN_BALANCES[4], (0.30, 0.70));
    }
}
