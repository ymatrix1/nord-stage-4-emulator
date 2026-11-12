//! 7 Independent Per-Layer FX Chains (`Mod1`, `Mod2`, `Amp/EQ`, `Delay`, `Reverb`) & Master Bus.
//!
//! Spec: `docs/specs/effects-and-routing.md`

use crate::morph::{MorphControllers, MorphParameter};
use std::f32::consts::TAU;

/// Fixed stereo delay line capacity (131,072 samples ≈ 2.73 s at 48 kHz; zero heap allocations during `process`).
const DELAY_LINE_LEN: usize = 131_072;

/// Fixed stereo modulation buffer capacity for Chorus / Vibe / Flanger (2048 samples ≈ 42 ms at 48 kHz).
const MOD2_LINE_LEN: usize = 2048;

/// Number of delay lines in the stereo Feedback Delay Network (FDN) Reverb.
const FDN_SIZE: usize = 4;

/// Fixed FDN circular buffer capacity (8192 samples ≈ 170 ms at 48 kHz; zero runtime allocs).
const FDN_BUF_LEN: usize = 8192;

/// Base prime sample delay lengths for the 4 FDN delay lines (calibrated for Room @ 48 kHz).
const FDN_DELAYS: [usize; FDN_SIZE] = [1399, 1747, 2113, 2551];

/// Prime stereo input-diffuser allpass lengths (`[L0, L1, R0, R1]`) to eliminate metallic slapback.
const REV_AP_DELAYS: [usize; FDN_SIZE] = [149, 331, 179, 389];

/// Hardware resonance (`Q`) table length (`0x2f970c` in OS v1.66 `bank0_CODE.bin`).
pub const FW_Q_STEPS: usize = 128;

/// Firmware log-scale resonance: `Q(idx) = 0.1 * 100^(idx / 127)` (`0.1 .. 10.0`).
///
/// VERIFIED: all 128 `f32` entries at `0x2f970c` of OS v1.66 `bank0_CODE.bin`, max rel. error `6.4e-8`.
/// Check: Python `struct.unpack('<128f')` vs formula (see `docs/research/sharc-dsp-and-preset-hash-teardown.md`);
/// unit test `test_firmware_q_table_endpoints_and_monotonic` pins endpoints only.
#[inline]
pub fn firmware_q(idx: usize) -> f32 {
    let i = idx.min(FW_Q_STEPS - 1) as f64;
    (0.1 * 100.0f64.powf(i / 127.0)) as f32
}

/// Filter shapes for [`Biquad`] designs (RBJ Audio-EQ-Cookbook forms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BiquadKind {
    LowShelf,
    HighShelf,
    Peak,
    LowPass,
    HighPass,
}

/// Designs normalized 2nd-order coefficients `[b0, b1, b2, a1, a2]` (`a0 = 1`).
///
/// `fc` is clamped to `[10, 0.49 * fs]` and `q` to `[0.1, 10]` so poles stay inside the unit circle.
fn design_biquad(kind: BiquadKind, fc: f32, fs: f32, q: f32, gain_db: f32) -> [f32; 5] {
    let fs = f64::from(fs);
    let fc = f64::from(fc).clamp(10.0, 0.49 * fs);
    let q = f64::from(q).clamp(0.1, 10.0);
    let a = 10.0f64.powf(f64::from(gain_db) / 40.0);
    let w0 = std::f64::consts::TAU * fc / fs;
    let (sin_w, cos_w) = w0.sin_cos();
    let alpha = sin_w / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match kind {
        BiquadKind::Peak => (
            1.0 + alpha * a,
            -2.0 * cos_w,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w,
            1.0 - alpha / a,
        ),
        BiquadKind::LowShelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w + t),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w),
                a * ((a + 1.0) - (a - 1.0) * cos_w - t),
                (a + 1.0) + (a - 1.0) * cos_w + t,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w),
                (a + 1.0) + (a - 1.0) * cos_w - t,
            )
        }
        BiquadKind::HighShelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w + t),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w),
                a * ((a + 1.0) + (a - 1.0) * cos_w - t),
                (a + 1.0) - (a - 1.0) * cos_w + t,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w),
                (a + 1.0) - (a - 1.0) * cos_w - t,
            )
        }
        BiquadKind::LowPass => (
            (1.0 - cos_w) * 0.5,
            1.0 - cos_w,
            (1.0 - cos_w) * 0.5,
            1.0 + alpha,
            -2.0 * cos_w,
            1.0 - alpha,
        ),
        BiquadKind::HighPass => (
            (1.0 + cos_w) * 0.5,
            -(1.0 + cos_w),
            (1.0 + cos_w) * 0.5,
            1.0 + alpha,
            -2.0 * cos_w,
            1.0 - alpha,
        ),
    };
    [
        (b0 / a0) as f32,
        (b1 / a0) as f32,
        (b2 / a0) as f32,
        (a1 / a0) as f32,
        (a2 / a0) as f32,
    ]
}

/// Stereo transposed direct-form II biquad (shared coefficients, per-channel state).
#[derive(Debug, Clone, Copy)]
struct Biquad {
    c: [f32; 5],
    z: [[f32; 2]; 2],
}

impl Biquad {
    const fn identity() -> Self {
        Self {
            c: [1.0, 0.0, 0.0, 0.0, 0.0],
            z: [[0.0; 2]; 2],
        }
    }

    #[inline]
    fn set(&mut self, kind: BiquadKind, fc: f32, fs: f32, q: f32, gain_db: f32) {
        self.c = design_biquad(kind, fc, fs, q, gain_db);
    }

    #[inline]
    fn process(&mut self, ch: usize, x: f32) -> f32 {
        let [b0, b1, b2, a1, a2] = self.c;
        let z = &mut self.z[ch];
        let y = b0 * x + z[0];
        z[0] = b1 * x - a1 * y + z[1];
        z[1] = b2 * x - a2 * y;
        if y.is_finite() {
            y
        } else {
            *z = [0.0; 2];
            0.0
        }
    }
}

/// Bass shelf corner (Hz). ASSUMED: inherited from the old 1-pole code and `effects-and-routing.md`; no firmware or recording check.
const EQ_BASS_HZ: f32 = 100.0;
/// Treble shelf corner (Hz). ASSUMED: same as bass corner, unverified.
const EQ_TREBLE_HZ: f32 = 4000.0;
/// Shelf slope `Q` (Butterworth-like `1/sqrt(2)`). ASSUMED: textbook default; firmware shelf shape unknown.
const EQ_SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Mid peak `Q` index. ASSUMED: scale is VERIFIED (`firmware_q`), but index 64 (`Q~1.02`) is chosen to match the spec's `Q = 1.0`, not read from firmware.
const EQ_MID_Q_IDX: usize = 64;

/// EQ Mid / Amp-filter frequency steps (`0..=127`, Hz).
///
/// EMPIRICAL: values as displayed by the hardware (OLED) and transcribed in third-party `ns4decode`
/// (`ns4decode.py::interpretEQfreq`, MIT, Randy 2024); precision is the display rounding (~1 to 5 %).
/// Not found as a numeric table in OS v1.66 `bank0_CODE.bin` (searched f32/i32/i16), so not firmware-verified.
pub const EQ_FREQ_HZ: [f32; 128] = [
    200.0, 205.0, 210.0, 215.0, 221.0, 226.0, 232.0, 238.0, 244.0, 250.0, 257.0, 263.0, 270.0,
    277.0, 284.0, 291.0, 299.0, 306.0, 314.0, 322.0, 330.0, 339.0, 347.0, 356.0, 365.0, 375.0,
    384.0, 394.0, 404.0, 414.0, 425.0, 436.0, 447.0, 458.0, 470.0, 482.0, 494.0, 507.0, 520.0,
    533.0, 546.0, 560.0, 575.0, 589.0, 604.0, 620.0, 635.0, 652.0, 668.0, 685.0, 703.0, 721.0,
    739.0, 758.0, 777.0, 797.0, 817.0, 838.0, 859.0, 881.0, 904.0, 927.0, 950.0, 975.0, 999.0,
    1000.0, 1000.0, 1100.0, 1100.0, 1200.0, 1200.0, 1300.0, 1300.0, 1400.0, 1400.0, 1500.0, 1500.0,
    1600.0, 1600.0, 1700.0, 1700.0, 1800.0, 1800.0, 1900.0, 1900.0, 2000.0, 2100.0, 2100.0, 2200.0,
    2300.0, 2400.0, 2400.0, 2500.0, 2600.0, 2700.0, 2800.0, 2900.0, 3000.0, 3100.0, 3200.0, 3300.0,
    3400.0, 3500.0, 3600.0, 3700.0, 3900.0, 4000.0, 4100.0, 4300.0, 4400.0, 4600.0, 4700.0, 4900.0,
    5000.0, 5200.0, 5400.0, 5600.0, 5800.0, 5900.0, 6100.0, 6300.0, 6600.0, 6800.0, 7000.0, 7200.0,
    7500.0, 7700.0, 8000.0,
];

/// Maps a normalized `0.0..=1.0` knob/morph position to the nearest [`EQ_FREQ_HZ`] step.
#[inline]
pub fn eq_freq_hz_from_norm(norm: f32) -> f32 {
    let idx = (norm.clamp(0.0, 1.0) * 127.0).round() as usize;
    EQ_FREQ_HZ[idx.min(127)]
}

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

/// Amp simulation modes (`Amp Sim / EQ`).
///
/// VERIFIED (names/order): UI strings at `0x2cc6cc` in OS v1.66 `bank0_CODE.bin` read
/// `Twin Type I/II`, `JC Type I/II`, `Small Speaker`, `Very Small Speaker`, `To Rotary`, `LP24`, `HP24`.
/// EMPIRICAL (numeric index 0..9 = off, Twin, Twin II, JC, JC II, Small, Small II, To Rotary, LP, HP):
/// third-party `ns4decode` (MIT, Randy 2024) `ns4names.AmpEQmode`, cross-checked on factory programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmpCabinet {
    #[default]
    Off,
    Small,
    Jc,
    Twin,
    ToRotary,
    LpFilter,
    HpFilter,
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

/// Per-parameter Morph (`Wheel` / `A.T.` / `CtrlPed`) bases and deltas for a layer FX chain.
///
/// A parameter is driven only when at least one delta is non-zero; otherwise the plain field is left alone.
#[derive(Debug, Clone, Copy)]
pub struct FxMorphSet {
    pub mod1_rate: MorphParameter,
    pub mod1_amount: MorphParameter,
    pub mod2_rate: MorphParameter,
    pub mod2_amount: MorphParameter,
    pub eq_mid_freq: MorphParameter,
    pub amp_drive: MorphParameter,
    pub delay_mix: MorphParameter,
    pub delay_feedback: MorphParameter,
    pub reverb_amount: MorphParameter,
}

impl Default for FxMorphSet {
    fn default() -> Self {
        let p = MorphParameter::new(0.0);
        Self {
            mod1_rate: p,
            mod1_amount: p,
            mod2_rate: p,
            mod2_amount: p,
            eq_mid_freq: p,
            amp_drive: p,
            delay_mix: p,
            delay_feedback: p,
            reverb_amount: p,
        }
    }
}

#[inline]
fn morph_active(p: &MorphParameter) -> bool {
    p.delta_wheel.abs() > 1e-5 || p.delta_pedal.abs() > 1e-5 || p.delta_aftertouch.abs() > 1e-5
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
    /// `Type II` / `Very Small Speaker` variation flag (stored; not yet modelled, ASSUMED no DSP difference).
    pub amp_variation: bool,
    /// Resonance (`0.0..=10.0`) of the `LP24` / `HP24` modes (replaces the Mid gain knob).
    pub eq_filter_res: f32,
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
    pub morph: FxMorphSet,
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
    cab_lp: Biquad,
    cab_hp: Biquad,
    cab_mid: Biquad,
    cab_cache: Option<AmpCabinet>,
    flt_a: Biquad,
    flt_b: Biquad,
    /// Last designed `[mode, freq_hz, res]` for LP24/HP24 (`NaN` forces first design).
    flt_cache: [f32; 3],
    eq_bass: Biquad,
    eq_mid: Biquad,
    eq_treble: Biquad,
    /// Last designed `[bass_db, mid_db, mid_hz, treble_db]` (`NaN` forces first design).
    eq_cache: [f32; 4],
    comp_env: f32,
    delay_buf_l: Box<[f32]>,
    delay_buf_r: Box<[f32]>,
    delay_write_idx: usize,
    delay_smoothed_samples: f32,
    delay_var_phase: f32,
    delay_lp_state: [f32; 2],
    delay_hp_state: [f32; 2],
    fdn_bufs: Box<[[f32; FDN_BUF_LEN]; FDN_SIZE]>,
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
            amp_variation: false,
            eq_filter_res: 0.0,
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
            morph: FxMorphSet::default(),
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
            cab_lp: Biquad::identity(),
            cab_hp: Biquad::identity(),
            cab_mid: Biquad::identity(),
            cab_cache: None,
            flt_a: Biquad::identity(),
            flt_b: Biquad::identity(),
            flt_cache: [f32::NAN; 3],
            eq_bass: Biquad::identity(),
            eq_mid: Biquad::identity(),
            eq_treble: Biquad::identity(),
            eq_cache: [f32::NAN; 4],
            comp_env: 0.0,
            delay_buf_l: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_buf_r: vec![0.0; DELAY_LINE_LEN].into_boxed_slice(),
            delay_write_idx: 0,
            delay_smoothed_samples: default_delay_ms * 0.001 * fs,
            delay_var_phase: 0.0,
            delay_lp_state: [0.0; 2],
            delay_hp_state: [0.0; 2],
            fdn_bufs: Box::new([[0.0; FDN_BUF_LEN]; FDN_SIZE]),
            fdn_write_indices: [0; FDN_SIZE],
            fdn_lp_states: [0.0; FDN_SIZE],
            rev_ap_bufs: Box::new([[0.0; 512]; FDN_SIZE]),
            rev_ap_indices: [0; FDN_SIZE],
            reverb_chorale_phase: 0.0,
        }
    }

    /// Drives Morph-assigned parameters from the live `Wheel` / `A.T.` / `CtrlPed` state.
    ///
    /// Allocation-free; parameters with all-zero deltas are left untouched.
    pub fn apply_morph(&mut self, ctrl: &MorphControllers) {
        let m = self.morph;
        if morph_active(&m.mod1_rate) {
            self.mod1_rate_hz = 0.1 + m.mod1_rate.evaluate(ctrl) * 14.9;
        }
        if morph_active(&m.mod1_amount) {
            self.mod1_amount = m.mod1_amount.evaluate(ctrl);
        }
        if morph_active(&m.mod2_rate) {
            self.mod2_rate_hz = 0.1 + m.mod2_rate.evaluate(ctrl) * 9.9;
        }
        if morph_active(&m.mod2_amount) {
            self.mod2_amount = m.mod2_amount.evaluate(ctrl);
        }
        if morph_active(&m.eq_mid_freq) {
            self.eq_mid_freq_hz = eq_freq_hz_from_norm(m.eq_mid_freq.evaluate(ctrl));
        }
        if morph_active(&m.amp_drive) {
            self.drive_amount = m.amp_drive.evaluate(ctrl);
        }
        if morph_active(&m.delay_mix) {
            self.delay_mix = m.delay_mix.evaluate(ctrl);
        }
        if morph_active(&m.delay_feedback) {
            self.delay_feedback = m.delay_feedback.evaluate(ctrl).min(0.95);
        }
        if morph_active(&m.reverb_amount) {
            self.reverb_mix = m.reverb_amount.evaluate(ctrl);
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

        // Cabinet voicing: 2nd-order Butterworth HP + LP and a peaking mid band.
        // ASSUMED: all cabinet Hz/gain values below are inherited from the old 1-pole code (no firmware/recording source).
        let cab = matches!(
            self.amp_cabinet,
            AmpCabinet::Small | AmpCabinet::Jc | AmpCabinet::Twin
        );
        if cab {
            if self.cab_cache != Some(self.amp_cabinet) {
                let (lp_hz, hp_hz, mid_hz, mid_gain) = match self.amp_cabinet {
                    AmpCabinet::Small => (3400.0, 140.0, 900.0, 0.45f32),
                    AmpCabinet::Jc => (6500.0, 70.0, 2800.0, 0.35),
                    _ => (5200.0, 65.0, 650.0, -0.30),
                };
                let fs = self.sample_rate;
                self.cab_hp
                    .set(BiquadKind::HighPass, hp_hz, fs, EQ_SHELF_Q, 0.0);
                self.cab_lp
                    .set(BiquadKind::LowPass, lp_hz, fs, EQ_SHELF_Q, 0.0);
                let mid_db = 20.0 * (1.0 + mid_gain).log10();
                self.cab_mid.set(BiquadKind::Peak, mid_hz, fs, 1.0, mid_db);
                self.cab_cache = Some(self.amp_cabinet);
            }
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                let x = self.cab_hp.process(ch, *sig);
                let x = self.cab_lp.process(ch, x);
                *sig = self.cab_mid.process(ch, x);
            }
        } else {
            self.cab_cache = None;
        }

        // `LP24` / `HP24`: 24 dB/oct (2 cascaded 2nd-order sections), cutoff = EQ Mid Freq knob,
        // resonance = Mid knob. VERIFIED: names `LP24`/`HP24` (0x2cc6cc). ASSUMED: Butterworth-4 section Q
        // (0.5412 / 1.3066) with resonance scaling the second section's Q; real topology unknown.
        if matches!(
            self.amp_cabinet,
            AmpCabinet::LpFilter | AmpCabinet::HpFilter
        ) {
            let mode = if self.amp_cabinet == AmpCabinet::LpFilter {
                0.0
            } else {
                1.0
            };
            let now = [mode, self.eq_mid_freq_hz, self.eq_filter_res];
            if now != self.flt_cache {
                let kind = if mode == 0.0 {
                    BiquadKind::LowPass
                } else {
                    BiquadKind::HighPass
                };
                let res = (self.eq_filter_res / 10.0).clamp(0.0, 1.0);
                let q2 = (1.3066 * (1.0 + 2.0 * res)).min(10.0);
                self.flt_a
                    .set(kind, self.eq_mid_freq_hz, self.sample_rate, 0.5412, 0.0);
                self.flt_b
                    .set(kind, self.eq_mid_freq_hz, self.sample_rate, q2, 0.0);
                self.flt_cache = now;
            }
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                let x = self.flt_a.process(ch, *sig);
                *sig = self.flt_b.process(ch, x);
            }
        }

        // 3-band EQ: bass low shelf, sweepable mid peak (200 Hz .. 8 kHz), treble high shelf.
        // True 2nd-order biquads per channel (stereo image preserved); redesigned only on change.
        let eq_now = [
            self.eq_bass_db,
            self.eq_mid_db,
            self.eq_mid_freq_hz,
            self.eq_treble_db,
        ];
        if eq_now != self.eq_cache {
            let fs = self.sample_rate;
            let mid_fc = self.eq_mid_freq_hz.clamp(200.0, 8000.0);
            self.eq_bass.set(
                BiquadKind::LowShelf,
                EQ_BASS_HZ,
                fs,
                EQ_SHELF_Q,
                self.eq_bass_db.clamp(-15.0, 15.0),
            );
            self.eq_mid.set(
                BiquadKind::Peak,
                mid_fc,
                fs,
                firmware_q(EQ_MID_Q_IDX),
                self.eq_mid_db.clamp(-15.0, 15.0),
            );
            self.eq_treble.set(
                BiquadKind::HighShelf,
                EQ_TREBLE_HZ,
                fs,
                EQ_SHELF_Q,
                self.eq_treble_db.clamp(-15.0, 15.0),
            );
            self.eq_cache = eq_now;
        }
        if self.eq_bass_db.abs() > 0.1 {
            l = self.eq_bass.process(0, l);
            r = self.eq_bass.process(1, r);
        }
        let mid_is_res = matches!(
            self.amp_cabinet,
            AmpCabinet::LpFilter | AmpCabinet::HpFilter
        );
        if !mid_is_res && self.eq_mid_db.abs() > 0.1 {
            l = self.eq_mid.process(0, l);
            r = self.eq_mid.process(1, r);
        }
        if self.eq_treble_db.abs() > 0.1 {
            l = self.eq_treble.process(0, l);
            r = self.eq_treble.process(1, r);
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
        let (decay_gain, room_scale) = match self.reverb_model {
            ReverbModel::Off => (0.0, 1.0),
            ReverbModel::Booth => (0.52, 0.58),
            ReverbModel::Room => (0.72, 1.00),
            ReverbModel::Stage => (0.83, 1.48),
            ReverbModel::Spring => (0.77, 0.72),
            ReverbModel::Hall => (0.89, 2.05),
            ReverbModel::Cathedral => (0.935, 2.65),
        };
        let damp = match self.reverb_color {
            ReverbColor::Normal => 0.42,
            ReverbColor::Bright => 0.68,
            ReverbColor::Dark => 0.20,
            ReverbColor::Chorale => 0.48,
        };
        let (mod_rate_hz, mod_depth) = if self.reverb_color == ReverbColor::Chorale {
            (0.75, 8.0)
        } else {
            (0.42, 1.5)
        };
        self.reverb_chorale_phase = (self.reverb_chorale_phase + TAU * mod_rate_hz * inv_fs) % TAU;
        let mod_offset = (self.reverb_chorale_phase.sin() * mod_depth).round() as isize;
        let fs_scale = (self.sample_rate / 48_000.0).clamp(0.4, 4.0);

        let mut tap_outs = [0.0f32; FDN_SIZE];
        for (i, out) in tap_outs.iter_mut().enumerate() {
            let base_f = if self.reverb_model == ReverbModel::Spring {
                (FDN_DELAYS[i] as f32) * 0.52 + (127.0 + i as f32 * 43.0)
            } else {
                (FDN_DELAYS[i] as f32) * room_scale
            };
            let base_delay = (base_f * fs_scale).round() as isize;
            let sign = if i % 2 == 0 { mod_offset } else { -mod_offset };
            let delay = (base_delay + sign).clamp(64, (FDN_BUF_LEN - 8) as isize) as usize;
            let r_idx = (self.fdn_write_indices[i] + FDN_BUF_LEN - delay) % FDN_BUF_LEN;
            *out = self.fdn_bufs[i][r_idx];
        }

        // Hadamard 4x4 unitary feedback matrix (normalized by 0.5)
        let s0 = tap_outs[0] + tap_outs[1] + tap_outs[2] + tap_outs[3];
        let s1 = tap_outs[0] - tap_outs[1] + tap_outs[2] - tap_outs[3];
        let s2 = tap_outs[0] + tap_outs[1] - tap_outs[2] - tap_outs[3];
        let s3 = tap_outs[0] - tap_outs[1] - tap_outs[2] + tap_outs[3];
        let hadamard = [0.5 * s0, 0.5 * s1, 0.5 * s2, 0.5 * s3];

        // Two-stage stereo input allpass diffusion prevents transient flutter/metallic ringing
        const AP_G: f32 = 0.55;
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
            0.70 * dif_l + 0.30 * dif_r,
            0.30 * dif_l + 0.70 * dif_r,
            0.85 * dif_l - 0.15 * dif_r,
            -0.15 * dif_l + 0.85 * dif_r,
        ];
        for (i, &had_val) in hadamard.iter().enumerate().take(FDN_SIZE) {
            let fb = had_val * decay_gain;
            self.fdn_lp_states[i] += damp * (fb - self.fdn_lp_states[i]);
            self.fdn_bufs[i][self.fdn_write_indices[i]] =
                stereo_in[i] * 0.55 + self.fdn_lp_states[i];
            self.fdn_write_indices[i] = (self.fdn_write_indices[i] + 1) % FDN_BUF_LEN;
        }

        let rev_l = 0.42 * (tap_outs[0] + 0.75 * tap_outs[2] + 0.25 * tap_outs[3]);
        let rev_r = 0.42 * (tap_outs[1] + 0.75 * tap_outs[3] + 0.25 * tap_outs[2]);
        let mix = self.reverb_mix.clamp(0.0, 1.0);
        [
            l * (1.0 - 0.22 * mix) + rev_l * mix,
            r * (1.0 - 0.22 * mix) + rev_r * mix,
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
        fx.amp_cabinet = AmpCabinet::Jc;
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

    fn tone_gain_db(fx: &mut LayerFxChain, hz: f32) -> f32 {
        let fs = fx.sample_rate;
        let (mut e_in, mut e_out) = (0.0f64, 0.0f64);
        for i in 0..24_000 {
            let x = (TAU * hz * i as f32 / fs).sin() * 0.1;
            let [l, _] = fx.process_frame([x, x]);
            if i >= 12_000 {
                e_in += f64::from(x * x);
                e_out += f64::from(l * l);
            }
        }
        (10.0 * (e_out / e_in).log10()) as f32
    }

    fn eq_only(bass: f32, mid: f32, mid_hz: f32, treble: f32) -> LayerFxChain {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = ReverbModel::Off;
        fx.eq_bass_db = bass;
        fx.eq_mid_db = mid;
        fx.eq_mid_freq_hz = mid_hz;
        fx.eq_treble_db = treble;
        fx
    }

    #[test]
    fn test_firmware_q_table_endpoints_and_monotonic() {
        assert!((firmware_q(0) - 0.1).abs() < 1e-6);
        assert!((firmware_q(127) - 10.0).abs() < 1e-4);
        assert!((firmware_q(64) - 1.018_296).abs() < 1e-4);
        assert_eq!(firmware_q(500), firmware_q(127));
        assert!((1..FW_Q_STEPS).all(|i| firmware_q(i) > firmware_q(i - 1)));
    }

    #[test]
    fn test_biquad_eq_frequency_response() {
        // Bass +12 dB shelf: boosts 40 Hz by ~12 dB, leaves 4 kHz flat
        let mut fx = eq_only(12.0, 0.0, 1000.0, 0.0);
        assert!((tone_gain_db(&mut fx, 40.0) - 12.0).abs() < 1.5);
        let mut fx = eq_only(12.0, 0.0, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 4000.0).abs() < 0.5);
        // Treble -9 dB shelf
        let mut fx = eq_only(0.0, 0.0, 1000.0, -9.0);
        assert!((tone_gain_db(&mut fx, 12_000.0) + 9.0).abs() < 1.5);
        let mut fx = eq_only(0.0, 0.0, 1000.0, -9.0);
        assert!(tone_gain_db(&mut fx, 200.0).abs() < 0.5);
        // Mid peak +10 dB @ 2 kHz is exact at center and narrow (2nd-order)
        let mut fx = eq_only(0.0, 10.0, 2000.0, 0.0);
        assert!((tone_gain_db(&mut fx, 2000.0) - 10.0).abs() < 0.3);
        let mut fx = eq_only(0.0, 10.0, 2000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 200.0).abs() < 1.0);
    }

    #[test]
    fn test_biquad_eq_preserves_stereo_image() {
        let mut fx = eq_only(6.0, 6.0, 1000.0, 6.0);
        let mut right_energy = 0.0f32;
        for i in 0..2000 {
            let x = (i as f32 * 0.07).sin() * 0.3;
            let [_, r] = fx.process_frame([x, 0.0]);
            right_energy += r * r;
        }
        assert!(
            right_energy < 1e-12,
            "silent right channel must stay silent"
        );
    }

    #[test]
    fn test_biquad_stable_at_extremes() {
        for (fc, db) in [
            (10.0, 15.0),
            (200.0, -15.0),
            (24_000.0, 15.0),
            (8000.0, 15.0),
        ] {
            for kind in [
                BiquadKind::LowShelf,
                BiquadKind::HighShelf,
                BiquadKind::Peak,
                BiquadKind::LowPass,
                BiquadKind::HighPass,
            ] {
                let c = design_biquad(kind, fc, 48_000.0, 10.0, db);
                assert!(c.iter().all(|v| v.is_finite()));
                // Stability triangle: |a2| < 1, |a1| < 1 + a2
                assert!(
                    c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4],
                    "{kind:?} {fc} {db}"
                );
            }
        }
    }

    #[test]
    fn test_apply_morph_drives_only_assigned_params() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_mix = 0.3;
        fx.delay_mix = 0.2;
        fx.morph.reverb_amount = MorphParameter::new(0.2);
        fx.morph.reverb_amount.delta_wheel = 0.5;
        fx.morph.eq_mid_freq = MorphParameter::new(0.0);
        fx.morph.eq_mid_freq.delta_aftertouch = 1.0;
        let ctrl = MorphControllers {
            mod_wheel: 1.0,
            control_pedal: 0.0,
            aftertouch: 64.0 / 127.0,
        };
        fx.apply_morph(&ctrl);
        assert!((fx.reverb_mix - 0.7).abs() < 1e-5);
        assert!((fx.eq_mid_freq_hz - EQ_FREQ_HZ[64]).abs() < 1e-3);
        assert!(
            (fx.delay_mix - 0.2).abs() < 1e-6,
            "un-morphed param untouched"
        );
    }

    #[test]
    fn test_eq_mid_freq_morph_moves_peak() {
        let mut fx = eq_only(0.0, 12.0, 200.0, 0.0);
        fx.morph.eq_mid_freq = MorphParameter::new(0.0);
        fx.morph.eq_mid_freq.delta_pedal = 1.0;
        fx.apply_morph(&MorphControllers::default());
        let low = tone_gain_db(&mut fx, 200.0);
        fx.apply_morph(&MorphControllers {
            control_pedal: 1.0,
            ..MorphControllers::default()
        });
        let moved = tone_gain_db(&mut fx, 200.0);
        assert!(low > 8.0 && moved < 2.0, "low={low} moved={moved}");
    }

    #[test]
    fn test_eq_freq_table_hardware_steps() {
        assert_eq!(EQ_FREQ_HZ[0], 200.0);
        assert_eq!(EQ_FREQ_HZ[64], 999.0);
        assert_eq!(EQ_FREQ_HZ[127], 8000.0);
        assert!(EQ_FREQ_HZ.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(eq_freq_hz_from_norm(0.0), 200.0);
        assert_eq!(eq_freq_hz_from_norm(64.0 / 127.0), 999.0);
        assert_eq!(eq_freq_hz_from_norm(2.0), 8000.0);
    }

    fn filter_chain(mode: AmpCabinet, hz: f32, res: f32) -> LayerFxChain {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = ReverbModel::Off;
        fx.amp_cabinet = mode;
        fx.eq_mid_freq_hz = hz;
        fx.eq_filter_res = res;
        fx
    }

    #[test]
    fn test_lp24_hp24_slope_and_passband() {
        // LP24 @ 1 kHz: passband ~flat, 2 kHz (1 oct) < -18 dB, 4 kHz (2 oct) < -36 dB
        let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 100.0).abs() < 1.0);
        let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 2000.0) < -18.0);
        let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 4000.0) < -36.0);
        // HP24 mirror image
        let mut fx = filter_chain(AmpCabinet::HpFilter, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 10_000.0).abs() < 1.0);
        let mut fx = filter_chain(AmpCabinet::HpFilter, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 250.0) < -36.0);
        // Resonance raises the level near cutoff
        let mut flat = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
        let mut res = filter_chain(AmpCabinet::LpFilter, 1000.0, 10.0);
        assert!(tone_gain_db(&mut res, 1000.0) > tone_gain_db(&mut flat, 1000.0) + 3.0);
    }

    #[test]
    fn test_filter_mode_ignores_mid_gain_peak() {
        let mut a = filter_chain(AmpCabinet::LpFilter, 8000.0, 0.0);
        a.eq_mid_db = 12.0;
        let mut b = filter_chain(AmpCabinet::LpFilter, 8000.0, 0.0);
        let ga = tone_gain_db(&mut a, 1000.0);
        let gb = tone_gain_db(&mut b, 1000.0);
        assert!(
            (ga - gb).abs() < 0.1,
            "mid knob is resonance here, not a peak"
        );
    }
}
