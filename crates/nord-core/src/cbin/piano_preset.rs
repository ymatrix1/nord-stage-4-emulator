//! Piano program block (`0x0C`) with two `0x0B` layers and per-layer FX.
//!
//! Spec: `docs/specs/cbin-and-presets.md` (Piano Block).

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use super::fx_preset::FxDecoded;
use super::presets::*;
use crate::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use serde::{Deserialize, Serialize};

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
    pub(in crate::cbin) raw_payload: Vec<u8>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
