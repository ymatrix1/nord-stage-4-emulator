//! Decoded Organ / Piano / Synth / FX / Global / Program bitstream structures.
//!
//! All bit offsets and morph encodings (`W`-bit base + three `(W+1)`-bit
//! Wheel/Aftertouch/Control-Pedal deltas with zero sentinel `max_val`) match
//! Nord Stage 4 OS v1.66 `Buffer_*` serialization and verified factory `.ns4b` presets.

use super::bitstream::{BitReader, BitWriter};
use super::container::{CbinError, CbinFile, CbinHeader, CbinKind};
pub use super::fx_preset::FxDecoded;
pub use super::organ_preset::OrganPresetDecoded;
pub use super::piano_preset::{PianoLayerDecoded, PianoPresetDecoded};
use super::synth_preset::SynthPresetDecoded;
use crate::morph::CrossfadeWidth;
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

pub(super) fn decode_signed(raw: u32, bits: usize) -> i32 {
    let sign_bit = 1u32 << (bits - 1);
    let mask = (1u32 << bits) - 1;
    let masked = raw & mask;
    if (masked & sign_bit) != 0 {
        (masked as i32) - ((1u32 << bits) as i32)
    } else {
        masked as i32
    }
}

pub(super) fn encode_signed(val: i32, bits: usize) -> u32 {
    let mask = (1u32 << bits) - 1;
    (val as u32) & mask
}

pub(super) fn read_morph(
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

pub(super) fn write_morph(
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

pub(super) fn prepare_buffer(raw_payload: &[u8], expected_len: usize, block_tag: u8) -> Vec<u8> {
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
/// VERIFIED: the displayed value of all 128 steps = firmware display-string table at the same address (bank0_CODE.bin pointer array of 8-byte strings), checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, 0 mismatches within display rounding. The DSP-side value behind the display is not read from firmware.
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
/// VERIFIED: the displayed value of all 128 steps = firmware display-string table at the same address (bank0_CODE.bin pointer array of 8-byte strings), checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, 0 mismatches within display rounding. The DSP-side value behind the display is not read from firmware.
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
/// VERIFIED: the displayed value of all 128 steps = firmware display-string table at the same address (bank0_CODE.bin pointer array of 8-byte strings), checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, 0 mismatches within display rounding. The DSP-side value behind the display is not read from firmware.
pub const ENV_RELEASE_MS_TABLE: [f32; 128] = ENV_DECAY_MS_TABLE;

/// Exact 128-step hardware Synth Filter Cutoff curve (Hz) from NS4 OS v1.66 (`0x53b1e4`).
/// VERIFIED: the displayed value of all 128 steps = firmware display-string table at the same address (bank0_CODE.bin pointer array of 8-byte strings), checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, 0 mismatches within display rounding. The DSP-side value behind the display is not read from firmware.
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
/// VERIFIED: the displayed value of all 128 steps = firmware display-string table at the same address (bank0_CODE.bin pointer array of 8-byte strings), checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, 0 mismatches within display rounding. The DSP-side value behind the display is not read from firmware.
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

pub(super) fn table_val_to_idx(table: &[f32; 128], val: f32) -> u32 {
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

pub(super) fn split_idx_to_midi(idx: u32) -> u8 {
    SPLIT_NOTE_TABLE[(idx as usize).min(SPLIT_NOTE_TABLE.len() - 1)]
}

pub(super) fn midi_to_split_idx(midi: u8) -> u32 {
    SPLIT_NOTE_TABLE
        .iter()
        .enumerate()
        .min_by_key(|(_, &n)| n.abs_diff(midi))
        .map_or(4, |(idx, _)| idx as u32)
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
    use crate::cbin::synth_preset::SynthLayerDecoded;
    use crate::organ::OrganModel;
    use crate::piano::PianoTimbre;
    use crate::synth::SynthOscMode;

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
}
