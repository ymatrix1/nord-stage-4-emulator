//! Decoded Organ / Piano / Synth / FX / Global / Program bitstream structures.
//!
//! All bit offsets and morph encodings (`W`-bit base + three `(W+1)`-bit
//! Wheel/Aftertouch/Control-Pedal deltas with zero sentinel `max_val`) match
//! Nord Stage 4 OS v1.66 `Buffer_*` serialization and verified factory `.ns4b` presets.

use super::bitstream::{BitReader, BitWriter};
use super::container::{CbinError, CbinFile, CbinHeader, CbinKind};
use crate::fx::{Mod1Type, Mod2Type, ReverbModel};
use crate::morph::CrossfadeWidth;
use crate::organ::{OrganModel, RotarySpeed, ScannerMode};
use crate::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use crate::synth::{FilterModel, SynthOscMode, VaWaveform};
use serde::{Deserialize, Serialize};

const SPLIT_NOTE_TABLE: [u8; 10] = [41, 48, 53, 60, 65, 72, 77, 84, 89, 96];

/// Normalized (`-1.0..=1.0`) Morph deltas for `[ModWheel, Aftertouch, CtrlPedal]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MorphDeltas {
    pub wheel: f32,
    pub aftertouch: f32,
    pub ctrl_pedal: f32,
}

impl MorphDeltas {
    pub fn is_zero(self) -> bool {
        self.wheel.abs() < 1e-4 && self.aftertouch.abs() < 1e-4 && self.ctrl_pedal.abs() < 1e-4
    }
}

fn decode_signed(raw: u32, bits: usize) -> i32 {
    let sign_bit = 1u32 << (bits - 1);
    let mask = (1u32 << bits) - 1;
    let masked = raw & mask;
    if (masked & sign_bit) != 0 {
        (masked as i32) - ((1u32 << bits) as i32)
    } else {
        masked as i32
    }
}

fn encode_signed(val: i32, bits: usize) -> u32 {
    let mask = (1u32 << bits) - 1;
    (val as u32) & mask
}

fn read_morph(
    r: &mut BitReader,
    bit_pos: usize,
    width: usize,
    max_val: u32,
) -> Result<(u32, MorphDeltas), CbinError> {
    r.set_bit_pos(bit_pos)?;
    let base = r.read_bits(width)?;
    let max_i = max_val as i32;
    let max_f = max_val as f32;
    let w = (r.read_bits(width + 1)? as i32 - max_i) as f32 / max_f;
    let at = (r.read_bits(width + 1)? as i32 - max_i) as f32 / max_f;
    let cp = (r.read_bits(width + 1)? as i32 - max_i) as f32 / max_f;
    Ok((
        base,
        MorphDeltas {
            wheel: w,
            aftertouch: at,
            ctrl_pedal: cp,
        },
    ))
}

fn write_morph(
    w_out: &mut BitWriter,
    bit_pos: usize,
    base: u32,
    deltas: MorphDeltas,
    width: usize,
    max_val: u32,
) -> Result<(), CbinError> {
    w_out.set_bit_pos(bit_pos)?;
    w_out.write_bits(base.min(max_val), width)?;
    let max_i = max_val as i32;
    let max_f = max_val as f32;
    for d in [deltas.wheel, deltas.aftertouch, deltas.ctrl_pedal] {
        let raw = (max_i + (d * max_f).round() as i32).clamp(0, max_i * 2) as u32;
        w_out.write_bits(raw, width + 1)?;
    }
    Ok(())
}

fn prepare_buffer(raw_payload: &[u8], expected_len: usize, block_tag: u8) -> Vec<u8> {
    if raw_payload.len() >= expected_len {
        let mut out = raw_payload[..expected_len].to_vec();
        out[0] = block_tag;
        out
    } else {
        let mut out = vec![0u8; expected_len];
        out[0] = block_tag;
        out
    }
}

/// Exact 128-step hardware Synth Env Attack curve (ms) from NS4 OS v1.66 (`0x53abe4`).
pub const ENV_ATTACK_MS_TABLE: [f32; 128] = [
    0.5, 0.6, 0.7, 0.9, 1.1, 1.3, 1.5, 1.8, 2.1, 2.5, 3.0, 3.5, 4.0, 4.7, 5.5, 6.3, 7.3, 8.4, 9.7,
    11.0, 13.0, 14.0, 16.0, 19.0, 21.0, 24.0, 27.0, 31.0, 34.0, 39.0, 43.0, 49.0, 54.0, 61.0, 68.0,
    75.0, 84.0, 93.0, 103.0, 114.0, 126.0, 139.0, 153.0, 169.0, 186.0, 204.0, 224.0, 246.0, 269.0,
    295.0, 322.0, 352.0, 384.0, 419.0, 456.0, 496.0, 540.0, 586.0, 636.0, 690.0, 748.0, 810.0,
    876.0, 947.0, 1020.0, 1100.0, 1190.0, 1280.0, 1380.0, 1490.0, 1600.0, 1720.0, 1850.0, 1990.0,
    2130.0, 2280.0, 2450.0, 2620.0, 2810.0, 3000.0, 3210.0, 3430.0, 3660.0, 3910.0, 4170.0, 4450.0,
    4740.0, 5050.0, 5370.0, 5720.0, 6080.0, 6470.0, 6870.0, 7300.0, 7750.0, 8220.0, 8720.0, 9250.0,
    9800.0, 10000.0, 11000.0, 12000.0, 12000.0, 13000.0, 14000.0, 15000.0, 15000.0, 16000.0,
    17000.0, 18000.0, 19000.0, 20000.0, 21000.0, 22000.0, 24000.0, 25000.0, 26000.0, 27000.0,
    29000.0, 30000.0, 32000.0, 34000.0, 35000.0, 37000.0, 39000.0, 41000.0, 43000.0, 45000.0,
];

/// Exact 128-step hardware Synth Env Decay curve (ms) from NS4 OS v1.66 (`0x53ade4`; idx 127 = Hold).
pub const ENV_DECAY_MS_TABLE: [f32; 128] = [
    3.0, 3.5, 4.0, 4.6, 5.3, 6.0, 6.9, 7.9, 9.0, 10.0, 12.0, 13.0, 15.0, 17.0, 19.0, 21.0, 23.0,
    26.0, 29.0, 33.0, 36.0, 41.0, 45.0, 50.0, 55.0, 61.0, 68.0, 75.0, 82.0, 91.0, 100.0, 110.0,
    120.0, 132.0, 144.0, 158.0, 173.0, 188.0, 206.0, 224.0, 244.0, 265.0, 288.0, 313.0, 340.0,
    368.0, 399.0, 432.0, 467.0, 505.0, 545.0, 588.0, 634.0, 683.0, 736.0, 792.0, 851.0, 915.0,
    983.0, 1050.0, 1130.0, 1210.0, 1300.0, 1390.0, 1490.0, 1590.0, 1700.0, 1820.0, 1940.0, 2070.0,
    2210.0, 2360.0, 2510.0, 2670.0, 2850.0, 3030.0, 3220.0, 3420.0, 3640.0, 3860.0, 4100.0, 4350.0,
    4610.0, 4890.0, 5180.0, 5490.0, 5810.0, 6150.0, 6500.0, 6880.0, 7270.0, 7680.0, 8110.0, 8570.0,
    9040.0, 9540.0, 10000.0, 11000.0, 11000.0, 12000.0, 12000.0, 13000.0, 14000.0, 14000.0,
    15000.0, 16000.0, 17000.0, 18000.0, 19000.0, 20000.0, 20000.0, 22000.0, 23000.0, 24000.0,
    25000.0, 26000.0, 27000.0, 29000.0, 30000.0, 31000.0, 33000.0, 34000.0, 36000.0, 38000.0,
    39000.0, 41000.0, 43000.0, 45000.0,
];

/// Exact 128-step hardware Synth Env Release curve (ms) from NS4 OS v1.66 (`0x53afe4`).
pub const ENV_RELEASE_MS_TABLE: [f32; 128] = ENV_DECAY_MS_TABLE;

/// Exact 128-step hardware Synth Filter Cutoff curve (Hz) from NS4 OS v1.66 (`0x53b1e4`).
pub const FILTER_CUTOFF_HZ_TABLE: [f32; 128] = [
    14.0, 15.0, 15.0, 16.0, 17.0, 18.0, 19.0, 21.0, 22.0, 23.0, 24.0, 26.0, 27.0, 29.0, 31.0, 33.0,
    35.0, 37.0, 39.0, 41.0, 44.0, 46.0, 49.0, 52.0, 55.0, 58.0, 62.0, 65.0, 69.0, 73.0, 78.0, 82.0,
    87.0, 92.0, 98.0, 104.0, 110.0, 117.0, 123.0, 131.0, 139.0, 147.0, 156.0, 165.0, 175.0, 185.0,
    196.0, 208.0, 220.0, 233.0, 247.0, 262.0, 277.0, 294.0, 311.0, 330.0, 349.0, 370.0, 392.0,
    415.0, 440.0, 466.0, 494.0, 523.0, 554.0, 587.0, 622.0, 659.0, 698.0, 740.0, 784.0, 831.0,
    880.0, 932.0, 988.0, 1000.0, 1100.0, 1200.0, 1200.0, 1300.0, 1400.0, 1500.0, 1600.0, 1700.0,
    1800.0, 1900.0, 2000.0, 2100.0, 2200.0, 2300.0, 2500.0, 2600.0, 2800.0, 3000.0, 3100.0, 3300.0,
    3500.0, 3700.0, 4000.0, 4200.0, 4400.0, 4700.0, 5000.0, 5300.0, 5600.0, 5900.0, 6300.0, 6600.0,
    7000.0, 7500.0, 7900.0, 8400.0, 8900.0, 9400.0, 10000.0, 11000.0, 11000.0, 12000.0, 13000.0,
    14000.0, 16000.0, 18000.0, 19000.0, 21000.0, 24000.0, 26000.0, 29000.0, 32000.0,
];

/// Exact 128-step hardware Synth LFO Rate curve (Hz) from NS4 OS v1.66 (`0x53b3e4`).
pub const LFO_RATE_HZ_TABLE: [f32; 128] = [
    0.03, 0.03, 0.03, 0.04, 0.04, 0.04, 0.05, 0.05, 0.05, 0.06, 0.06, 0.07, 0.07, 0.08, 0.09, 0.09,
    0.1, 0.11, 0.12, 0.13, 0.14, 0.15, 0.16, 0.17, 0.19, 0.2, 0.22, 0.24, 0.26, 0.28, 0.3, 0.32,
    0.35, 0.38, 0.41, 0.44, 0.47, 0.51, 0.55, 0.6, 0.64, 0.7, 0.75, 0.81, 0.88, 0.95, 1.0, 1.1,
    1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 1.9, 2.0, 2.2, 2.4, 2.6, 2.8, 3.0, 3.2, 3.5, 3.8, 4.1, 4.4, 4.8,
    5.2, 5.6, 6.0, 6.5, 7.0, 7.6, 8.2, 8.8, 9.5, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 18.0,
    19.0, 21.0, 22.0, 24.0, 26.0, 28.0, 30.0, 33.0, 35.0, 38.0, 41.0, 45.0, 48.0, 52.0, 56.0, 61.0,
    65.0, 71.0, 76.0, 82.0, 89.0, 96.0, 104.0, 112.0, 121.0, 131.0, 141.0, 153.0, 165.0, 178.0,
    192.0, 208.0, 224.0, 242.0, 262.0, 283.0, 305.0, 330.0, 356.0, 385.0, 415.0, 449.0, 484.0,
    523.0,
];

fn table_val_to_idx(table: &[f32; 128], val: f32) -> u32 {
    let target = val.max(table[0]).ln();
    let mut best_idx = 0usize;
    let mut best_err = f32::INFINITY;
    for (i, &entry) in table.iter().enumerate() {
        let err = (target - entry.ln()).abs();
        if err < best_err - 1e-6 {
            best_err = err;
            best_idx = i;
        }
    }
    best_idx as u32
}

/// Converts normalized `0.0..=1.0` cutoff knob/morph position to Hz via the OS v1.66 hardware curve.
pub fn cutoff_norm_to_hz(norm: f32) -> f32 {
    let pos = norm.clamp(0.0, 1.0) * 127.0;
    let lo = (pos.floor() as usize).min(127);
    let hi = (lo + 1).min(127);
    let frac = pos - lo as f32;
    let a = FILTER_CUTOFF_HZ_TABLE[lo];
    let b = FILTER_CUTOFF_HZ_TABLE[hi];
    a * (b / a).powf(frac)
}

/// Converts cutoff Hz to normalized `0.0..=1.0` via the inverse OS v1.66 hardware curve.
pub fn cutoff_hz_to_norm(hz: f32) -> f32 {
    if hz <= FILTER_CUTOFF_HZ_TABLE[0] {
        return 0.0;
    }
    if hz >= FILTER_CUTOFF_HZ_TABLE[127] {
        return 1.0;
    }
    for i in 0..127 {
        let a = FILTER_CUTOFF_HZ_TABLE[i];
        let b = FILTER_CUTOFF_HZ_TABLE[i + 1];
        if hz <= b {
            let frac = if b > a {
                ((hz / a).ln() / (b / a).ln()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return (i as f32 + frac) / 127.0;
        }
    }
    1.0
}

/// Computes Clavia OS v1.66 (`ARM0 @ 0x0a5a44`) preset sound-name hash (MurmurHash2 with BE 4-byte reads, seed = len, `0 -> 1`).
pub fn nord_name_hash(stem: &str) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let data = stem.as_bytes();
    let mut h: u32 = data.len() as u32;
    let (chunks, rem) = data.as_chunks::<4>();
    for &chunk in chunks {
        let mut k = u32::from_be_bytes(chunk);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M);
        h ^= k;
    }
    if rem.len() == 3 {
        h ^= u32::from(rem[2]) << 16;
    }
    if rem.len() >= 2 {
        h ^= u32::from(rem[1]) << 8;
    }
    if !rem.is_empty() {
        h ^= u32::from(rem[0]);
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    if h == 0 {
        1
    } else {
        h
    }
}

/// Checks whether a sound stem matches a `0x0B` Piano `name_hash` or `0x32` Synth `sample_id` (raw or ASCII uppercase).
pub fn nord_name_hash_matches(stem: &str, target: u32) -> bool {
    target != 0
        && (nord_name_hash(stem) == target || nord_name_hash(&stem.to_ascii_uppercase()) == target)
}

fn split_idx_to_midi(idx: u32) -> u8 {
    SPLIT_NOTE_TABLE[(idx as usize).min(SPLIT_NOTE_TABLE.len() - 1)]
}

fn midi_to_split_idx(midi: u8) -> u32 {
    SPLIT_NOTE_TABLE
        .iter()
        .enumerate()
        .min_by_key(|(_, &n)| n.abs_diff(midi))
        .map_or(4, |(idx, _)| idx as u32)
}

/// Decoded hardware per-layer FX chain block (`0x36`, 55 bytes = 440 bits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxDecoded {
    pub mod1_enabled: bool,
    pub mod1_clock_sync: bool,
    pub mod1_type_raw: u8,
    pub mod1_rate: f32,
    pub mod1_rate_morph: MorphDeltas,
    pub mod1_amount: f32,
    pub mod1_amount_morph: MorphDeltas,
    pub mod2_enabled: bool,
    pub mod2_type_raw: u8,
    pub mod2_rate: f32,
    pub mod2_rate_morph: MorphDeltas,
    pub mod2_amount: f32,
    pub mod2_amount_morph: MorphDeltas,
    pub amp_enabled: bool,
    pub amp_type_new: u16,
    pub eq_treble_db: f32,
    pub eq_mid_db: f32,
    pub eq_bass_db: f32,
    pub eq_mid_freq_norm: f32,
    pub eq_mid_freq_morph: MorphDeltas,
    pub amp_drive: f32,
    pub amp_drive_morph: MorphDeltas,
    pub comp_enabled: bool,
    pub comp_amount: f32,
    pub comp_fast: bool,
    pub delay_enabled: bool,
    pub delay_clock_sync: bool,
    pub delay_rate_norm: f32,
    pub delay_rate_morph: MorphDeltas,
    pub delay_mix: f32,
    pub delay_mix_morph: MorphDeltas,
    pub delay_mode: u8,
    pub delay_ping_pong: bool,
    pub delay_fb_filter: bool,
    pub delay_feedback: f32,
    pub delay_feedback_morph: MorphDeltas,
    pub delay_fb_fx_type: u8,
    pub reverb_enabled: bool,
    pub reverb_type_raw: u8,
    pub reverb_color: u8,
    pub reverb_amount: f32,
    pub reverb_amount_morph: MorphDeltas,
    #[serde(skip)]
    raw_payload: Vec<u8>,
}

impl Default for FxDecoded {
    fn default() -> Self {
        Self {
            mod1_enabled: false,
            mod1_clock_sync: false,
            mod1_type_raw: 0,
            mod1_rate: 0.4,
            mod1_rate_morph: MorphDeltas::default(),
            mod1_amount: 0.5,
            mod1_amount_morph: MorphDeltas::default(),
            mod2_enabled: false,
            mod2_type_raw: 4, // Chorus Analog
            mod2_rate: 0.3,
            mod2_rate_morph: MorphDeltas::default(),
            mod2_amount: 0.4,
            mod2_amount_morph: MorphDeltas::default(),
            amp_enabled: false,
            amp_type_new: 0,
            eq_treble_db: 0.0,
            eq_mid_db: 0.0,
            eq_bass_db: 0.0,
            eq_mid_freq_norm: 64.0 / 127.0,
            eq_mid_freq_morph: MorphDeltas::default(),
            amp_drive: 0.0,
            amp_drive_morph: MorphDeltas::default(),
            comp_enabled: false,
            comp_amount: 0.0,
            comp_fast: false,
            delay_enabled: false,
            delay_clock_sync: false,
            delay_rate_norm: 0.25,
            delay_rate_morph: MorphDeltas::default(),
            delay_mix: 0.0,
            delay_mix_morph: MorphDeltas::default(),
            delay_mode: 0,
            delay_ping_pong: false,
            delay_fb_filter: false,
            delay_feedback: 0.3,
            delay_feedback_morph: MorphDeltas::default(),
            delay_fb_fx_type: 0,
            reverb_enabled: false,
            reverb_type_raw: 3, // Stage
            reverb_color: 0,
            reverb_amount: 0.0,
            reverb_amount_morph: MorphDeltas::default(),
            raw_payload: Vec::new(),
        }
    }
}

impl PartialEq for FxDecoded {
    fn eq(&self, other: &Self) -> bool {
        self.mod1_enabled == other.mod1_enabled
            && self.mod1_clock_sync == other.mod1_clock_sync
            && self.mod1_type_raw == other.mod1_type_raw
            && self.mod1_rate == other.mod1_rate
            && self.mod1_rate_morph == other.mod1_rate_morph
            && self.mod1_amount == other.mod1_amount
            && self.mod1_amount_morph == other.mod1_amount_morph
            && self.mod2_enabled == other.mod2_enabled
            && self.mod2_type_raw == other.mod2_type_raw
            && self.mod2_rate == other.mod2_rate
            && self.mod2_rate_morph == other.mod2_rate_morph
            && self.mod2_amount == other.mod2_amount
            && self.mod2_amount_morph == other.mod2_amount_morph
            && self.amp_enabled == other.amp_enabled
            && self.amp_type_new == other.amp_type_new
            && self.eq_treble_db == other.eq_treble_db
            && self.eq_mid_db == other.eq_mid_db
            && self.eq_bass_db == other.eq_bass_db
            && self.eq_mid_freq_norm == other.eq_mid_freq_norm
            && self.eq_mid_freq_morph == other.eq_mid_freq_morph
            && self.amp_drive == other.amp_drive
            && self.amp_drive_morph == other.amp_drive_morph
            && self.comp_enabled == other.comp_enabled
            && self.comp_amount == other.comp_amount
            && self.comp_fast == other.comp_fast
            && self.delay_enabled == other.delay_enabled
            && self.delay_clock_sync == other.delay_clock_sync
            && self.delay_rate_norm == other.delay_rate_norm
            && self.delay_rate_morph == other.delay_rate_morph
            && self.delay_mix == other.delay_mix
            && self.delay_mix_morph == other.delay_mix_morph
            && self.delay_mode == other.delay_mode
            && self.delay_ping_pong == other.delay_ping_pong
            && self.delay_fb_filter == other.delay_fb_filter
            && self.delay_feedback == other.delay_feedback
            && self.delay_feedback_morph == other.delay_feedback_morph
            && self.delay_fb_fx_type == other.delay_fb_fx_type
            && self.reverb_enabled == other.reverb_enabled
            && self.reverb_type_raw == other.reverb_type_raw
            && self.reverb_color == other.reverb_color
            && self.reverb_amount == other.reverb_amount
            && self.reverb_amount_morph == other.reverb_amount_morph
    }
}

impl FxDecoded {
    pub const BLOCK_TAG: u8 = 0x36;
    pub const PAYLOAD_LEN: usize = 55;

    /// Maps hardware `Edt_Effects_kFx1Type` (0..=11) into `Mod1Type`.
    pub fn mod1_type(&self) -> Mod1Type {
        if !self.mod1_enabled {
            return Mod1Type::Off;
        }
        match self.mod1_type_raw {
            0 | 1 => Mod1Type::AutoPan,
            2 | 3 | 10 | 11 => Mod1Type::Tremolo,
            4 | 5 => Mod1Type::RingMod,
            6..=9 => Mod1Type::Wah,
            _ => Mod1Type::Tremolo,
        }
    }

    /// Maps hardware `Edt_Effects_kFx2Type` (0..=11) into `Mod2Type`.
    pub fn mod2_type(&self) -> Mod2Type {
        if !self.mod2_enabled {
            return Mod2Type::Off;
        }
        match self.mod2_type_raw {
            0 | 1 => Mod2Type::Phaser,
            2 | 3 => Mod2Type::Flanger,
            4 | 7 | 8 => Mod2Type::Chorus,
            5 | 6 | 9..=11 => Mod2Type::Vibe,
            _ => Mod2Type::Chorus,
        }
    }

    /// Maps hardware `Edt_Effects_kRvbType` (0..=5) into `ReverbModel`.
    pub fn reverb_model(&self) -> ReverbModel {
        if !self.reverb_enabled {
            return ReverbModel::Off;
        }
        match self.reverb_type_raw {
            0 | 1 => ReverbModel::Booth,
            2 => ReverbModel::Room,
            3 => ReverbModel::Stage,
            4 => ReverbModel::Hall,
            5 => ReverbModel::Cathedral,
            _ => ReverbModel::Stage,
        }
    }

    /// Computes EQ Mid center frequency in Hz (`200..8000 Hz`).
    pub fn eq_mid_freq_hz(&self) -> f32 {
        200.0 * 40.0f32.powf(self.eq_mid_freq_norm.clamp(0.0, 1.0))
    }

    /// Computes Delay time in milliseconds (`20..2000 ms`).
    pub fn delay_time_ms(&self) -> f32 {
        20.0 + self.delay_rate_norm.clamp(0.0, 1.0) * 1980.0
    }

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(&slice[..Self::PAYLOAD_LEN]);
        r.set_bit_pos(8)?;

        let mod1_enabled = r.read_bool()?;
        let mod1_clock_sync = r.read_bool()?;
        let (mod1_rate_raw, mod1_rate_morph) = read_morph(&mut r, 10, 7, 127)?;
        let (mod1_amt_raw, mod1_amount_morph) = read_morph(&mut r, 41, 7, 127)?;
        r.set_bit_pos(72)?;
        let mod1_type_raw = r.read_bits(4)? as u8;

        let mod2_enabled = r.read_bool()?;
        let (mod2_rate_raw, mod2_rate_morph) = read_morph(&mut r, 77, 7, 127)?;
        let (mod2_amt_raw, mod2_amount_morph) = read_morph(&mut r, 108, 7, 127)?;
        r.set_bit_pos(139)?;
        let mod2_type_raw = r.read_bits(4)? as u8;

        let amp_enabled = r.read_bool()?;
        let eq_treble_db = decode_signed(r.read_bits(7)?, 7) as f32 * 0.25;
        let eq_mid_db = decode_signed(r.read_bits(7)?, 7) as f32 * 0.25;
        let eq_bass_db = decode_signed(r.read_bits(7)?, 7) as f32 * 0.25;
        let (eq_mid_freq_raw, eq_mid_freq_morph) = read_morph(&mut r, 165, 7, 127)?;
        let (amp_drive_raw, amp_drive_morph) = read_morph(&mut r, 196, 7, 127)?;

        r.set_bit_pos(231)?;
        let comp_enabled = r.read_bool()?;
        let comp_amount = r.read_bits(7)? as f32 / 127.0;
        let comp_fast = r.read_bool()?;

        let delay_enabled = r.read_bool()?;
        let delay_clock_sync = r.read_bool()?;
        let (dly_rate_raw, delay_rate_morph) = read_morph(&mut r, 242, 14, 16383)?;
        let (dly_mix_raw, delay_mix_morph) = read_morph(&mut r, 301, 7, 127)?;
        r.set_bit_pos(332)?;
        let delay_mode = r.read_bits(2)? as u8;
        let delay_ping_pong = r.read_bool()?;
        let delay_fb_filter = r.read_bool()?;
        let (dly_fb_raw, delay_feedback_morph) = read_morph(&mut r, 336, 7, 127)?;
        r.set_bit_pos(367)?;
        let delay_fb_fx_type = r.read_bits(4)? as u8;

        let reverb_enabled = r.read_bool()?;
        let (rvb_amt_raw, reverb_amount_morph) = read_morph(&mut r, 372, 7, 127)?;
        r.set_bit_pos(403)?;
        let reverb_color = r.read_bits(2)? as u8;
        let reverb_type_raw = r.read_bits(3)? as u8;
        let amp_type_new = r.read_bits(9)? as u16;

        Ok(Self {
            mod1_enabled,
            mod1_clock_sync,
            mod1_type_raw,
            mod1_rate: mod1_rate_raw as f32 / 127.0,
            mod1_rate_morph,
            mod1_amount: mod1_amt_raw as f32 / 127.0,
            mod1_amount_morph,
            mod2_enabled,
            mod2_type_raw,
            mod2_rate: mod2_rate_raw as f32 / 127.0,
            mod2_rate_morph,
            mod2_amount: mod2_amt_raw as f32 / 127.0,
            mod2_amount_morph,
            amp_enabled,
            amp_type_new,
            eq_treble_db,
            eq_mid_db,
            eq_bass_db,
            eq_mid_freq_norm: eq_mid_freq_raw as f32 / 127.0,
            eq_mid_freq_morph,
            amp_drive: (amp_drive_raw as f32 / 127.0) * 10.0,
            amp_drive_morph,
            comp_enabled,
            comp_amount,
            comp_fast,
            delay_enabled,
            delay_clock_sync,
            delay_rate_norm: dly_rate_raw as f32 / 16383.0,
            delay_rate_morph,
            delay_mix: dly_mix_raw as f32 / 127.0,
            delay_mix_morph,
            delay_mode,
            delay_ping_pong,
            delay_fb_filter,
            delay_feedback: dly_fb_raw as f32 / 127.0,
            delay_feedback_morph,
            delay_fb_fx_type,
            reverb_enabled,
            reverb_type_raw,
            reverb_color,
            reverb_amount: rvb_amt_raw as f32 / 127.0,
            reverb_amount_morph,
            raw_payload: slice[..Self::PAYLOAD_LEN].to_vec(),
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = prepare_buffer(&self.raw_payload, Self::PAYLOAD_LEN, Self::BLOCK_TAG);
        let mut w = BitWriter::new(&mut out);

        w.set_bit_pos(8)?;
        w.write_bool(self.mod1_enabled)?;
        w.write_bool(self.mod1_clock_sync)?;
        write_morph(
            &mut w,
            10,
            (self.mod1_rate.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.mod1_rate_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            41,
            (self.mod1_amount.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.mod1_amount_morph,
            7,
            127,
        )?;
        w.set_bit_pos(72)?;
        w.write_bits(u32::from(self.mod1_type_raw & 0x0F), 4)?;

        w.write_bool(self.mod2_enabled)?;
        write_morph(
            &mut w,
            77,
            (self.mod2_rate.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.mod2_rate_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            108,
            (self.mod2_amount.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.mod2_amount_morph,
            7,
            127,
        )?;
        w.set_bit_pos(139)?;
        w.write_bits(u32::from(self.mod2_type_raw & 0x0F), 4)?;

        w.write_bool(self.amp_enabled)?;
        w.write_bits(
            encode_signed(
                (self.eq_treble_db * 4.0).round().clamp(-60.0, 60.0) as i32,
                7,
            ),
            7,
        )?;
        w.write_bits(
            encode_signed((self.eq_mid_db * 4.0).round().clamp(-60.0, 60.0) as i32, 7),
            7,
        )?;
        w.write_bits(
            encode_signed((self.eq_bass_db * 4.0).round().clamp(-60.0, 60.0) as i32, 7),
            7,
        )?;
        write_morph(
            &mut w,
            165,
            (self.eq_mid_freq_norm.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.eq_mid_freq_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            196,
            ((self.amp_drive / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
            self.amp_drive_morph,
            7,
            127,
        )?;

        w.set_bit_pos(231)?;
        w.write_bool(self.comp_enabled)?;
        w.write_bits((self.comp_amount.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
        w.write_bool(self.comp_fast)?;

        w.write_bool(self.delay_enabled)?;
        w.write_bool(self.delay_clock_sync)?;
        write_morph(
            &mut w,
            242,
            (self.delay_rate_norm.clamp(0.0, 1.0) * 16383.0).round() as u32,
            self.delay_rate_morph,
            14,
            16383,
        )?;
        write_morph(
            &mut w,
            301,
            (self.delay_mix.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.delay_mix_morph,
            7,
            127,
        )?;
        w.set_bit_pos(332)?;
        w.write_bits(u32::from(self.delay_mode & 0x03), 2)?;
        w.write_bool(self.delay_ping_pong)?;
        w.write_bool(self.delay_fb_filter)?;
        write_morph(
            &mut w,
            336,
            (self.delay_feedback.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.delay_feedback_morph,
            7,
            127,
        )?;
        w.set_bit_pos(367)?;
        w.write_bits(u32::from(self.delay_fb_fx_type & 0x0F), 4)?;

        w.write_bool(self.reverb_enabled)?;
        write_morph(
            &mut w,
            372,
            (self.reverb_amount.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.reverb_amount_morph,
            7,
            127,
        )?;
        w.set_bit_pos(403)?;
        w.write_bits(u32::from(self.reverb_color & 0x03), 2)?;
        w.write_bits(u32::from(self.reverb_type_raw & 0x07), 3)?;
        w.write_bits(u32::from(self.amp_type_new & 0x01FF), 9)?;

        Ok(out)
    }
}

/// Decoded Organ section configuration across Layer A, Layer B, and shared FX (`0x11`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganPresetDecoded {
    pub model: OrganModel,
    pub model_b: OrganModel,
    pub percussion_on: bool,
    pub percussion_soft: bool,
    pub percussion_fast: bool,
    pub percussion_third: bool,
    pub vibrato_on: bool,
    pub vibrato_on_b: bool,
    pub vibrato_type: u8,
    pub rotary_enabled: bool,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    pub layer_a_enabled: bool,
    pub layer_a_level: f32,
    pub layer_a_level_morph: MorphDeltas,
    pub layer_a_octave: i8,
    pub drawbars_a: [u8; 9],
    pub drawbars_a_morph: [MorphDeltas; 9],
    pub layer_b_enabled: bool,
    pub layer_b_level: f32,
    pub layer_b_level_morph: MorphDeltas,
    pub layer_b_octave: i8,
    pub drawbars_b: [u8; 9],
    pub drawbars_b_morph: [MorphDeltas; 9],
    pub enable_ii_mask: u8,
    pub fx: FxDecoded,
    #[serde(skip)]
    raw_payload: Vec<u8>,
}

impl Default for OrganPresetDecoded {
    fn default() -> Self {
        Self {
            model: OrganModel::B3,
            model_b: OrganModel::B3,
            percussion_on: false,
            percussion_soft: false,
            percussion_fast: false,
            percussion_third: false,
            vibrato_on: false,
            vibrato_on_b: false,
            vibrato_type: 5, // C3 default
            rotary_enabled: true,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.0,
            layer_a_enabled: true,
            layer_a_level: 0.8,
            layer_a_level_morph: MorphDeltas::default(),
            layer_a_octave: 0,
            drawbars_a: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            drawbars_a_morph: [MorphDeltas::default(); 9],
            layer_b_enabled: false,
            layer_b_level: 0.8,
            layer_b_level_morph: MorphDeltas::default(),
            layer_b_octave: 0,
            drawbars_b: [8, 8, 8, 8, 8, 8, 8, 8, 8],
            drawbars_b_morph: [MorphDeltas::default(); 9],
            enable_ii_mask: 0b01,
            fx: FxDecoded::default(),
            raw_payload: Vec::new(),
        }
    }
}

impl PartialEq for OrganPresetDecoded {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
            && self.model_b == other.model_b
            && self.percussion_on == other.percussion_on
            && self.percussion_soft == other.percussion_soft
            && self.percussion_fast == other.percussion_fast
            && self.percussion_third == other.percussion_third
            && self.vibrato_on == other.vibrato_on
            && self.vibrato_on_b == other.vibrato_on_b
            && self.vibrato_type == other.vibrato_type
            && self.rotary_enabled == other.rotary_enabled
            && self.rotary_speed == other.rotary_speed
            && self.rotary_drive == other.rotary_drive
            && self.layer_a_enabled == other.layer_a_enabled
            && self.layer_a_level == other.layer_a_level
            && self.layer_a_level_morph == other.layer_a_level_morph
            && self.layer_a_octave == other.layer_a_octave
            && self.drawbars_a == other.drawbars_a
            && self.drawbars_a_morph == other.drawbars_a_morph
            && self.layer_b_enabled == other.layer_b_enabled
            && self.layer_b_level == other.layer_b_level
            && self.layer_b_level_morph == other.layer_b_level_morph
            && self.layer_b_octave == other.layer_b_octave
            && self.drawbars_b == other.drawbars_b
            && self.drawbars_b_morph == other.drawbars_b_morph
            && self.enable_ii_mask == other.enable_ii_mask
            && self.fx == other.fx
    }
}

struct OrganLayerRaw {
    model: OrganModel,
    octave: i8,
    drawbars: [u8; 9],
    drawbar_morphs: [MorphDeltas; 9],
    vibrato_on: bool,
    perc_on: bool,
    perc_third: bool,
    perc_fast: bool,
    perc_soft: bool,
}

fn decode_organ_model(idx: u32) -> OrganModel {
    match idx {
        0 => OrganModel::B3,
        1 => OrganModel::Vox,
        2 => OrganModel::Farfisa,
        3 => OrganModel::Pipe1,
        _ => OrganModel::Pipe2,
    }
}

fn encode_organ_model(model: OrganModel) -> u32 {
    match model {
        OrganModel::B3 => 0,
        OrganModel::Vox => 1,
        OrganModel::Farfisa => 2,
        OrganModel::Pipe1 => 3,
        OrganModel::Pipe2 => 4,
    }
}

fn unpack_organ_layer_block(r: &mut BitReader, base: usize) -> Result<OrganLayerRaw, CbinError> {
    r.set_bit_pos(base + 12)?;
    let octave = decode_signed(r.read_bits(4)?, 4).clamp(-2, 2) as i8;
    r.set_bit_pos(base + 17)?;
    let model = decode_organ_model(r.read_bits(3)?);

    let mut drawbars = [0u8; 9];
    let mut drawbar_morphs = [MorphDeltas::default(); 9];
    for i in 0..9 {
        let (db, morph) = read_morph(r, base + 40 + i * 19, 4, 8)?;
        drawbars[i] = (db as u8).min(8);
        drawbar_morphs[i] = morph;
    }

    r.set_bit_pos(base + 232)?;
    let vibrato_on = r.read_bool()?;
    let perc_on = r.read_bool()?;
    let perc_third = r.read_bool()?;
    let perc_fast = r.read_bool()?;
    let perc_soft = r.read_bool()?;

    Ok(OrganLayerRaw {
        model,
        octave,
        drawbars,
        drawbar_morphs,
        vibrato_on,
        perc_on,
        perc_third,
        perc_fast,
        perc_soft,
    })
}

fn pack_organ_layer_block(
    w: &mut BitWriter,
    base: usize,
    layer: &OrganLayerRaw,
) -> Result<(), CbinError> {
    w.set_bit_pos(base)?;
    w.write_bits(0x03, 8)?;
    w.set_bit_pos(base + 12)?;
    w.write_bits(encode_signed(i32::from(layer.octave.clamp(-2, 2)), 4), 4)?;
    w.set_bit_pos(base + 17)?;
    w.write_bits(encode_organ_model(layer.model), 3)?;

    for i in 0..9 {
        write_morph(
            w,
            base + 40 + i * 19,
            u32::from(layer.drawbars[i].min(8)),
            layer.drawbar_morphs[i],
            4,
            8,
        )?;
    }

    w.set_bit_pos(base + 232)?;
    w.write_bool(layer.vibrato_on)?;
    w.write_bool(layer.perc_on)?;
    w.write_bool(layer.perc_third)?;
    w.write_bool(layer.perc_fast)?;
    w.write_bool(layer.perc_soft)?;
    Ok(())
}

impl OrganPresetDecoded {
    pub const BLOCK_TAG: u8 = 0x11;
    pub const PAYLOAD_LEN: usize = 135;

    /// Converts hardware `Edt_OrgCom_kVibratoTypeB3` (0..=5) to `ScannerMode`.
    pub fn scanner_mode(&self, layer_vibrato_on: bool) -> ScannerMode {
        if !layer_vibrato_on {
            return ScannerMode::Off;
        }
        match self.vibrato_type {
            0 => ScannerMode::V1,
            1 => ScannerMode::C1,
            2 => ScannerMode::V2,
            3 => ScannerMode::C2,
            4 => ScannerMode::V3,
            _ => ScannerMode::C3,
        }
    }

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(&slice[..Self::PAYLOAD_LEN]);
        r.set_bit_pos(9)?;
        let enable_i = r.read_bits(2)? as u8;
        r.set_bit_pos(12)?;
        let enable_ii_mask = r.read_bits(2)? as u8;

        let (lvl_a_raw, layer_a_level_morph) = read_morph(&mut r, 14, 7, 127)?;
        let (lvl_b_raw, layer_b_level_morph) = read_morph(&mut r, 45, 7, 127)?;

        r.set_bit_pos(80)?;
        let rotary_enabled = r.read_bool()?;
        let leslie_bits = r.read_bits(2)?;
        let rotary_speed = match leslie_bits {
            0 | 2 => RotarySpeed::Stop,
            1 => RotarySpeed::Slow,
            _ => RotarySpeed::Fast,
        };
        r.set_bit_pos(92)?;
        let rotary_drive = (r.read_bits(7)? as f32 / 127.0) * 10.0;
        r.set_bit_pos(130)?;
        let vibrato_type = (r.read_bits(3)? as u8).min(5);

        let lay_a = unpack_organ_layer_block(&mut r, 144)?;
        let lay_b = unpack_organ_layer_block(&mut r, 392)?;
        let fx = FxDecoded::unpack(&slice[80..135])?;

        Ok(Self {
            model: lay_a.model,
            model_b: lay_b.model,
            percussion_on: lay_a.perc_on,
            percussion_soft: lay_a.perc_soft,
            percussion_fast: lay_a.perc_fast,
            percussion_third: lay_a.perc_third,
            vibrato_on: lay_a.vibrato_on,
            vibrato_on_b: lay_b.vibrato_on,
            vibrato_type,
            rotary_enabled,
            rotary_speed,
            rotary_drive,
            layer_a_enabled: (enable_i & 0x01) != 0,
            layer_a_level: lvl_a_raw as f32 / 127.0,
            layer_a_level_morph,
            layer_a_octave: lay_a.octave,
            drawbars_a: lay_a.drawbars,
            drawbars_a_morph: lay_a.drawbar_morphs,
            layer_b_enabled: (enable_i & 0x02) != 0,
            layer_b_level: lvl_b_raw as f32 / 127.0,
            layer_b_level_morph,
            layer_b_octave: lay_b.octave,
            drawbars_b: lay_b.drawbars,
            drawbars_b_morph: lay_b.drawbar_morphs,
            enable_ii_mask,
            fx,
            raw_payload: slice[..Self::PAYLOAD_LEN].to_vec(),
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = prepare_buffer(&self.raw_payload, Self::PAYLOAD_LEN, Self::BLOCK_TAG);
        let mut w = BitWriter::new(&mut out);

        let enable_i = (u32::from(self.layer_b_enabled) << 1) | u32::from(self.layer_a_enabled);
        w.set_bit_pos(9)?;
        w.write_bits(enable_i, 2)?;
        w.set_bit_pos(12)?;
        w.write_bits(u32::from(self.enable_ii_mask & 0x03), 2)?;

        write_morph(
            &mut w,
            14,
            (self.layer_a_level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_a_level_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            45,
            (self.layer_b_level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_b_level_morph,
            7,
            127,
        )?;

        w.set_bit_pos(80)?;
        w.write_bool(self.rotary_enabled)?;
        let leslie_bits = match self.rotary_speed {
            RotarySpeed::Stop => 0b00,
            RotarySpeed::Slow => 0b01,
            RotarySpeed::Fast => 0b11,
        };
        w.write_bits(leslie_bits, 2)?;
        w.set_bit_pos(92)?;
        w.write_bits(
            ((self.rotary_drive / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
            7,
        )?;
        w.set_bit_pos(130)?;
        w.write_bits(u32::from(self.vibrato_type.min(5)), 3)?;

        pack_organ_layer_block(
            &mut w,
            144,
            &OrganLayerRaw {
                model: self.model,
                octave: self.layer_a_octave,
                drawbars: self.drawbars_a,
                drawbar_morphs: self.drawbars_a_morph,
                vibrato_on: self.vibrato_on,
                perc_on: self.percussion_on,
                perc_third: self.percussion_third,
                perc_fast: self.percussion_fast,
                perc_soft: self.percussion_soft,
            },
        )?;
        pack_organ_layer_block(
            &mut w,
            392,
            &OrganLayerRaw {
                model: self.model_b,
                octave: self.layer_b_octave,
                drawbars: self.drawbars_b,
                drawbar_morphs: self.drawbars_b_morph,
                vibrato_on: self.vibrato_on_b,
                perc_on: false,
                perc_third: false,
                perc_fast: false,
                perc_soft: false,
            },
        )?;

        let fx_bytes = self.fx.pack()?;
        out[80..135].copy_from_slice(&fx_bytes);
        Ok(out)
    }
}

/// Decoded single Piano layer configuration (`0x0B`, 12 bytes = 96 bits).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PianoLayerDecoded {
    pub enabled: bool,
    pub level: f32,
    pub level_morph: MorphDeltas,
    pub octave: i8,
    pub category: PianoCategory,
    pub model_id: u8,
    pub variation: u8,
    pub name_hash: u32,
    pub timbre: PianoTimbre,
    pub touch: VelocityTouch,
    pub string_res: bool,
    pub acoustic_level: u8,
    pub pedal_noise: bool,
    pub soft_release: bool,
    pub dyn_comp: u8,
    pub unison: u8,
}

impl Default for PianoLayerDecoded {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 0.8,
            level_morph: MorphDeltas::default(),
            octave: 0,
            category: PianoCategory::Grand,
            model_id: 1,
            variation: 0,
            name_hash: 0,
            timbre: PianoTimbre::Off,
            touch: VelocityTouch::Medium,
            string_res: true,
            acoustic_level: 2,
            pedal_noise: true,
            soft_release: false,
            dyn_comp: 0,
            unison: 0,
        }
    }
}

/// Decoded Piano section configuration across Layer A, Layer B, and per-layer FX (`0x0C`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoPresetDecoded {
    pub layer_a: PianoLayerDecoded,
    pub layer_b: PianoLayerDecoded,
    pub enable_ii_mask: u8,
    pub fx_a: FxDecoded,
    pub fx_b: FxDecoded,
    #[serde(skip)]
    raw_payload: Vec<u8>,
}

impl Default for PianoPresetDecoded {
    fn default() -> Self {
        Self {
            layer_a: PianoLayerDecoded::default(),
            layer_b: PianoLayerDecoded {
                enabled: false,
                category: PianoCategory::Upright,
                ..PianoLayerDecoded::default()
            },
            enable_ii_mask: 0b01,
            fx_a: FxDecoded::default(),
            fx_b: FxDecoded::default(),
            raw_payload: Vec::new(),
        }
    }
}

impl PartialEq for PianoPresetDecoded {
    fn eq(&self, other: &Self) -> bool {
        self.layer_a == other.layer_a
            && self.layer_b == other.layer_b
            && self.enable_ii_mask == other.enable_ii_mask
            && self.fx_a == other.fx_a
            && self.fx_b == other.fx_b
    }
}

fn unpack_piano_layer_block(
    r: &mut BitReader,
    base: usize,
    enabled: bool,
    level: f32,
    level_morph: MorphDeltas,
) -> Result<PianoLayerDecoded, CbinError> {
    r.set_bit_pos(base + 12)?;
    let octave = decode_signed(r.read_bits(4)?, 4).clamp(-2, 2) as i8;
    r.set_bit_pos(base + 18)?;
    let category = match r.read_bits(3)? {
        0 => PianoCategory::Grand,
        1 => PianoCategory::Upright,
        2 => PianoCategory::Electric,
        3 => PianoCategory::Clav,
        4 => PianoCategory::Digital,
        _ => PianoCategory::Misc,
    };
    let model_id = r.read_bits(5)? as u8;
    r.set_bit_pos(base + 28)?;
    let name_hash = r.read_bits(32)?;
    let variation = r.read_bits(2)? as u8;
    let acoustic_level = r.read_bits(2)? as u8;
    let string_res = (acoustic_level & 0x02) != 0;
    let soft_release = (acoustic_level & 0x01) != 0;
    let pedal_noise = r.read_bool()?;
    let unison = r.read_bits(2)? as u8;
    let dyn_comp = r.read_bits(2)? as u8;
    let _reserved_pad = r.read_bool()?;
    let timbre = match r.read_bits(3)? {
        0 => PianoTimbre::Off,
        1 => PianoTimbre::Soft,
        2 => PianoTimbre::Mid,
        3 | 6 | 7 => PianoTimbre::Bright, // 6/7 = Clav Treble+Brill / Soft+Treble+Brill
        4 => PianoTimbre::Dyno1,
        _ => PianoTimbre::Dyno2,
    };
    let touch = match r.read_bits(2)? {
        1 => VelocityTouch::Light,
        2 => VelocityTouch::Heavy,
        _ => VelocityTouch::Medium, // Factory v1.66 leaves bits 73..75 = 0 (`MED` default LED)
    };

    Ok(PianoLayerDecoded {
        enabled,
        level,
        level_morph,
        octave,
        category,
        model_id,
        variation,
        name_hash,
        timbre,
        touch,
        string_res,
        acoustic_level,
        pedal_noise,
        soft_release,
        dyn_comp,
        unison,
    })
}

fn pack_piano_layer_block(
    w: &mut BitWriter,
    base: usize,
    layer: &PianoLayerDecoded,
) -> Result<(), CbinError> {
    w.set_bit_pos(base)?;
    w.write_bits(0x0B, 8)?;
    w.set_bit_pos(base + 12)?;
    w.write_bits(encode_signed(i32::from(layer.octave.clamp(-2, 2)), 4), 4)?;
    w.set_bit_pos(base + 18)?;
    let cat_idx = match layer.category {
        PianoCategory::Grand => 0,
        PianoCategory::Upright => 1,
        PianoCategory::Electric => 2,
        PianoCategory::Clav => 3,
        PianoCategory::Digital => 4,
        PianoCategory::Misc => 5,
    };
    w.write_bits(cat_idx, 3)?;
    w.write_bits(u32::from(layer.model_id.min(31)), 5)?;
    w.set_bit_pos(base + 28)?;
    w.write_bits(layer.name_hash, 32)?;
    w.write_bits(u32::from(layer.variation & 0x03), 2)?;
    let acoustic = (u32::from(layer.string_res) << 1) | u32::from(layer.soft_release);
    w.write_bits(acoustic, 2)?;
    w.write_bool(layer.pedal_noise)?;
    w.write_bits(u32::from(layer.unison.min(3)), 2)?;
    w.write_bits(u32::from(layer.dyn_comp.min(3)), 2)?;
    w.write_bool(false)?;
    let timbre_idx = match layer.timbre {
        PianoTimbre::Off => 0,
        PianoTimbre::Soft => 1,
        PianoTimbre::Mid => 2,
        PianoTimbre::Bright => 3,
        PianoTimbre::Dyno1 => 4,
        PianoTimbre::Dyno2 => 5,
    };
    w.write_bits(timbre_idx, 3)?;
    let touch_idx = match layer.touch {
        VelocityTouch::Medium => 0,
        VelocityTouch::Light => 1,
        VelocityTouch::Heavy => 2,
    };
    w.write_bits(touch_idx, 2)?;
    Ok(())
}

impl PianoPresetDecoded {
    pub const BLOCK_TAG: u8 = 0x0C;
    pub const PAYLOAD_LEN: usize = 147;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(&slice[..Self::PAYLOAD_LEN]);
        r.set_bit_pos(9)?;
        let enable_i = r.read_bits(2)? as u8;
        r.set_bit_pos(12)?;
        let enable_ii_mask = r.read_bits(2)? as u8;

        let (lvl_a_raw, lvl_a_morph) = read_morph(&mut r, 14, 7, 127)?;
        let (lvl_b_raw, lvl_b_morph) = read_morph(&mut r, 45, 7, 127)?;

        let layer_a = unpack_piano_layer_block(
            &mut r,
            104,
            (enable_i & 0x01) != 0,
            lvl_a_raw as f32 / 127.0,
            lvl_a_morph,
        )?;
        let layer_b = unpack_piano_layer_block(
            &mut r,
            200,
            (enable_i & 0x02) != 0,
            lvl_b_raw as f32 / 127.0,
            lvl_b_morph,
        )?;
        let fx_a = FxDecoded::unpack(&slice[37..92])?;
        let fx_b = FxDecoded::unpack(&slice[92..147])?;

        Ok(Self {
            layer_a,
            layer_b,
            enable_ii_mask,
            fx_a,
            fx_b,
            raw_payload: slice[..Self::PAYLOAD_LEN].to_vec(),
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = prepare_buffer(&self.raw_payload, Self::PAYLOAD_LEN, Self::BLOCK_TAG);
        let mut w = BitWriter::new(&mut out);

        let enable_i = (u32::from(self.layer_b.enabled) << 1) | u32::from(self.layer_a.enabled);
        w.set_bit_pos(9)?;
        w.write_bits(enable_i, 2)?;
        w.set_bit_pos(12)?;
        w.write_bits(u32::from(self.enable_ii_mask & 0x03), 2)?;

        write_morph(
            &mut w,
            14,
            (self.layer_a.level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_a.level_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            45,
            (self.layer_b.level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_b.level_morph,
            7,
            127,
        )?;

        pack_piano_layer_block(&mut w, 104, &self.layer_a)?;
        pack_piano_layer_block(&mut w, 200, &self.layer_b)?;

        out[37..92].copy_from_slice(&self.fx_a.pack()?);
        out[92..147].copy_from_slice(&self.fx_b.pack()?);
        Ok(out)
    }
}

/// Decoded single Synth layer configuration (`0x32` Layer + `0x2F` Sound).
///
/// Filter envelope ADR offset `0x2F +318..+339`: EMPIRICAL: located by
/// `python tools/ns4_bitscan.py ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" --hist 318:7:3`
/// over 1152 Sound blocks (see `docs/research/synth-0x2f-filter-env-adr.md`); no firmware
/// byte-for-byte check. Curves reuse the amp ADR tables: ASSUMED, confirm via the filter-env
/// time on hardware. The other
/// trailing "engine-side" fields (velocity-to-filter, LFO shape/destination, unison,
/// mono/legato/glide) have no verified bit offsets yet: they are not read from or
/// written to the bitstream (unpack yields defaults, the raw payload preserves the
/// original bits) but drive the DSP via the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthLayerDecoded {
    pub enabled: bool,
    pub level: f32,
    pub level_morph: MorphDeltas,
    pub pan: f32,
    pub octave: i8,
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    pub wave_id: u8,
    pub osc_bank: u8,
    pub osc_cat: u8,
    pub osc_wave_raw: u16,
    pub sample_id: u32,
    pub detune: f32,
    pub osc_ctrl_morph: MorphDeltas,
    pub filter_model: FilterModel,
    pub filter_cutoff_hz: f32,
    pub filter_cutoff_morph: MorphDeltas,
    pub filter_resonance: f32,
    pub filter_resonance_morph: MorphDeltas,
    pub filter_drive: f32,
    pub filter_env_amount: f32,
    pub kb_track: u8,
    pub env_attack_ms: f32,
    pub env_decay_ms: f32,
    pub env_release_ms: f32,
    pub lfo_rate_hz: f32,
    pub lfo_amount: f32,
    pub arp_enabled: bool,
    pub arp_bpm: f32,
    /// `false` = filter envelope mirrors the amp envelope decay.
    pub filter_env_dedicated: bool,
    pub filter_env_attack_ms: f32,
    pub filter_env_decay_ms: f32,
    pub filter_env_release_ms: f32,
    /// Velocity -> filter envelope depth, `0..1`.
    pub filter_env_vel_amount: f32,
    pub lfo_shape: crate::synth::LfoShape,
    pub lfo_dest: crate::synth::LfoDestination,
    /// `0` = off, `1`/`2`/`3` = stereo detune width.
    pub unison: u8,
    pub mono: bool,
    pub legato: bool,
    pub glide_ms: f32,
}

impl Default for SynthLayerDecoded {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 0.8,
            level_morph: MorphDeltas::default(),
            pan: 0.5,
            octave: 0,
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::Saw,
            wave_id: 2,
            osc_bank: 0,
            osc_cat: 0,
            osc_wave_raw: 2,
            sample_id: 0,
            detune: 0.0,
            osc_ctrl_morph: MorphDeltas::default(),
            filter_model: FilterModel::Ladder24,
            filter_cutoff_hz: 4000.0,
            filter_cutoff_morph: MorphDeltas::default(),
            filter_resonance: 0.2,
            filter_resonance_morph: MorphDeltas::default(),
            filter_drive: 0.0,
            filter_env_amount: 0.0,
            kb_track: 2,
            env_attack_ms: 10.0,
            env_decay_ms: 300.0,
            env_release_ms: 250.0,
            lfo_rate_hz: 2.5,
            lfo_amount: 0.0,
            arp_enabled: false,
            arp_bpm: 120.0,
            filter_env_dedicated: false,
            filter_env_attack_ms: 10.0,
            filter_env_decay_ms: 300.0,
            filter_env_release_ms: 250.0,
            filter_env_vel_amount: 0.0,
            lfo_shape: crate::synth::LfoShape::Triangle,
            lfo_dest: crate::synth::LfoDestination::FilterCutoff,
            unison: 0,
            mono: false,
            legato: false,
            glide_ms: 0.0,
        }
    }
}

/// Decoded Synth section configuration across 3 layers (A, B, C) and 3 FX chains (`0x1E`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthPresetDecoded {
    pub layer_a: SynthLayerDecoded,
    pub layer_b: SynthLayerDecoded,
    pub layer_c: SynthLayerDecoded,
    pub enable_ii_mask: u8,
    pub fx_a: FxDecoded,
    pub fx_b: FxDecoded,
    pub fx_c: FxDecoded,
    #[serde(skip)]
    raw_payload: Vec<u8>,
}

impl Default for SynthPresetDecoded {
    fn default() -> Self {
        Self {
            layer_a: SynthLayerDecoded::default(),
            layer_b: SynthLayerDecoded {
                enabled: false,
                ..SynthLayerDecoded::default()
            },
            layer_c: SynthLayerDecoded {
                enabled: false,
                ..SynthLayerDecoded::default()
            },
            enable_ii_mask: 0b001,
            fx_a: FxDecoded::default(),
            fx_b: FxDecoded::default(),
            fx_c: FxDecoded::default(),
            raw_payload: Vec::new(),
        }
    }
}

impl PartialEq for SynthPresetDecoded {
    fn eq(&self, other: &Self) -> bool {
        self.layer_a == other.layer_a
            && self.layer_b == other.layer_b
            && self.layer_c == other.layer_c
            && self.enable_ii_mask == other.enable_ii_mask
            && self.fx_a == other.fx_a
            && self.fx_b == other.fx_b
            && self.fx_c == other.fx_c
    }
}

fn map_osc_mode_and_wave(
    is_osc_mode: bool,
    osc_bank: u8,
    osc_cat: u8,
    osc_wave: u16,
) -> (SynthOscMode, VaWaveform) {
    if !is_osc_mode {
        return (SynthOscMode::Sample, VaWaveform::Saw);
    }
    if osc_cat == 8 {
        return (SynthOscMode::Fm, VaWaveform::Saw);
    }
    if osc_bank == 1 {
        return (SynthOscMode::Wavetable, VaWaveform::Saw);
    }
    let hi3 = (osc_wave >> 7) & 0x07;
    let lo7 = osc_wave & 0x7F;
    let waveform = if hi3 == 0 && lo7 == 0 {
        VaWaveform::Sine
    } else if hi3 == 0 && lo7 == 1 {
        VaWaveform::Triangle
    } else if (hi3 == 5 || hi3 == 6) && lo7 <= 4 {
        VaWaveform::SuperSaw
    } else if (hi3 == 3 && lo7 >= 2) || lo7 == 3 {
        VaWaveform::Pulse
    } else {
        VaWaveform::Saw
    };
    (SynthOscMode::Analog, waveform)
}

fn unpack_synth_layer_pair(
    r: &mut BitReader,
    layer_base: usize,
    sound_base: usize,
    enabled: bool,
    level: f32,
    level_morph: MorphDeltas,
    pan: f32,
) -> Result<SynthLayerDecoded, CbinError> {
    // 0x32 Layer block
    r.set_bit_pos(layer_base + 8)?;
    let is_osc_mode = r.read_bool()?;
    r.set_bit_pos(layer_base + 26)?;
    let sample_id = r.read_bits(32)?;
    r.set_bit_pos(layer_base + 135)?;
    let arp_enabled = r.read_bool()?;
    let (arp_rate_raw, _) = read_morph(r, layer_base + 175, 7, 127)?;
    let arp_bpm = 30.0 + (arp_rate_raw as f32 / 127.0) * 270.0;

    // 0x2F Sound block
    r.set_bit_pos(sound_base + 8)?;
    let osc_bank = r.read_bits(3)? as u8;
    let osc_cat = r.read_bits(4)? as u8;
    let osc_wave_raw = r.read_bits(10)? as u16;
    let (osc_ctrl_raw, osc_ctrl_morph) = read_morph(r, sound_base + 25, 7, 127)?;
    let (lfo_rate_raw, _) = read_morph(r, sound_base + 133, 7, 127)?;
    let (lfo_amt_raw, _) = read_morph(r, sound_base + 164, 7, 127)?;

    r.set_bit_pos(sound_base + 195)?;
    let env_attack_ms = ENV_ATTACK_MS_TABLE[(r.read_bits(7)? as usize).min(127)];
    let dcy_raw = r.read_bits(7)? as usize;
    let env_decay_ms = ENV_DECAY_MS_TABLE[dcy_raw.min(127)];
    let env_release_ms = ENV_RELEASE_MS_TABLE[(r.read_bits(7)? as usize).min(127)];

    r.set_bit_pos(sound_base + 218)?;
    let filter_model = match r.read_bits(3)? {
        0 | 3 => FilterModel::Lp12,
        1 => FilterModel::Lp24,
        2 => FilterModel::Ladder24,
        4 => FilterModel::BandPass,
        _ => FilterModel::HighPass,
    };
    let (flt_freq_raw, filter_cutoff_morph) = read_morph(r, sound_base + 221, 7, 127)?;
    let (flt_res_raw, filter_resonance_morph) = read_morph(r, sound_base + 252, 7, 127)?;
    r.set_bit_pos(sound_base + 283)?;
    let kb_track = r.read_bits(2)? as u8;
    let filter_drive = r.read_bits(2)? as f32 / 3.0;
    let (flt_env_raw, _) = read_morph(r, sound_base + 287, 7, 127)?;
    // Filter envelope ADR (+318..+339, 3x7b): same layout as the amp ADR at +195.
    r.set_bit_pos(sound_base + 318)?;
    let fenv_atk = r.read_bits(7)? as usize;
    let fenv_dcy = r.read_bits(7)? as usize;
    let fenv_rel = r.read_bits(7)? as usize;

    let (osc_mode, waveform) = map_osc_mode_and_wave(is_osc_mode, osc_bank, osc_cat, osc_wave_raw);

    Ok(SynthLayerDecoded {
        enabled,
        level,
        level_morph,
        pan,
        octave: 0,
        osc_mode,
        waveform,
        wave_id: (osc_wave_raw & 0xFF) as u8,
        osc_bank,
        osc_cat,
        osc_wave_raw,
        sample_id,
        detune: (osc_ctrl_raw as f32 / 127.0) * 10.0,
        osc_ctrl_morph,
        filter_model,
        filter_cutoff_hz: FILTER_CUTOFF_HZ_TABLE[(flt_freq_raw as usize).min(127)],
        filter_cutoff_morph,
        filter_resonance: (flt_res_raw as f32 / 127.0) * 0.95,
        filter_resonance_morph,
        filter_drive,
        filter_env_amount: (flt_env_raw as f32 - 64.0) / 64.0,
        kb_track,
        env_attack_ms,
        env_decay_ms,
        env_release_ms,
        lfo_rate_hz: LFO_RATE_HZ_TABLE[(lfo_rate_raw as usize).min(127)],
        lfo_amount: lfo_amt_raw as f32 / 127.0,
        arp_enabled,
        arp_bpm,
        filter_env_dedicated: true,
        filter_env_attack_ms: ENV_ATTACK_MS_TABLE[fenv_atk.min(127)],
        filter_env_decay_ms: ENV_DECAY_MS_TABLE[fenv_dcy.min(127)],
        filter_env_release_ms: ENV_RELEASE_MS_TABLE[fenv_rel.min(127)],
        ..SynthLayerDecoded::default()
    })
}

fn encode_osc_fields(layer: &SynthLayerDecoded) -> (bool, u32, u32, u32) {
    match layer.osc_mode {
        SynthOscMode::Sample => (false, 0, 0, u32::from(layer.osc_wave_raw) & 0x03FF),
        SynthOscMode::Fm => (
            true,
            u32::from(layer.osc_bank.min(1)),
            8,
            u32::from(layer.wave_id),
        ),
        SynthOscMode::Wavetable => (true, 1, 0, u32::from(layer.wave_id)),
        SynthOscMode::Analog => {
            let wave = match layer.waveform {
                VaWaveform::Sine => 0,
                VaWaveform::Triangle => 1,
                VaWaveform::Saw => 2,
                VaWaveform::Pulse => (3 << 7) | 2,
                VaWaveform::SuperSaw => 6 << 7,
            };
            (true, 0, 0, wave)
        }
    }
}

fn pack_synth_layer_pair(
    w: &mut BitWriter,
    layer_base: usize,
    sound_base: usize,
    layer: &SynthLayerDecoded,
) -> Result<(), CbinError> {
    let (is_osc_mode, osc_bank, osc_cat, osc_wave) = encode_osc_fields(layer);

    // 0x32 Layer block
    w.set_bit_pos(layer_base)?;
    w.write_bits(0x32, 8)?;
    w.write_bool(is_osc_mode)?;
    w.set_bit_pos(layer_base + 26)?;
    w.write_bits(layer.sample_id, 32)?;
    w.set_bit_pos(layer_base + 135)?;
    w.write_bool(layer.arp_enabled)?;
    let arp_rate = (((layer.arp_bpm - 30.0) / 270.0).clamp(0.0, 1.0) * 127.0).round() as u32;
    write_morph(
        w,
        layer_base + 175,
        arp_rate,
        MorphDeltas::default(),
        7,
        127,
    )?;

    // 0x2F Sound block
    w.set_bit_pos(sound_base)?;
    w.write_bits(0x2F, 8)?;
    w.write_bits(osc_bank, 3)?;
    w.write_bits(osc_cat, 4)?;
    w.write_bits(osc_wave, 10)?;
    write_morph(
        w,
        sound_base + 25,
        ((layer.detune / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
        layer.osc_ctrl_morph,
        7,
        127,
    )?;
    let lfo_rate = table_val_to_idx(&LFO_RATE_HZ_TABLE, layer.lfo_rate_hz);
    write_morph(
        w,
        sound_base + 133,
        lfo_rate,
        MorphDeltas::default(),
        7,
        127,
    )?;
    let lfo_amt = (layer.lfo_amount.clamp(0.0, 1.0) * 127.0).round() as u32;
    write_morph(w, sound_base + 164, lfo_amt, MorphDeltas::default(), 7, 127)?;

    w.set_bit_pos(sound_base + 195)?;
    w.write_bits(
        table_val_to_idx(&ENV_ATTACK_MS_TABLE, layer.env_attack_ms),
        7,
    )?;
    let dcy_bits = if layer.env_decay_ms >= 44_000.0 || !layer.env_decay_ms.is_finite() {
        127
    } else {
        table_val_to_idx(&ENV_DECAY_MS_TABLE, layer.env_decay_ms).min(126)
    };
    w.write_bits(dcy_bits, 7)?;
    w.write_bits(
        table_val_to_idx(&ENV_RELEASE_MS_TABLE, layer.env_release_ms),
        7,
    )?;

    w.set_bit_pos(sound_base + 218)?;
    let flt_idx = match layer.filter_model {
        FilterModel::Lp12 => 0,
        FilterModel::Lp24 => 1,
        FilterModel::Ladder24 => 2,
        FilterModel::BandPass => 4,
        FilterModel::HighPass => 5,
    };
    w.write_bits(flt_idx, 3)?;
    write_morph(
        w,
        sound_base + 221,
        table_val_to_idx(&FILTER_CUTOFF_HZ_TABLE, layer.filter_cutoff_hz),
        layer.filter_cutoff_morph,
        7,
        127,
    )?;
    write_morph(
        w,
        sound_base + 252,
        ((layer.filter_resonance / 0.95).clamp(0.0, 1.0) * 127.0).round() as u32,
        layer.filter_resonance_morph,
        7,
        127,
    )?;
    w.set_bit_pos(sound_base + 283)?;
    w.write_bits(u32::from(layer.kb_track.min(3)), 2)?;
    w.write_bits((layer.filter_drive.clamp(0.0, 1.0) * 3.0).round() as u32, 2)?;
    let env_amt_raw = ((64.0 + layer.filter_env_amount.clamp(-1.0, 1.0) * 63.0).round() as i32)
        .clamp(0, 127) as u32;
    write_morph(
        w,
        sound_base + 287,
        env_amt_raw,
        MorphDeltas::default(),
        7,
        127,
    )?;

    w.set_bit_pos(sound_base + 318)?;
    w.write_bits(
        table_val_to_idx(&ENV_ATTACK_MS_TABLE, layer.filter_env_attack_ms),
        7,
    )?;
    let fdcy_bits =
        if layer.filter_env_decay_ms >= 44_000.0 || !layer.filter_env_decay_ms.is_finite() {
            127
        } else {
            table_val_to_idx(&ENV_DECAY_MS_TABLE, layer.filter_env_decay_ms).min(126)
        };
    w.write_bits(fdcy_bits, 7)?;
    w.write_bits(
        table_val_to_idx(&ENV_RELEASE_MS_TABLE, layer.filter_env_release_ms),
        7,
    )?;

    Ok(())
}

impl SynthPresetDecoded {
    pub const BLOCK_TAG: u8 = 0x1E;
    pub const PAYLOAD_LEN: usize = 493;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(&slice[..Self::PAYLOAD_LEN]);
        r.set_bit_pos(10)?;
        let enable_i = r.read_bits(3)? as u8;
        r.set_bit_pos(15)?;
        let enable_ii_mask = r.read_bits(3)? as u8;

        let (lvl_a, lvl_a_morph) = read_morph(&mut r, 18, 7, 127)?;
        let (lvl_b, lvl_b_morph) = read_morph(&mut r, 49, 7, 127)?;
        let (lvl_c, lvl_c_morph) = read_morph(&mut r, 80, 7, 127)?;
        r.set_bit_pos(111)?;
        let pan_a = r.read_bits(7)? as f32 / 127.0;
        r.set_bit_pos(142)?;
        let pan_b = r.read_bits(7)? as f32 / 127.0;
        r.set_bit_pos(173)?;
        let pan_c = r.read_bits(7)? as f32 / 127.0;

        let layer_a = unpack_synth_layer_pair(
            &mut r,
            248,
            1472,
            (enable_i & 0x01) != 0,
            lvl_a as f32 / 127.0,
            lvl_a_morph,
            pan_a,
        )?;
        let layer_b = unpack_synth_layer_pair(
            &mut r,
            656,
            1856,
            (enable_i & 0x02) != 0,
            lvl_b as f32 / 127.0,
            lvl_b_morph,
            pan_b,
        )?;
        let layer_c = unpack_synth_layer_pair(
            &mut r,
            1064,
            2240,
            (enable_i & 0x04) != 0,
            lvl_c as f32 / 127.0,
            lvl_c_morph,
            pan_c,
        )?;

        let fx_a = FxDecoded::unpack(&slice[328..383])?;
        let fx_b = FxDecoded::unpack(&slice[383..438])?;
        let fx_c = FxDecoded::unpack(&slice[438..493])?;

        Ok(Self {
            layer_a,
            layer_b,
            layer_c,
            enable_ii_mask,
            fx_a,
            fx_b,
            fx_c,
            raw_payload: slice[..Self::PAYLOAD_LEN].to_vec(),
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = prepare_buffer(&self.raw_payload, Self::PAYLOAD_LEN, Self::BLOCK_TAG);
        let mut w = BitWriter::new(&mut out);

        let enable_i = (u32::from(self.layer_c.enabled) << 2)
            | (u32::from(self.layer_b.enabled) << 1)
            | u32::from(self.layer_a.enabled);
        w.set_bit_pos(10)?;
        w.write_bits(enable_i, 3)?;
        w.set_bit_pos(15)?;
        w.write_bits(u32::from(self.enable_ii_mask & 0x07), 3)?;

        write_morph(
            &mut w,
            18,
            (self.layer_a.level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_a.level_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            49,
            (self.layer_b.level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_b.level_morph,
            7,
            127,
        )?;
        write_morph(
            &mut w,
            80,
            (self.layer_c.level.clamp(0.0, 1.0) * 127.0).round() as u32,
            self.layer_c.level_morph,
            7,
            127,
        )?;

        w.set_bit_pos(111)?;
        w.write_bits((self.layer_a.pan.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
        w.set_bit_pos(142)?;
        w.write_bits((self.layer_b.pan.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
        w.set_bit_pos(173)?;
        w.write_bits((self.layer_c.pan.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;

        pack_synth_layer_pair(&mut w, 248, 1472, &self.layer_a)?;
        pack_synth_layer_pair(&mut w, 656, 1856, &self.layer_b)?;
        pack_synth_layer_pair(&mut w, 1064, 2240, &self.layer_c)?;

        out[328..383].copy_from_slice(&self.fx_a.pack()?);
        out[383..438].copy_from_slice(&self.fx_b.pack()?);
        out[438..493].copy_from_slice(&self.fx_c.pack()?);
        Ok(out)
    }
}

/// Decoded global program parameters (`0x2C`, 45 bytes: Split zones, Section/Scene enables, Master Clock).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalProgramDecoded {
    pub tempo_bpm: f32,
    pub master_compressor: f32,
    pub transpose: i8,
    pub split_enabled: bool,
    pub split_zone_enabled: [bool; 3],
    pub split_points: [u8; 3],
    pub split_crossfades: [CrossfadeWidth; 3],
    pub section_enable_i: u8,
    pub section_enable_ii: u8,
    pub scene_1_mask: u8,
    pub scene_2_mask: u8,
    #[serde(skip)]
    raw_payload: Vec<u8>,
}

impl Default for GlobalProgramDecoded {
    fn default() -> Self {
        Self {
            tempo_bpm: 120.0,
            master_compressor: 0.0,
            transpose: 0,
            split_enabled: false,
            split_zone_enabled: [false, false, false],
            split_points: [48, 65, 84],
            split_crossfades: [
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
            ],
            section_enable_i: 0b111,
            section_enable_ii: 0b111,
            scene_1_mask: 0b0001_0101,
            scene_2_mask: 0b0111_1111,
            raw_payload: Vec::new(),
        }
    }
}

impl PartialEq for GlobalProgramDecoded {
    fn eq(&self, other: &Self) -> bool {
        self.tempo_bpm == other.tempo_bpm
            && self.master_compressor == other.master_compressor
            && self.transpose == other.transpose
            && self.split_enabled == other.split_enabled
            && self.split_zone_enabled == other.split_zone_enabled
            && self.split_points == other.split_points
            && self.split_crossfades == other.split_crossfades
            && self.section_enable_i == other.section_enable_i
            && self.section_enable_ii == other.section_enable_ii
            && self.scene_1_mask == other.scene_1_mask
            && self.scene_2_mask == other.scene_2_mask
    }
}

impl GlobalProgramDecoded {
    pub const BLOCK_TAG: u8 = 0x2C;
    pub const PAYLOAD_LEN: usize = 45;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(&slice[..Self::PAYLOAD_LEN]);
        r.set_bit_pos(8)?;

        let split_enabled = r.read_bool()?;
        let split_zone_enabled = [r.read_bool()?, r.read_bool()?, r.read_bool()?];
        let split_points = [
            split_idx_to_midi(r.read_bits(4)?),
            split_idx_to_midi(r.read_bits(4)?),
            split_idx_to_midi(r.read_bits(4)?),
        ];
        let mut split_crossfades = [CrossfadeWidth::Off; 3];
        for cf in &mut split_crossfades {
            *cf = match r.read_bits(2)? {
                0 => CrossfadeWidth::Off,
                1 => CrossfadeWidth::Small,
                _ => CrossfadeWidth::Large,
            };
        }

        r.set_bit_pos(35)?;
        let tempo_bpm = 30.0 + r.read_bits(8)? as f32;

        r.set_bit_pos(284)?;
        let section_enable_i = r.read_bits(3)? as u8;
        let section_enable_ii = r.read_bits(3)? as u8;

        let scene_1_mask = u8::from((section_enable_i & 0x04) != 0)
            | ((u8::from((section_enable_i & 0x02) != 0)) << 2)
            | ((u8::from((section_enable_i & 0x01) != 0)) << 4);
        let scene_2_mask = u8::from((section_enable_ii & 0x04) != 0)
            | ((u8::from((section_enable_ii & 0x02) != 0)) << 2)
            | ((u8::from((section_enable_ii & 0x01) != 0)) << 4);

        Ok(Self {
            tempo_bpm,
            master_compressor: 0.0,
            transpose: 0,
            split_enabled,
            split_zone_enabled,
            split_points,
            split_crossfades,
            section_enable_i,
            section_enable_ii,
            scene_1_mask,
            scene_2_mask,
            raw_payload: slice[..Self::PAYLOAD_LEN].to_vec(),
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = prepare_buffer(&self.raw_payload, Self::PAYLOAD_LEN, Self::BLOCK_TAG);
        let mut w = BitWriter::new(&mut out);

        w.set_bit_pos(8)?;
        w.write_bool(self.split_enabled)?;
        for en in self.split_zone_enabled {
            w.write_bool(en)?;
        }
        for note in self.split_points {
            w.write_bits(midi_to_split_idx(note), 4)?;
        }
        for cf in self.split_crossfades {
            let cf_idx = match cf {
                CrossfadeWidth::Off => 0,
                CrossfadeWidth::Small => 1,
                CrossfadeWidth::Large => 2,
            };
            w.write_bits(cf_idx, 2)?;
        }

        w.set_bit_pos(35)?;
        let rate = (self.tempo_bpm - 30.0).round().clamp(0.0, 255.0) as u32;
        w.write_bits(rate, 8)?;

        w.set_bit_pos(284)?;
        w.write_bits(u32::from(self.section_enable_i & 0x07), 3)?;
        w.write_bits(u32::from(self.section_enable_ii & 0x07), 3)?;

        Ok(out)
    }
}

/// Fully decoded, high-level representation of a Nord Stage 4 Program (`.ns4p` / `.ns4l`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramDecoded {
    pub name: String,
    pub bank_index: u16,
    pub location_index: u16,
    pub category_id: u32,
    pub global: GlobalProgramDecoded,
    pub organ: OrganPresetDecoded,
    pub piano: PianoPresetDecoded,
    pub synth: SynthPresetDecoded,
}

impl Default for ProgramDecoded {
    fn default() -> Self {
        Self {
            name: "Init Program".to_string(),
            bank_index: 0,
            location_index: 0,
            category_id: 0,
            global: GlobalProgramDecoded::default(),
            organ: OrganPresetDecoded::default(),
            piano: PianoPresetDecoded::default(),
            synth: SynthPresetDecoded::default(),
        }
    }
}

fn combine_scene_mask(sec_mask: u8, org_layers: u8, pno_layers: u8, syn_layers: u8) -> u8 {
    let org = if (sec_mask & 0b100) != 0 {
        org_layers & 0b11
    } else {
        0
    };
    let pno = if (sec_mask & 0b010) != 0 {
        pno_layers & 0b11
    } else {
        0
    };
    let syn = if (sec_mask & 0b001) != 0 {
        syn_layers & 0b111
    } else {
        0
    };
    org | (pno << 2) | (syn << 4)
}

impl ProgramDecoded {
    /// Decodes a Program from a parsed `CbinFile`.
    pub fn from_cbin_file(cbin: &CbinFile) -> Result<Self, CbinError> {
        if !matches!(cbin.header.kind, CbinKind::Program | CbinKind::Live) {
            return Err(CbinError::UnknownTag(cbin.header.kind.tag()));
        }
        if cbin.payload.len() < 820 {
            return Err(CbinError::TooShort {
                actual: cbin.payload.len(),
                min: 820,
            });
        }

        let mut global = GlobalProgramDecoded::unpack(&cbin.payload[0..45])?;
        let organ = OrganPresetDecoded::unpack(&cbin.payload[45..180])?;
        let piano = PianoPresetDecoded::unpack(&cbin.payload[180..327])?;
        let synth = SynthPresetDecoded::unpack(&cbin.payload[327..820])?;

        let org_i = (u8::from(organ.layer_b_enabled) << 1) | u8::from(organ.layer_a_enabled);
        let pno_i = (u8::from(piano.layer_b.enabled) << 1) | u8::from(piano.layer_a.enabled);
        let syn_i = (u8::from(synth.layer_c.enabled) << 2)
            | (u8::from(synth.layer_b.enabled) << 1)
            | u8::from(synth.layer_a.enabled);
        global.scene_1_mask = combine_scene_mask(global.section_enable_i, org_i, pno_i, syn_i);
        global.scene_2_mask = combine_scene_mask(
            global.section_enable_ii,
            organ.enable_ii_mask,
            piano.enable_ii_mask,
            synth.enable_ii_mask,
        );

        let bank_letter = (b'A' + (cbin.header.bank_index as u8).min(7)) as char;
        let prog_num = (cbin.header.location_index % 48) + 1;
        let page = (prog_num - 1) / 8 + 1;
        let btn = (prog_num - 1) % 8 + 1;
        let name = format!("{bank_letter}:{page}{btn} Program");

        Ok(Self {
            name,
            bank_index: cbin.header.bank_index,
            location_index: cbin.header.location_index,
            category_id: cbin.header.category_id,
            global,
            organ,
            piano,
            synth,
        })
    }

    /// Encodes this program back into a valid 868-byte `CbinFile`.
    pub fn to_cbin_file(&self, bank: u16, loc: u16) -> Result<CbinFile, CbinError> {
        let mut payload = Vec::with_capacity(820);
        payload.extend_from_slice(&self.global.pack()?);
        payload.extend_from_slice(&self.organ.pack()?);
        payload.extend_from_slice(&self.piano.pack()?);
        payload.extend_from_slice(&self.synth.pack()?);

        let checksum = payload
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(u32::from(b)));

        Ok(CbinFile {
            header: CbinHeader {
                format_version: 1,
                kind: CbinKind::Program,
                bank_index: bank,
                location_index: loc,
                category_id: self.category_id,
                schema_id: 0x0139,
                checksum,
                reserved: [0u8; 18],
            },
            payload,
        })
    }

    /// Serializes this program to human-readable pretty JSON.
    pub fn to_json(&self) -> Result<String, CbinError> {
        serde_json::to_string_pretty(self).map_err(|e| CbinError::JsonError(e.to_string()))
    }

    /// Deserializes a program from JSON.
    pub fn from_json(json_str: &str) -> Result<Self, CbinError> {
        serde_json::from_str(json_str).map_err(|e| CbinError::JsonError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fx_bitstream_pack_and_unpack_with_morph() {
        let original = FxDecoded {
            mod1_enabled: true,
            mod1_clock_sync: true,
            mod1_type_raw: 7, // A-Wah Envelope
            mod1_rate: 0.35,
            mod1_amount: 0.72,
            mod2_enabled: true,
            mod2_type_raw: 2, // Flanger Stereo
            mod2_rate: 0.25,
            mod2_amount: 0.55,
            amp_enabled: true,
            eq_treble_db: 4.5,
            eq_mid_db: -3.0,
            eq_bass_db: 2.25,
            amp_drive: 6.0,
            comp_enabled: true,
            comp_amount: 0.4,
            comp_fast: true,
            delay_enabled: true,
            delay_rate_norm: 0.5,
            delay_mix: 0.35,
            delay_ping_pong: true,
            delay_feedback: 0.45,
            reverb_enabled: true,
            reverb_type_raw: 4, // Hall
            reverb_color: 2,
            reverb_amount: 0.42,
            reverb_amount_morph: MorphDeltas {
                wheel: 0.25,
                aftertouch: -0.1,
                ctrl_pedal: 0.0,
            },
            ..Default::default()
        };

        let packed = original.pack().expect("pack fx");
        assert_eq!(packed.len(), FxDecoded::PAYLOAD_LEN);
        assert_eq!(packed[0], FxDecoded::BLOCK_TAG);

        let unpacked = FxDecoded::unpack(&packed).expect("unpack fx");
        assert_eq!(unpacked.mod1_type(), Mod1Type::Wah);
        assert_eq!(unpacked.mod2_type(), Mod2Type::Flanger);
        assert_eq!(unpacked.reverb_model(), ReverbModel::Hall);
        assert!((unpacked.eq_treble_db - 4.5).abs() < 0.01);
        assert!((unpacked.eq_mid_db + 3.0).abs() < 0.01);
        assert!((unpacked.eq_bass_db - 2.25).abs() < 0.01);
        assert!(unpacked.delay_ping_pong);
        assert!((unpacked.reverb_amount_morph.wheel - 0.25).abs() < 0.02);
        assert!((unpacked.reverb_amount_morph.aftertouch + 0.1).abs() < 0.02);
    }

    #[test]
    fn test_organ_preset_bitstream_pack_and_unpack() {
        let original = OrganPresetDecoded {
            model: OrganModel::Vox,
            percussion_on: true,
            percussion_soft: true,
            percussion_fast: false,
            percussion_third: true,
            vibrato_on: true,
            vibrato_type: 3,
            rotary_enabled: true,
            rotary_speed: RotarySpeed::Fast,
            rotary_drive: 7.5,
            layer_a_octave: -1,
            drawbars_a: [8, 4, 2, 0, 1, 3, 5, 7, 8],
            drawbars_b: [0, 1, 2, 3, 4, 5, 6, 7, 8],
            ..Default::default()
        };

        let packed = original.pack().expect("pack organ");
        assert_eq!(packed.len(), 135);
        assert_eq!(packed[0], OrganPresetDecoded::BLOCK_TAG);

        let unpacked = OrganPresetDecoded::unpack(&packed).expect("unpack organ");
        assert_eq!(unpacked.model, OrganModel::Vox);
        assert_eq!(unpacked.scanner_mode(true), ScannerMode::C2);
        assert!(unpacked.percussion_on);
        assert!(unpacked.percussion_soft);
        assert!(!unpacked.percussion_fast);
        assert!(unpacked.percussion_third);
        assert!(unpacked.vibrato_on);
        assert_eq!(unpacked.vibrato_type, 3);
        assert_eq!(unpacked.rotary_speed, RotarySpeed::Fast);
        assert!((unpacked.rotary_drive - 7.5).abs() < 0.1);
        assert_eq!(unpacked.layer_a_octave, -1);
        assert_eq!(unpacked.drawbars_a, original.drawbars_a);
        assert_eq!(unpacked.drawbars_b, original.drawbars_b);
    }

    #[test]
    fn test_piano_preset_bitstream_pack_and_unpack() {
        let original = PianoPresetDecoded {
            layer_a: PianoLayerDecoded {
                category: PianoCategory::Electric,
                model_id: 14,
                timbre: PianoTimbre::Bright,
                touch: VelocityTouch::Heavy,
                string_res: true,
                pedal_noise: true,
                dyn_comp: 2,
                unison: 3,
                octave: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        let packed = original.pack().expect("pack piano");
        assert_eq!(packed.len(), 147);
        assert_eq!(packed[0], PianoPresetDecoded::BLOCK_TAG);

        let unpacked = PianoPresetDecoded::unpack(&packed).expect("unpack piano");
        assert_eq!(unpacked.layer_a.category, PianoCategory::Electric);
        assert_eq!(unpacked.layer_a.model_id, 14);
        assert_eq!(unpacked.layer_a.timbre, PianoTimbre::Bright);
        assert_eq!(unpacked.layer_a.touch, VelocityTouch::Heavy);
        assert!(unpacked.layer_a.string_res);
        assert!(unpacked.layer_a.pedal_noise);
        assert_eq!(unpacked.layer_a.dyn_comp, 2);
        assert_eq!(unpacked.layer_a.unison, 3);
        assert_eq!(unpacked.layer_a.octave, 1);
    }

    #[test]
    fn test_synth_filter_env_adr_decoded_from_bits_318_and_roundtrips() {
        let mut original = SynthPresetDecoded::default();
        original.layer_a.filter_env_dedicated = true;
        original.layer_a.filter_env_attack_ms = ENV_ATTACK_MS_TABLE[24];
        original.layer_a.filter_env_decay_ms = ENV_DECAY_MS_TABLE[61];
        original.layer_a.filter_env_release_ms = ENV_RELEASE_MS_TABLE[33];
        let packed = original.pack().unwrap();

        // Raw bits at 0x2F(layer A)=1472, +318: 7b attack, 7b decay, 7b release.
        let mut r = BitReader::new(&packed);
        r.set_bit_pos(1472 + 318).unwrap();
        assert_eq!(r.read_bits(7).unwrap(), 24);
        assert_eq!(r.read_bits(7).unwrap(), 61);
        assert_eq!(r.read_bits(7).unwrap(), 33);

        let un = SynthPresetDecoded::unpack(&packed).unwrap();
        assert!(un.layer_a.filter_env_dedicated);
        assert_eq!(un.layer_a.filter_env_attack_ms, ENV_ATTACK_MS_TABLE[24]);
        assert_eq!(un.layer_a.filter_env_decay_ms, ENV_DECAY_MS_TABLE[61]);
        assert_eq!(un.layer_a.filter_env_release_ms, ENV_RELEASE_MS_TABLE[33]);
        assert_eq!(un.pack().unwrap(), packed);
    }

    #[test]
    fn test_synth_filter_env_hold_decay_roundtrips_as_127() {
        let mut p = SynthPresetDecoded::default();
        p.layer_a.filter_env_decay_ms = 44_000.0;
        let packed = p.pack().unwrap();
        let mut r = BitReader::new(&packed);
        r.set_bit_pos(1472 + 325).unwrap();
        assert_eq!(r.read_bits(7).unwrap(), 127);
    }

    #[test]
    fn test_synth_preset_bitstream_pack_and_unpack() {
        let original = SynthPresetDecoded {
            layer_a: SynthLayerDecoded {
                osc_mode: SynthOscMode::Wavetable,
                waveform: VaWaveform::Saw,
                wave_id: 17,
                detune: 3.4,
                filter_model: FilterModel::Lp24,
                filter_cutoff_hz: 1200.0,
                filter_cutoff_morph: MorphDeltas {
                    wheel: 0.3,
                    aftertouch: 0.0,
                    ctrl_pedal: 0.0,
                },
                filter_resonance: 0.65,
                filter_drive: 0.67,
                arp_enabled: true,
                arp_bpm: 135.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let packed = original.pack().expect("pack synth");
        assert_eq!(packed.len(), 493);
        assert_eq!(packed[0], SynthPresetDecoded::BLOCK_TAG);

        let unpacked = SynthPresetDecoded::unpack(&packed).expect("unpack synth");
        assert_eq!(unpacked.layer_a.osc_mode, SynthOscMode::Wavetable);
        assert_eq!(unpacked.layer_a.wave_id, 17);
        assert!((unpacked.layer_a.detune - 3.4).abs() < 0.2);
        assert_eq!(unpacked.layer_a.filter_model, FilterModel::Lp24);
        assert!((unpacked.layer_a.filter_cutoff_hz - 1200.0).abs() < 100.0);
        assert!((unpacked.layer_a.filter_cutoff_morph.wheel - 0.3).abs() < 0.02);
        assert!((unpacked.layer_a.filter_resonance - 0.65).abs() < 0.05);
        assert!((unpacked.layer_a.filter_drive - 0.67).abs() < 0.05);
        assert!(unpacked.layer_a.arp_enabled);
        assert!((unpacked.layer_a.arp_bpm - 135.0).abs() < 3.0);
    }

    #[test]
    fn test_program_decoded_roundtrip_cbin_and_json_and_lossless_preservation() {
        let prog = ProgramDecoded {
            global: GlobalProgramDecoded {
                tempo_bpm: 132.0,
                split_enabled: true,
                split_zone_enabled: [false, true, false],
                split_points: [48, 65, 84],
                split_crossfades: [
                    CrossfadeWidth::Small,
                    CrossfadeWidth::Large,
                    CrossfadeWidth::Off,
                ],
                section_enable_i: 0b101,
                ..Default::default()
            },
            organ: OrganPresetDecoded {
                model: OrganModel::Farfisa,
                ..Default::default()
            },
            piano: PianoPresetDecoded {
                layer_a: PianoLayerDecoded {
                    timbre: PianoTimbre::Dyno1,
                    ..Default::default()
                },
                ..Default::default()
            },
            synth: SynthPresetDecoded {
                layer_a: SynthLayerDecoded {
                    filter_cutoff_hz: 2500.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let json = prog.to_json().expect("to json");
        assert!(json.contains("Init Program"));
        assert!(json.contains("Farfisa"));
        let parsed_from_json = ProgramDecoded::from_json(&json).expect("from json");
        assert_eq!(parsed_from_json.global.tempo_bpm.round(), 132.0);
        assert_eq!(parsed_from_json.organ.model, OrganModel::Farfisa);
        assert_eq!(parsed_from_json.piano.layer_a.timbre, PianoTimbre::Dyno1);

        let cbin_file = prog.to_cbin_file(0, 0).expect("to cbin");
        assert_eq!(cbin_file.header.kind, CbinKind::Program);
        assert_eq!(cbin_file.payload.len(), 820);
        let bytes = cbin_file.to_bytes();
        assert_eq!(bytes.len(), 868);

        let reparsed_cbin = CbinFile::parse(&bytes).expect("parse cbin");
        let decoded = reparsed_cbin.decode_program().expect("decode program");
        assert_eq!(decoded.organ.model, OrganModel::Farfisa);
        assert_eq!(decoded.piano.layer_a.timbre, PianoTimbre::Dyno1);
        assert!(decoded.global.split_enabled);
        assert_eq!(decoded.global.split_zone_enabled, [false, true, false]);
        assert_eq!(decoded.global.scene_1_mask, 0b0001_0001); // Organ A + Synth A
        assert_eq!(decoded.global.split_crossfades[0], CrossfadeWidth::Small);
        assert_eq!(decoded.global.split_crossfades[1], CrossfadeWidth::Large);
        assert_eq!(decoded.global.split_crossfades[2], CrossfadeWidth::Off);

        // Lossless preservation of unedited raw payload bytes (e.g. hardware NameHash / flags)
        let mut synthetic_hw = bytes.clone();
        // Set untouched reserved bits inside Global (bit 250) and Synth NameHash (byte 327+36)
        synthetic_hw[48 + 31] = 0xAA;
        synthetic_hw[48 + 327 + 36] = 0x5A;
        let hw_cbin = CbinFile::parse(&synthetic_hw).expect("parse synthetic hw");
        let hw_prog = hw_cbin.decode_program().expect("decode hw");
        let repacked = hw_prog.to_cbin_file(0, 0).expect("repack hw");
        assert_eq!(repacked.payload[31], 0xAA);
        assert_eq!(repacked.payload[327 + 36], 0x5A);
    }

    #[test]
    fn test_arm0_murmurhash2_name_hash_and_bit_offsets() {
        // Known hashes verified against NS4 v1.66 factory .ns4p & ARM0 @ 0x0a5a44
        assert_eq!(nord_name_hash("Astoria Grand Lrg 6.1"), 0x444e_bb81);
        assert_eq!(nord_name_hash("Ac Guitar Finger_ST Mart 4.1"), 0xa281_b28c);
        assert!(!nord_name_hash_matches("astoria grand lrg 6.1", 0));
        assert!(nord_name_hash_matches("Astoria Grand Lrg 6.1", 0x444e_bb81));
        assert!(nord_name_hash_matches(
            "astoria grand lrg 6.1",
            nord_name_hash("ASTORIA GRAND LRG 6.1")
        ));

        // Verify Piano 0x0B name_hash is aligned to bit (base + 28) and Synth 0x32 sample_id to (layer_base + 26)
        let mut piano_preset = PianoPresetDecoded::default();
        piano_preset.layer_a.name_hash = 0x444e_bb81;
        piano_preset.layer_a.variation = 2;
        let piano_raw = piano_preset.pack().expect("pack piano");
        let piano_decoded = PianoPresetDecoded::unpack(&piano_raw).expect("unpack piano");
        assert_eq!(piano_decoded.layer_a.name_hash, 0x444e_bb81);
        assert_eq!(piano_decoded.layer_a.variation, 2);

        let mut synth_preset = SynthPresetDecoded::default();
        synth_preset.layer_a.osc_mode = SynthOscMode::Sample;
        synth_preset.layer_a.sample_id = 0xa281_b28c;
        synth_preset.layer_a.env_attack_ms = 54.0;
        synth_preset.layer_a.env_decay_ms = 45_000.0;
        synth_preset.layer_a.env_release_ms = 1490.0;
        synth_preset.layer_a.filter_cutoff_hz = 440.0;
        synth_preset.layer_a.lfo_rate_hz = 523.0;
        let synth_raw = synth_preset.pack().expect("pack synth");
        let synth_decoded = SynthPresetDecoded::unpack(&synth_raw).expect("unpack synth");
        assert_eq!(synth_decoded.layer_a.osc_mode, SynthOscMode::Sample);
        assert_eq!(synth_decoded.layer_a.sample_id, 0xa281_b28c);
        assert_eq!(synth_decoded.layer_a.env_attack_ms, 54.0);
        assert_eq!(synth_decoded.layer_a.env_decay_ms, 45_000.0);
        assert_eq!(synth_decoded.layer_a.env_release_ms, 1490.0);
        assert_eq!(synth_decoded.layer_a.filter_cutoff_hz, 440.0);
        assert_eq!(synth_decoded.layer_a.lfo_rate_hz, 523.0);

        // Cutoff norm<->Hz bijectivity at boundaries and 440 Hz anchor
        assert_eq!(cutoff_norm_to_hz(0.0), 14.0);
        assert_eq!(cutoff_norm_to_hz(1.0), 32000.0);
        assert!((cutoff_norm_to_hz(cutoff_hz_to_norm(440.0)) - 440.0).abs() < 1e-2);
        assert!((cutoff_norm_to_hz(cutoff_hz_to_norm(3500.0)) - 3500.0).abs() < 1.0);
    }

    #[test]
    fn test_revk_v166_piano_layer_0x0b_flags_and_timbre() {
        // Replicate exact RevK v1.66 `White Grand.ns4p` (`var=1, ac=2 StringRes only, ped=1, uni=0, dyn=0, timbre=0 Off`)
        let mut wg = [0u8; 147];
        wg[0] = PianoPresetDecoded::BLOCK_TAG;
        wg[13..25].copy_from_slice(&[
            0x0b, 0x60, 0x40, 0x04, 0x81, 0x42, 0x54, 0xf6, 0x80, 0x00, 0x00, 0x00,
        ]);
        let dec_wg = PianoPresetDecoded::unpack(&wg).expect("unpack white grand fixture");
        assert!(dec_wg.layer_a.string_res);
        assert!(!dec_wg.layer_a.soft_release);
        assert!(dec_wg.layer_a.pedal_noise);
        assert_eq!(dec_wg.layer_a.variation, 1);
        assert_eq!(dec_wg.layer_a.unison, 0);
        assert_eq!(dec_wg.layer_a.dyn_comp, 0);
        assert_eq!(dec_wg.layer_a.timbre, PianoTimbre::Off);
        assert_eq!(dec_wg.layer_a.touch, VelocityTouch::Medium);
        assert_eq!(&dec_wg.pack().unwrap()[13..25], &wg[13..25]);

        // Replicate `Horizon Whl.ns4p` (`var=1, ac=2 (string_res=true), ped=0, uni=0, dyn=3, timbre=1 Soft`)
        let mut hw = [0u8; 147];
        hw[0] = PianoPresetDecoded::BLOCK_TAG;
        hw[13..25].copy_from_slice(&[
            0x0b, 0x90, 0x40, 0x04, 0x81, 0x42, 0x54, 0xf6, 0x18, 0x80, 0x00, 0x00,
        ]);
        let dec_hw = PianoPresetDecoded::unpack(&hw).expect("unpack horizon whl fixture");
        assert!(dec_hw.layer_a.string_res);
        assert!(!dec_hw.layer_a.soft_release);
        assert!(!dec_hw.layer_a.pedal_noise);
        assert_eq!(dec_hw.layer_a.variation, 1);
        assert_eq!(dec_hw.layer_a.dyn_comp, 3);
        assert_eq!(dec_hw.layer_a.unison, 0);
        assert_eq!(dec_hw.layer_a.timbre, PianoTimbre::Soft);
        assert_eq!(dec_hw.layer_a.touch, VelocityTouch::Medium);
        assert_eq!(&dec_hw.pack().unwrap()[13..25], &hw[13..25]);
    }
}
