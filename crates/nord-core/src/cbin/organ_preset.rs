//! Organ program block (`0x11`) with two `0x03` layers and shared FX.
//!
//! Spec: `docs/specs/cbin-and-presets.md` (Organ Block).

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use super::fx_preset::FxDecoded;
use super::presets::*;
use crate::organ::{OrganModel, RotarySpeed, ScannerMode};
use serde::{Deserialize, Serialize};

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
    pub(in crate::cbin) raw_payload: Vec<u8>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
