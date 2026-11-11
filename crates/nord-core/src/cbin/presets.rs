//! Decoded Organ / Piano / Synth / Global / Program bitstream structures.

use super::bitstream::{BitReader, BitWriter};
use super::container::{CbinError, CbinFile, CbinHeader, CbinKind};
use crate::morph::CrossfadeWidth;
use crate::organ::{OrganModel, RotarySpeed};
use crate::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use crate::synth::{FilterModel, SynthOscMode, VaWaveform};
use serde::{Deserialize, Serialize};

/// Decoded Organ section configuration across Layer A and Layer B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrganPresetDecoded {
    pub model: OrganModel,
    pub percussion_on: bool,
    pub percussion_soft: bool,
    pub percussion_fast: bool,
    pub percussion_third: bool,
    pub vibrato_on: bool,
    pub vibrato_type: u8,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    pub layer_a_enabled: bool,
    pub layer_a_level: f32,
    pub layer_a_octave: i8,
    pub drawbars_a: [u8; 9],
    pub layer_b_enabled: bool,
    pub layer_b_level: f32,
    pub layer_b_octave: i8,
    pub drawbars_b: [u8; 9],
}

impl Default for OrganPresetDecoded {
    fn default() -> Self {
        Self {
            model: OrganModel::B3,
            percussion_on: false,
            percussion_soft: false,
            percussion_fast: false,
            percussion_third: false,
            vibrato_on: false,
            vibrato_type: 0,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.0,
            layer_a_enabled: true,
            layer_a_level: 0.8,
            layer_a_octave: 0,
            drawbars_a: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            layer_b_enabled: false,
            layer_b_level: 0.8,
            layer_b_octave: 0,
            drawbars_b: [8, 8, 8, 8, 8, 8, 8, 8, 8],
        }
    }
}

impl OrganPresetDecoded {
    pub const SIGNATURE_PREFIX: [u8; 9] = [0x11, 0x27, 0xFB, 0xFB, 0xFB, 0xFF, 0xF7, 0xF7, 0xF7];
    pub const PAYLOAD_LEN: usize = 135;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(slice);
        // Skip first 9 signature bytes = 72 bits
        r.set_bit_pos(72)?;

        let model_idx = r.read_bits(3)?;
        let model = match model_idx {
            0 => OrganModel::B3,
            1 => OrganModel::Vox,
            2 => OrganModel::Farfisa,
            3 => OrganModel::Pipe1,
            _ => OrganModel::Pipe2,
        };
        let percussion_on = r.read_bool()?;
        let percussion_soft = r.read_bool()?;
        let percussion_fast = r.read_bool()?;
        let percussion_third = r.read_bool()?;
        let vibrato_on = r.read_bool()?;
        let vibrato_type = (r.read_bits(3)? as u8).min(5);
        let speed_idx = r.read_bits(2)?;
        let rotary_speed = match speed_idx {
            0 => RotarySpeed::Stop,
            1 => RotarySpeed::Slow,
            _ => RotarySpeed::Fast,
        };
        let rotary_drive = (r.read_bits(7)? as f32 / 127.0) * 10.0;

        let layer_a_enabled = r.read_bool()?;
        let layer_a_level = r.read_bits(7)? as f32 / 127.0;
        let layer_a_octave = (r.read_bits(3)? as i8) - 2;
        let mut drawbars_a = [0u8; 9];
        for db in &mut drawbars_a {
            *db = (r.read_bits(4)? as u8).min(8);
        }

        let layer_b_enabled = r.read_bool()?;
        let layer_b_level = r.read_bits(7)? as f32 / 127.0;
        let layer_b_octave = (r.read_bits(3)? as i8) - 2;
        let mut drawbars_b = [0u8; 9];
        for db in &mut drawbars_b {
            *db = (r.read_bits(4)? as u8).min(8);
        }

        Ok(Self {
            model,
            percussion_on,
            percussion_soft,
            percussion_fast,
            percussion_third,
            vibrato_on,
            vibrato_type,
            rotary_speed,
            rotary_drive,
            layer_a_enabled,
            layer_a_level,
            layer_a_octave,
            drawbars_a,
            layer_b_enabled,
            layer_b_level,
            layer_b_octave,
            drawbars_b,
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = vec![0u8; Self::PAYLOAD_LEN];
        out[..9].copy_from_slice(&Self::SIGNATURE_PREFIX);

        let mut w = BitWriter::new(&mut out);
        w.set_bit_pos(72)?;

        let model_idx = match self.model {
            OrganModel::B3 => 0,
            OrganModel::Vox => 1,
            OrganModel::Farfisa => 2,
            OrganModel::Pipe1 => 3,
            OrganModel::Pipe2 => 4,
        };
        w.write_bits(model_idx, 3)?;
        w.write_bool(self.percussion_on)?;
        w.write_bool(self.percussion_soft)?;
        w.write_bool(self.percussion_fast)?;
        w.write_bool(self.percussion_third)?;
        w.write_bool(self.vibrato_on)?;
        w.write_bits(u32::from(self.vibrato_type & 0x07), 3)?;
        let speed_idx = match self.rotary_speed {
            RotarySpeed::Stop => 0,
            RotarySpeed::Slow => 1,
            RotarySpeed::Fast => 2,
        };
        w.write_bits(speed_idx, 2)?;
        let drive_bits = ((self.rotary_drive / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32;
        w.write_bits(drive_bits, 7)?;

        w.write_bool(self.layer_a_enabled)?;
        w.write_bits(
            (self.layer_a_level.clamp(0.0, 1.0) * 127.0).round() as u32,
            7,
        )?;
        w.write_bits((self.layer_a_octave + 2).clamp(0, 4) as u32, 3)?;
        for &db in &self.drawbars_a {
            w.write_bits(u32::from(db.min(8)), 4)?;
        }

        w.write_bool(self.layer_b_enabled)?;
        w.write_bits(
            (self.layer_b_level.clamp(0.0, 1.0) * 127.0).round() as u32,
            7,
        )?;
        w.write_bits((self.layer_b_octave + 2).clamp(0, 4) as u32, 3)?;
        for &db in &self.drawbars_b {
            w.write_bits(u32::from(db.min(8)), 4)?;
        }

        Ok(out)
    }
}

/// Decoded single Piano layer configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PianoLayerDecoded {
    pub enabled: bool,
    pub level: f32,
    pub octave: i8,
    pub category: PianoCategory,
    pub model_id: u8,
    pub timbre: PianoTimbre,
    pub touch: VelocityTouch,
    pub string_res: bool,
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
            octave: 0,
            category: PianoCategory::Grand,
            model_id: 1,
            timbre: PianoTimbre::Off,
            touch: VelocityTouch::Medium,
            string_res: true,
            pedal_noise: true,
            soft_release: false,
            dyn_comp: 0,
            unison: 0,
        }
    }
}

/// Decoded Piano section configuration across Layer A and Layer B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PianoPresetDecoded {
    pub layer_a: PianoLayerDecoded,
    pub layer_b: PianoLayerDecoded,
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
        }
    }
}

impl PianoPresetDecoded {
    pub const SIGNATURE_PREFIX: [u8; 9] = [0x0C, 0x27, 0x1B, 0xFB, 0xFB, 0xFD, 0xA7, 0xF7, 0xF7];
    pub const PAYLOAD_LEN: usize = 147;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(slice);
        r.set_bit_pos(72)?;

        let unpack_layer = |r: &mut BitReader| -> Result<PianoLayerDecoded, CbinError> {
            let enabled = r.read_bool()?;
            let level = r.read_bits(7)? as f32 / 127.0;
            let octave = (r.read_bits(3)? as i8) - 2;
            let cat_idx = r.read_bits(3)?;
            let category = match cat_idx {
                0 => PianoCategory::Grand,
                1 => PianoCategory::Upright,
                2 => PianoCategory::Electric,
                3 => PianoCategory::Clav,
                4 => PianoCategory::Digital,
                _ => PianoCategory::Misc,
            };
            let model_id = r.read_bits(8)? as u8;
            let timbre_idx = r.read_bits(3)?;
            let timbre = match timbre_idx {
                0 => PianoTimbre::Off,
                1 => PianoTimbre::Soft,
                2 => PianoTimbre::Mid,
                3 => PianoTimbre::Bright,
                4 => PianoTimbre::Dyno1,
                _ => PianoTimbre::Dyno2,
            };
            let touch_idx = r.read_bits(2)?;
            let touch = match touch_idx {
                0 => VelocityTouch::Light,
                1 => VelocityTouch::Medium,
                _ => VelocityTouch::Heavy,
            };
            let string_res = r.read_bool()?;
            let pedal_noise = r.read_bool()?;
            let soft_release = r.read_bool()?;
            let dyn_comp = (r.read_bits(2)? as u8).min(3);
            let unison = (r.read_bits(2)? as u8).min(3);

            Ok(PianoLayerDecoded {
                enabled,
                level,
                octave,
                category,
                model_id,
                timbre,
                touch,
                string_res,
                pedal_noise,
                soft_release,
                dyn_comp,
                unison,
            })
        };

        let layer_a = unpack_layer(&mut r)?;
        let layer_b = unpack_layer(&mut r)?;

        Ok(Self { layer_a, layer_b })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = vec![0u8; Self::PAYLOAD_LEN];
        out[..9].copy_from_slice(&Self::SIGNATURE_PREFIX);

        let mut w = BitWriter::new(&mut out);
        w.set_bit_pos(72)?;

        let pack_layer = |l: &PianoLayerDecoded, w: &mut BitWriter| -> Result<(), CbinError> {
            w.write_bool(l.enabled)?;
            w.write_bits((l.level.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
            w.write_bits((l.octave + 2).clamp(0, 4) as u32, 3)?;
            let cat_idx = match l.category {
                PianoCategory::Grand => 0,
                PianoCategory::Upright => 1,
                PianoCategory::Electric => 2,
                PianoCategory::Clav => 3,
                PianoCategory::Digital => 4,
                PianoCategory::Misc => 5,
            };
            w.write_bits(cat_idx, 3)?;
            w.write_bits(u32::from(l.model_id), 8)?;
            let timbre_idx = match l.timbre {
                PianoTimbre::Off => 0,
                PianoTimbre::Soft => 1,
                PianoTimbre::Mid => 2,
                PianoTimbre::Bright => 3,
                PianoTimbre::Dyno1 => 4,
                PianoTimbre::Dyno2 => 5,
            };
            w.write_bits(timbre_idx, 3)?;
            let touch_idx = match l.touch {
                VelocityTouch::Light => 0,
                VelocityTouch::Medium => 1,
                VelocityTouch::Heavy => 2,
            };
            w.write_bits(touch_idx, 2)?;
            w.write_bool(l.string_res)?;
            w.write_bool(l.pedal_noise)?;
            w.write_bool(l.soft_release)?;
            w.write_bits(u32::from(l.dyn_comp.min(3)), 2)?;
            w.write_bits(u32::from(l.unison.min(3)), 2)?;
            Ok(())
        };

        pack_layer(&self.layer_a, &mut w)?;
        pack_layer(&self.layer_b, &mut w)?;

        Ok(out)
    }
}

/// Decoded single Synth layer configuration (Nord Wave 2 voice architecture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthLayerDecoded {
    pub enabled: bool,
    pub level: f32,
    pub octave: i8,
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    pub wave_id: u8,
    pub detune: f32,
    pub filter_model: FilterModel,
    pub filter_cutoff_hz: f32,
    pub filter_resonance: f32,
    pub filter_drive: f32,
    pub kb_track: u8,
    pub env_attack_ms: f32,
    pub env_decay_ms: f32,
    pub env_release_ms: f32,
    pub lfo_rate_hz: f32,
    pub arp_enabled: bool,
    pub arp_bpm: f32,
}

impl Default for SynthLayerDecoded {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 0.8,
            octave: 0,
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::Saw,
            wave_id: 0,
            detune: 0.0,
            filter_model: FilterModel::Ladder24,
            filter_cutoff_hz: 4000.0,
            filter_resonance: 0.2,
            filter_drive: 0.0,
            kb_track: 2,
            env_attack_ms: 10.0,
            env_decay_ms: 300.0,
            env_release_ms: 250.0,
            lfo_rate_hz: 2.5,
            arp_enabled: false,
            arp_bpm: 120.0,
        }
    }
}

/// Decoded Synth section configuration across 3 layers (A, B, C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthPresetDecoded {
    pub layer_a: SynthLayerDecoded,
    pub layer_b: SynthLayerDecoded,
    pub layer_c: SynthLayerDecoded,
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
        }
    }
}

impl SynthPresetDecoded {
    pub const SIGNATURE_PREFIX: [u8; 9] = [0x1E, 0x08, 0x71, 0xBF, 0xBF, 0xBF, 0xD2, 0x7F, 0x7F];
    pub const PAYLOAD_LEN: usize = 493;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(slice);
        r.set_bit_pos(72)?;

        let unpack_layer = |r: &mut BitReader| -> Result<SynthLayerDecoded, CbinError> {
            let enabled = r.read_bool()?;
            let level = r.read_bits(7)? as f32 / 127.0;
            let octave = (r.read_bits(3)? as i8) - 2;
            let mode_idx = r.read_bits(2)?;
            let osc_mode = match mode_idx {
                0 => SynthOscMode::Analog,
                1 => SynthOscMode::Wavetable,
                2 => SynthOscMode::Fm,
                _ => SynthOscMode::Sample,
            };
            let wave_idx = r.read_bits(2)?;
            let waveform = match wave_idx {
                0 => VaWaveform::Saw,
                1 => VaWaveform::Pulse,
                2 => VaWaveform::Triangle,
                _ => VaWaveform::Sine,
            };
            let wave_id = r.read_bits(8)? as u8;
            let detune = (r.read_bits(7)? as f32 / 127.0) * 10.0;

            let filter_idx = r.read_bits(3)?;
            let filter_model = match filter_idx {
                0 => FilterModel::Lp12,
                1 => FilterModel::Lp24,
                2 => FilterModel::Ladder24,
                3 => FilterModel::BandPass,
                _ => FilterModel::HighPass,
            };
            let cutoff_norm = r.read_bits(7)? as f32 / 127.0;
            let filter_cutoff_hz = 20.0 * (1000.0f32).powf(cutoff_norm);
            let filter_resonance = r.read_bits(7)? as f32 / 127.0;
            let filter_drive = r.read_bits(7)? as f32 / 127.0;
            let kb_track = (r.read_bits(2)? as u8).min(3);

            let env_attack_ms = 1.0 + (r.read_bits(7)? as f32 / 127.0) * 4999.0;
            let env_decay_ms = 10.0 + (r.read_bits(7)? as f32 / 127.0) * 9990.0;
            let env_release_ms = 10.0 + (r.read_bits(7)? as f32 / 127.0) * 9990.0;
            let lfo_rate_hz = 0.1 + (r.read_bits(7)? as f32 / 127.0) * 29.9;
            let arp_enabled = r.read_bool()?;
            let arp_bpm = 30.0 + (r.read_bits(8)? as f32 / 255.0) * 270.0;

            Ok(SynthLayerDecoded {
                enabled,
                level,
                octave,
                osc_mode,
                waveform,
                wave_id,
                detune,
                filter_model,
                filter_cutoff_hz,
                filter_resonance,
                filter_drive,
                kb_track,
                env_attack_ms,
                env_decay_ms,
                env_release_ms,
                lfo_rate_hz,
                arp_enabled,
                arp_bpm,
            })
        };

        let layer_a = unpack_layer(&mut r)?;
        let layer_b = unpack_layer(&mut r)?;
        let layer_c = unpack_layer(&mut r)?;

        Ok(Self {
            layer_a,
            layer_b,
            layer_c,
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = vec![0u8; Self::PAYLOAD_LEN];
        out[..9].copy_from_slice(&Self::SIGNATURE_PREFIX);

        let mut w = BitWriter::new(&mut out);
        w.set_bit_pos(72)?;

        let pack_layer = |l: &SynthLayerDecoded, w: &mut BitWriter| -> Result<(), CbinError> {
            w.write_bool(l.enabled)?;
            w.write_bits((l.level.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
            w.write_bits((l.octave + 2).clamp(0, 4) as u32, 3)?;
            let mode_idx = match l.osc_mode {
                SynthOscMode::Analog => 0,
                SynthOscMode::Wavetable => 1,
                SynthOscMode::Fm => 2,
                SynthOscMode::Sample => 3,
            };
            w.write_bits(mode_idx, 2)?;
            let wave_idx = match l.waveform {
                VaWaveform::Saw | VaWaveform::SuperSaw => 0,
                VaWaveform::Pulse => 1,
                VaWaveform::Triangle => 2,
                VaWaveform::Sine => 3,
            };
            w.write_bits(wave_idx, 2)?;
            w.write_bits(u32::from(l.wave_id), 8)?;
            w.write_bits(
                ((l.detune / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
                7,
            )?;

            let filter_idx = match l.filter_model {
                FilterModel::Lp12 => 0,
                FilterModel::Lp24 => 1,
                FilterModel::Ladder24 => 2,
                FilterModel::BandPass => 3,
                FilterModel::HighPass => 4,
            };
            w.write_bits(filter_idx, 3)?;
            let cutoff_norm = ((l.filter_cutoff_hz.max(20.0) / 20.0).log10() / 3.0).clamp(0.0, 1.0);
            w.write_bits((cutoff_norm * 127.0).round() as u32, 7)?;
            w.write_bits(
                (l.filter_resonance.clamp(0.0, 1.0) * 127.0).round() as u32,
                7,
            )?;
            w.write_bits((l.filter_drive.clamp(0.0, 1.0) * 127.0).round() as u32, 7)?;
            w.write_bits(u32::from(l.kb_track.min(3)), 2)?;

            let att_norm = ((l.env_attack_ms - 1.0) / 4999.0).clamp(0.0, 1.0);
            w.write_bits((att_norm * 127.0).round() as u32, 7)?;
            let dec_norm = ((l.env_decay_ms - 10.0) / 9990.0).clamp(0.0, 1.0);
            w.write_bits((dec_norm * 127.0).round() as u32, 7)?;
            let rel_norm = ((l.env_release_ms - 10.0) / 9990.0).clamp(0.0, 1.0);
            w.write_bits((rel_norm * 127.0).round() as u32, 7)?;
            let lfo_norm = ((l.lfo_rate_hz - 0.1) / 29.9).clamp(0.0, 1.0);
            w.write_bits((lfo_norm * 127.0).round() as u32, 7)?;
            w.write_bool(l.arp_enabled)?;
            let bpm_norm = ((l.arp_bpm - 30.0) / 270.0).clamp(0.0, 1.0);
            w.write_bits((bpm_norm * 255.0).round() as u32, 8)?;

            Ok(())
        };

        pack_layer(&self.layer_a, &mut w)?;
        pack_layer(&self.layer_b, &mut w)?;
        pack_layer(&self.layer_c, &mut w)?;

        Ok(out)
    }
}

/// Decoded global program parameters (Split zones, Layer Scenes, Master Clock, Master Compressor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalProgramDecoded {
    pub tempo_bpm: f32,
    pub master_compressor: f32,
    pub transpose: i8,
    pub split_enabled: bool,
    pub split_points: [u8; 3],
    pub split_crossfades: [CrossfadeWidth; 3],
    pub scene_1_mask: u8,
    pub scene_2_mask: u8,
}

impl Default for GlobalProgramDecoded {
    fn default() -> Self {
        Self {
            tempo_bpm: 120.0,
            master_compressor: 0.0,
            transpose: 0,
            split_enabled: false,
            split_points: [53, 65, 77], // F3, F4, F5
            split_crossfades: [
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
            ],
            scene_1_mask: 0b0001_0101, // Organ A, Piano A, Synth A active
            scene_2_mask: 0b0111_1111, // All 7 layers active in Scene II
        }
    }
}

impl GlobalProgramDecoded {
    pub const PAYLOAD_LEN: usize = 45;

    pub fn unpack(slice: &[u8]) -> Result<Self, CbinError> {
        if slice.len() < Self::PAYLOAD_LEN {
            return Err(CbinError::TooShort {
                actual: slice.len(),
                min: Self::PAYLOAD_LEN,
            });
        }
        let mut r = BitReader::new(slice);
        // Byte 0 is 0x2C signature = 8 bits
        r.set_bit_pos(8)?;

        let tempo_bpm = 30.0 + (r.read_bits(9)? as f32 / 511.0) * 270.0;
        let master_compressor = (r.read_bits(7)? as f32 / 127.0) * 10.0;
        let transpose = (r.read_bits(4)? as i8) - 6;
        let split_enabled = r.read_bool()?;

        let mut split_points = [0u8; 3];
        let mut split_crossfades = [CrossfadeWidth::Off; 3];
        for i in 0..3 {
            split_points[i] = r.read_bits(7)? as u8;
            let cf_idx = r.read_bits(2)?;
            split_crossfades[i] = match cf_idx {
                0 => CrossfadeWidth::Off,
                1 => CrossfadeWidth::Small,
                _ => CrossfadeWidth::Large,
            };
        }

        let scene_1_mask = r.read_bits(7)? as u8;
        let scene_2_mask = r.read_bits(7)? as u8;

        Ok(Self {
            tempo_bpm,
            master_compressor,
            transpose,
            split_enabled,
            split_points,
            split_crossfades,
            scene_1_mask,
            scene_2_mask,
        })
    }

    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        let mut out = vec![0u8; Self::PAYLOAD_LEN];
        out[0] = 0x2C;

        let mut w = BitWriter::new(&mut out);
        w.set_bit_pos(8)?;

        let bpm_norm = ((self.tempo_bpm - 30.0) / 270.0).clamp(0.0, 1.0);
        w.write_bits((bpm_norm * 511.0).round() as u32, 9)?;
        let comp_norm = (self.master_compressor / 10.0).clamp(0.0, 1.0);
        w.write_bits((comp_norm * 127.0).round() as u32, 7)?;
        w.write_bits((self.transpose + 6).clamp(0, 12) as u32, 4)?;
        w.write_bool(self.split_enabled)?;

        for i in 0..3 {
            w.write_bits(u32::from(self.split_points[i] & 0x7F), 7)?;
            let cf_idx = match self.split_crossfades[i] {
                CrossfadeWidth::Off => 0,
                CrossfadeWidth::Small => 1,
                CrossfadeWidth::Large => 2,
            };
            w.write_bits(cf_idx, 2)?;
        }

        w.write_bits(u32::from(self.scene_1_mask & 0x7F), 7)?;
        w.write_bits(u32::from(self.scene_2_mask & 0x7F), 7)?;

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

        let global = GlobalProgramDecoded::unpack(&cbin.payload[0..45])?;
        let organ = OrganPresetDecoded::unpack(&cbin.payload[45..180])?;
        let piano = PianoPresetDecoded::unpack(&cbin.payload[180..327])?;
        let synth = SynthPresetDecoded::unpack(&cbin.payload[327..820])?;

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
    fn test_organ_preset_bitstream_pack_and_unpack() {
        let original = OrganPresetDecoded {
            model: OrganModel::Vox,
            percussion_on: true,
            percussion_soft: true,
            percussion_fast: false,
            percussion_third: true,
            vibrato_on: true,
            vibrato_type: 3,
            rotary_speed: RotarySpeed::Fast,
            rotary_drive: 7.5,
            drawbars_a: [8, 4, 2, 0, 1, 3, 5, 7, 8],
            drawbars_b: [0, 1, 2, 3, 4, 5, 6, 7, 8],
            ..Default::default()
        };

        let packed = original.pack().expect("pack organ");
        assert_eq!(packed.len(), 135);
        assert_eq!(&packed[..9], &OrganPresetDecoded::SIGNATURE_PREFIX);

        let unpacked = OrganPresetDecoded::unpack(&packed).expect("unpack organ");
        assert_eq!(unpacked.model, OrganModel::Vox);
        assert!(unpacked.percussion_on);
        assert!(unpacked.percussion_soft);
        assert!(!unpacked.percussion_fast);
        assert!(unpacked.percussion_third);
        assert!(unpacked.vibrato_on);
        assert_eq!(unpacked.vibrato_type, 3);
        assert_eq!(unpacked.rotary_speed, RotarySpeed::Fast);
        assert!((unpacked.rotary_drive - 7.5).abs() < 0.1);
        assert_eq!(unpacked.drawbars_a, original.drawbars_a);
        assert_eq!(unpacked.drawbars_b, original.drawbars_b);
    }

    #[test]
    fn test_piano_preset_bitstream_pack_and_unpack() {
        let original = PianoPresetDecoded {
            layer_a: PianoLayerDecoded {
                category: PianoCategory::Electric,
                model_id: 42,
                timbre: PianoTimbre::Bright,
                touch: VelocityTouch::Heavy,
                string_res: true,
                pedal_noise: true,
                soft_release: true,
                dyn_comp: 2,
                unison: 3,
                ..Default::default()
            },
            ..Default::default()
        };

        let packed = original.pack().expect("pack piano");
        assert_eq!(packed.len(), 147);
        assert_eq!(&packed[..9], &PianoPresetDecoded::SIGNATURE_PREFIX);

        let unpacked = PianoPresetDecoded::unpack(&packed).expect("unpack piano");
        assert_eq!(unpacked.layer_a.category, PianoCategory::Electric);
        assert_eq!(unpacked.layer_a.model_id, 42);
        assert_eq!(unpacked.layer_a.timbre, PianoTimbre::Bright);
        assert_eq!(unpacked.layer_a.touch, VelocityTouch::Heavy);
        assert!(unpacked.layer_a.string_res);
        assert!(unpacked.layer_a.pedal_noise);
        assert!(unpacked.layer_a.soft_release);
        assert_eq!(unpacked.layer_a.dyn_comp, 2);
        assert_eq!(unpacked.layer_a.unison, 3);
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
                filter_resonance: 0.65,
                filter_drive: 0.45,
                arp_enabled: true,
                arp_bpm: 135.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let packed = original.pack().expect("pack synth");
        assert_eq!(packed.len(), 493);
        assert_eq!(&packed[..9], &SynthPresetDecoded::SIGNATURE_PREFIX);

        let unpacked = SynthPresetDecoded::unpack(&packed).expect("unpack synth");
        assert_eq!(unpacked.layer_a.osc_mode, SynthOscMode::Wavetable);
        assert_eq!(unpacked.layer_a.wave_id, 17);
        assert!((unpacked.layer_a.detune - 3.4).abs() < 0.2);
        assert_eq!(unpacked.layer_a.filter_model, FilterModel::Lp24);
        assert!((unpacked.layer_a.filter_cutoff_hz - 1200.0).abs() < 100.0);
        assert!((unpacked.layer_a.filter_resonance - 0.65).abs() < 0.05);
        assert!((unpacked.layer_a.filter_drive - 0.45).abs() < 0.05);
        assert!(unpacked.layer_a.arp_enabled);
        assert!((unpacked.layer_a.arp_bpm - 135.0).abs() < 2.0);
    }

    #[test]
    fn test_program_decoded_roundtrip_cbin_and_json() {
        let prog = ProgramDecoded {
            global: GlobalProgramDecoded {
                tempo_bpm: 132.0,
                master_compressor: 4.5,
                split_enabled: true,
                split_points: [53, 65, 77],
                split_crossfades: [
                    CrossfadeWidth::Small,
                    CrossfadeWidth::Large,
                    CrossfadeWidth::Off,
                ],
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

        // 1. Test JSON conversion
        let json = prog.to_json().expect("to json");
        assert!(json.contains("Init Program"));
        assert!(json.contains("Farfisa"));
        let parsed_from_json = ProgramDecoded::from_json(&json).expect("from json");
        assert_eq!(parsed_from_json.global.tempo_bpm.round(), 132.0);
        assert_eq!(parsed_from_json.organ.model, OrganModel::Farfisa);
        assert_eq!(parsed_from_json.piano.layer_a.timbre, PianoTimbre::Dyno1);

        // 2. Test CBIN packing and decoding
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
        assert_eq!(decoded.global.split_crossfades[0], CrossfadeWidth::Small);
        assert_eq!(decoded.global.split_crossfades[1], CrossfadeWidth::Large);
        assert_eq!(decoded.global.split_crossfades[2], CrossfadeWidth::Off);
    }
}
