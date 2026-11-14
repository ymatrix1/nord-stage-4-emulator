//! 2-Layer Physical Modeling Organ Engine (`DSPO` — 91 Tonewheels + Model 122 Rotary Speaker).
//!
//! Spec: `docs/specs/organ-engine.md`. Submodules: `types` (enums, constants), `drawbar` (bus
//! mapping, firmware level curve), `firmware_bank` (PNO Core1 oscillator-bank arithmetic),
//! `vibrato_bank` (12-TET ladder, detune table, group gains), `tonewheel` (91-wheel
//! generator), `keyclick`, `scanner`, `rotary`.

mod b3_keys;
mod b3_tail;
mod b3_weights;
mod b3_wheels;
mod drawbar;
mod drawbar_sum;
mod farfisa_wheels;
mod firmware_bank;
mod globals;
mod keyclick;
mod lane_mix;
mod output_level;
mod percussion_poly;
mod preset_live;
mod rotary;
mod scanner;
#[cfg(test)]
mod stereo_lanes;
mod tonewheel;
mod types;
mod vibrato_bank;
mod vibrato_link;
mod vox_bank;
#[cfg(test)]
mod vox_bank_tests;
mod vox_image;
mod vox_keys;
mod vox_layer;
mod vox_output;
mod vox_shaper;
mod vox_sines;
mod vox_source;
mod vox_timeline;
mod weight_state;
mod weights;

#[cfg(test)]
mod b3_source_tests;
#[cfg(test)]
mod bank_golden;
#[cfg(test)]
mod bank_mix_golden;
#[cfg(test)]
mod farfisa_golden;
#[cfg(test)]
mod farfisa_source_tests;
#[cfg(test)]
mod pipe_lines;
#[cfg(test)]
mod vox_golden;
#[cfg(test)]
mod weight_tests;

pub use b3_keys::{A_CAPTURED_WHEELS as B3_A_CAPTURED_WHEELS, WHEEL_GAIN as B3_WHEEL_GAIN};
pub use b3_wheels::{
    base_step as b3_base_step, firmware_note_hz as b3_firmware_note_hz, note_hz as b3_note_hz,
    note_step as b3_note_step, B3Wheels, ARM_FINE_WORD as B3_ARM_FINE_WORD,
};
pub use drawbar::{drawbar_gain, drawbar_gain_law, DrawbarLaw, FIRMWARE_DRAWBAR_CURVE};
pub use farfisa_wheels::{
    FarfisaWheels, DSP_FRAME_RATE as FARFISA_DSP_FRAME_RATE, OSCILLATORS as FARFISA_OSCILLATORS,
    ROWS as FARFISA_ROWS,
};
pub use firmware_bank::{
    build_x_table, cycles_per_frame, firmware_wave_x, fracmul, implied_frame_rate,
    implied_frame_rate_at, phase_step, ToneBank, ARM_DEFAULT_TUNE_WORD, DEFAULT_TUNE_WORD,
};
pub use globals::OrganGlobals;
pub use lane_mix::{gains_for, keep_for_rate, LaneMixer, FIRMWARE_FRAME_RATE_HZ};
pub use output_level::{
    layer_level_gain, swell_word, OutputLevel, LEGACY_BUS_OVER_UNIT, ORGAN_BUS_GAIN, SWELL_DEFAULT,
};
pub use types::{
    OrganModel, RotarySpeed, ScannerMode, ToneWheelSource, HAMMOND_GEAR_RATIOS, NUM_TONEWHEELS,
    ROTARY_BASS_HORN_BALANCES, ROTARY_CROSSOVER_HZ,
};
pub use vibrato_bank::{
    gain_value, lane_matrix, smooth, wheel_hz, BANK_WHEELS, DETUNE_CENTS, GAINS_DETUNED,
    GAINS_EXACT,
};

pub use vibrato_link::{level_for_scanner, VibratoLevel, VibratoLink};
pub use weights::{
    a_key_down, a_update, targets as weight_targets, weight_sum, WeightMode, UNIT_WEIGHT,
    WEIGHT_SCALE,
};

use std::f32::consts::TAU;
use types::{ROTARY_BUFFER_SIZE, SCANNER_BUFFER_SIZE};
use weight_state::{velocity_for_gain, WeightState};

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
    /// POLY percussion (manual p.20, Shift+Volume): every new key stroke gets its own percussion
    /// attack regardless of held keys. Default `false` = single-trigger (unchanged behaviour).
    pub percussion_poly: bool,
    /// Drawbar Live mode (manual p.21, Preset off): the physical drawbar positions
    /// (`live_drawbars`) drive the sound instead of the stored `drawbars`. Default `false`.
    pub drawbar_live: bool,
    /// Physical drawbar positions (`0..=8`), used while `drawbar_live`; `88 8000 000` by default.
    pub live_drawbars: [u8; 9],
    /// Transformer compartment crosstalk intensity (`1.0` = nominal -42 dB B3 bleed).
    pub crosstalk_level: f32,
    pub rotary_enabled: bool,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    /// Opt-in rotary drive law `tanh(g x) / g`, `g = 1 + 3 drive` (unity small signal gain, so the
    /// drive no longer adds +18.4 dB at drive 2.44); default `false` keeps `tanh(g x)`.
    pub rotary_drive_unity: bool,
    /// Opt-in: reads `rotary_drive` as the preset unit 0..10 and rescales it to the engine unit
    /// 0..1 at use (EMPIRICAL, `docs/research/b3-rotary.md`); the stored value is untouched.
    pub rotary_drive_scaled: bool,
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
    /// Whether the bank table and lane gains follow `scanner_mode` with the firmware smoother and
    /// stereo lanes (`Firmware`, default) or only `detuned_copies` with instant mono mixing
    /// (`Manual`, the previous behaviour).
    pub vibrato_link: VibratoLink,
    /// Fine Tune in cents applied to the firmware bank (clamped to -400..+381, default 0).
    pub fine_tune_cents: f32,
    /// Drawbar level law (default: firmware curve; `Linear` is the old `level / 8`).
    pub drawbar_law: DrawbarLaw,
    /// How the per-wheel drawbar weights are formed (default `Legacy`: instant gains, ASSUMED
    /// bus scale and pickup calibration; `Firmware`: the ARM weights, smoothed, absolute scale).
    pub weight_mode: WeightMode,
    /// Output level `L` of the mix stage (`0x95B42`) applied in `WeightMode::Firmware`.
    /// ASSUMED 1.0: no ARM message sets it.
    pub weight_level: f32,
    /// Output level law of the firmware paths (`Firmware` weights, `B3Wheels`, Vox): `Legacy`
    /// keeps their built-in levels, `Firmware` applies [`ORGAN_BUS_GAIN`] ([`OutputLevel`]).
    pub output_level: OutputLevel,
    /// Key-click transient on key-down / key-up (default on; off for oracle comparisons).
    pub key_click_enabled: bool,
    /// Global Sound/System menu settings (defaults are exact no-ops).
    pub globals: OrganGlobals,
    /// Pitch stick bend in cents (manual p.18 PSTICK), set with [`Self::set_bend_cents`]. Default
    /// `0.0` and ratio `1.0` are exact no-ops.
    bend_cents: f32,
    /// `2^(bend_cents / 1200)`, the Legacy wheel phase step factor.
    bend_ratio: f32,
    tonewheel_freqs: [f32; NUM_TONEWHEELS],
    tonewheel_freqs_hz: [f64; NUM_TONEWHEELS],
    synced_fine_cents: f32,
    tone_bank: ToneBank,
    vox: vox_source::VoxSource,
    vox_timeline: vox_timeline::VoxTimeline,
    b3_wheels: B3Wheels,
    b3_weights: b3_weights::B3Weights,
    b3_synced: (f32, f32),
    /// Whether `b3_wheels` was built with the processor tail (see `set_b3_tail`).
    b3_tail: bool,
    farfisa_wheels: FarfisaWheels,
    farfisa_synced: (f32, f32),
    lane_mixer: LaneMixer,
    weight_state: WeightState,
    tonewheel_phases: [f32; NUM_TONEWHEELS],
    pickup_b3: [f32; NUM_TONEWHEELS],
    pickup_b3_bass: [f32; NUM_TONEWHEELS],
    held_notes: [bool; 128],
    note_gains: [f32; 128],
    active_key_count: usize,
    percussion_env: f32,
    poly_perc: percussion_poly::PolyPercussion,
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
            percussion_poly: false,
            drawbar_live: false,
            live_drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            crosstalk_level: 1.0,
            rotary_enabled: true,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.2,
            rotary_drive_unity: false,
            rotary_drive_scaled: false,
            rotary_close_mic: false,
            rotary_mic_angle_deg: 120.0,
            rotary_stop_mode: false,
            scanner_mode: ScannerMode::Off,
            tonewheel_source: ToneWheelSource::default(),
            detuned_copies: false,
            vibrato_link: VibratoLink::default(),
            fine_tune_cents: 0.0,
            drawbar_law: DrawbarLaw::default(),
            weight_mode: WeightMode::default(),
            weight_level: 1.0,
            output_level: OutputLevel::default(),
            key_click_enabled: true,
            globals: OrganGlobals::default(),
            bend_cents: 0.0,
            bend_ratio: 1.0,
            tonewheel_freqs: freqs,
            tonewheel_freqs_hz: freqs_hz,
            synced_fine_cents: 0.0,
            tone_bank,
            vox: vox_source::VoxSource::new(sample_rate),
            vox_timeline: vox_timeline::VoxTimeline::default(),
            b3_wheels: B3Wheels::new(),
            b3_weights: b3_weights::B3Weights::new(sample_rate),
            b3_synced: (0.0, f32::NAN),
            b3_tail: true,
            farfisa_wheels: FarfisaWheels::new(),
            farfisa_synced: (0.0, f32::NAN),
            lane_mixer: LaneMixer::new(sample_rate),
            weight_state: WeightState::new(sample_rate, [8, 8, 8, 0, 0, 0, 0, 0, 0]),
            tonewheel_phases: [0.0; NUM_TONEWHEELS],
            pickup_b3,
            pickup_b3_bass,
            held_notes: [false; 128],
            note_gains: [1.0; 128],
            active_key_count: 0,
            percussion_env: 0.0,
            poly_perc: percussion_poly::PolyPercussion::new(),
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

    /// Selects the B3 generator tables with (default) or without the processor tail's frequency
    /// response ([`b3_tail`](self)); setup-time only (rebuilds the wheel tables and phases).
    pub fn set_b3_tail(&mut self, on: bool) {
        if self.b3_tail == on {
            return; // live A/B re-sends the options often: rebuild (and reset the phases) only on a change
        }
        self.b3_tail = on;
        self.b3_wheels = if on {
            B3Wheels::new()
        } else {
            B3Wheels::without_tail()
        };
        self.b3_synced = (0.0, f32::NAN);
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
            if self.percussion_poly && self.percussion_enabled {
                self.percussion_env = 1.0;
                self.poly_perc.trigger(idx);
            }
            self.held_notes[idx] = true;
            self.note_gains[idx] = gain.clamp(0.0, 1.0);
            self.b3_weights.mark();
            let drawbars = *self.active_drawbars();
            self.weight_state.key_down(
                idx as u8,
                velocity_for_gain(gain),
                &drawbars,
                self.octave_shift.clamp(-2, 2),
            );
            self.active_key_count += 1;
            let click_scale = match self.model {
                OrganModel::B3 | OrganModel::Vox => 0.45,
                OrganModel::Farfisa => 0.18,
                OrganModel::Pipe1 | OrganModel::Pipe2 => 0.25,
            };
            if self.key_click_enabled {
                self.click_env = click_scale * self.note_gains[idx].max(0.25);
            }
        }
    }

    /// Releases a held key.
    pub fn note_off(&mut self, note: u8) {
        let idx = (note as usize).min(127);
        if self.held_notes[idx] {
            self.held_notes[idx] = false;
            self.b3_weights.mark();
            self.weight_state.key_up(idx as u8);
            self.active_key_count = self.active_key_count.saturating_sub(1);
            let click_scale = match self.model {
                OrganModel::B3 | OrganModel::Vox => 0.16,
                OrganModel::Farfisa => 0.06,
                OrganModel::Pipe1 | OrganModel::Pipe2 => 0.05,
            };
            if self.key_click_enabled {
                self.click_env = self.click_env.max(click_scale);
            }
        }
    }

    /// Renders a single stereo frame `[left, right]` with zero heap allocations.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        if self.vox_active() {
            return self.render_vox_frame(inv_fs);
        }

        // 1. Advance all 91 spinning tonewheels continuously (two lanes while the detuned
        //    ensemble's stereo blend is active, otherwise one)
        let (tw_buses, lanes) = self.render_tonewheel_lanes(inv_fs);

        if self.active_key_count == 0
            && !(self.firmware_weights() && self.weight_state.active())
            && !(self.b3_weights_active() && self.b3_weights.active())
            && self.click_env < 1e-6
            && self.click_bp_state[0].abs() < 1e-6
        {
            return [0.0, 0.0];
        }

        // 2. Sum active drawbars across held notes (with B3 Normal percussion -3 dB drawbar dip).
        //    Two lanes split into mid (through scanner and rotary, so the mono sum is the
        //    previous path) and side (added to L and subtracted from R after the rotary).
        let sums = self.sum_drawbars(&tw_buses, lanes);
        let (mut dry, side) = if lanes == 2 {
            (0.5 * (sums[0] + sums[1]), Some(0.5 * (sums[0] - sums[1])))
        } else {
            (sums[0], None)
        };

        // 3. Model filter shaping (Vox Continental Tone, Farfisa Formants, Pipe Chiff)
        match self.model {
            OrganModel::B3 => {}
            OrganModel::Vox => {
                // Vox drawbar 9 acts as Wave/Tone control morphing between flute and reed
                let tone = f32::from(self.active_drawbars()[8].min(8)) / 8.0;
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
                // ASSUMED: flat output gain 0.92 (a textbook trim, no firmware source). It shapes
                // no frequency: the bank + mix path matches the oracle to 0.00 dB per line on three
                // chords (`pipe_lines.rs`), so it is not the cause of the darkness against the video
                // (`docs/research/pipe-video-gap.md`).
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
        let perc_decay = (-inv_fs / perc_tau).exp();
        self.percussion_env *= perc_decay;
        let poly_peak = self.poly_perc.decay(perc_decay);
        if self.percussion_poly {
            self.percussion_env = poly_peak;
        }

        // 5. Hammond Scanner Vibrato & Chorus (V1-V3, C1-C3)
        let scanner_out = self.process_scanner(dry, inv_fs);

        // 6. Model 122 Rotary Speaker (skipped when disabled)
        let [l, r] = if self.rotary_enabled {
            self.process_rotary(scanner_out, inv_fs)
        } else {
            [scanner_out, scanner_out]
        };
        match side {
            Some(s) => [l + s, r - s],
            None => [l, r],
        }
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
    fn model_default_source_and_all_sources_render_bounded_audio() {
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
            ToneWheelSource::ModelDefault
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
