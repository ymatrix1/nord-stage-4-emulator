//! Per-layer FX chain block (`0x36`, 55 bytes = 440 bits).
//!
//! Spec: `docs/specs/effects-and-routing.md`, `docs/specs/cbin-and-presets.md`.

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use super::presets::*;
use crate::fx::{Mod1Type, Mod2Type, ReverbModel};
use serde::{Deserialize, Serialize};

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
    pub(in crate::cbin) raw_payload: Vec<u8>,
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
}
