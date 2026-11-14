//! Synth program block (`0x1E`): three `0x32` Layer + three `0x2F` Sound blocks and `0x36` FX.
//!
//! Spec: `docs/specs/cbin-and-presets.md` (Synth Block). Offsets and value scales are cross-checked
//! against the ns4decode regression program, see `docs/research/synth-0x2f-ns4decode-crosscheck.md`.

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use super::presets::{
    encode_signed, prepare_buffer, read_morph, table_val_to_idx, write_morph, FxDecoded,
    MorphDeltas, ENV_ATTACK_MS_TABLE, ENV_DECAY_MS_TABLE, ENV_RELEASE_MS_TABLE,
    FILTER_CUTOFF_HZ_TABLE, LFO_RATE_HZ_TABLE,
};
use super::synth_arp::SynthArpDecoded;
use super::synth_osc::map_osc_mode_and_wave;
use super::synth_osc::wave_from_flat_index;
use super::synth_pattern::SynthPatternDecoded;
use crate::synth::{EnvKnobs, FilterModel, StripKnobs, SynthOscMode, VaWaveform};
use serde::{Deserialize, Serialize};

/// Decoded single Synth layer configuration (`0x32` Layer + `0x2F` Sound).
///
/// Field offsets (block-relative bits): VERIFIED for order/widths: field order and widths chain = firmware `Buffer_kSynLay` @0x2d70b8 / `Buffer_kSynSnd` @0x2d751c (bank0_CODE.bin) name tables, checked by `python tools/ns4_fw_order_check.py target/fw_unpacked/bank0_CODE.bin`, 0 problems over 50 fields. Absolute anchors are EMPIRICAL (ns4decode + corpus); value scales are ASSUMED unless stated.
/// Offsets come from the third-party ns4decode map (MIT, `.reference/ns4decode_pkg`),
/// cross-checked by `python tools/ns4_crosscheck.py` (see `docs/research/synth-0x2f-ns4decode-crosscheck.md`).
/// `0x32` Layer: octave `+62` (4b signed), legato `+77`, mono `+78`, voice priority `+79` (2b),
/// glide `+81` (7b), unison `+306` (2b). `0x2F` Sound: osc type `+10` (2b), category `+15` (3b),
/// wave `+19` (6b), pitch fine `+56` (7b signed), coarse `+63` (6b signed), osc env ADR
/// `+71/+78/+85`, osc env amt `+92` (7b signed), env-to-pitch `+123`, osc env velocity `+124`,
/// LFO target `+127` (2b), shape `+129` (3b), master clock `+132`, amp velocity `+216` (2b),
/// filter ADR `+318/+325/+332`, filter velocity `+339`, filter on `+340`, vibrato rate `+341`
/// (7b), vibrato amount `+348` (6b). The value-to-unit scales are ASSUMED unless stated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthLayerDecoded {
    pub enabled: bool,
    pub level: f32,
    pub level_morph: MorphDeltas,
    pub pan: f32,
    /// Octave shift, `-2..=2` (4b two's complement).
    pub octave: i8,
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    /// Analog: raw wave index; FM: ratio index; Wavetable: flat index over all WAVE categories.
    pub wave_id: u8,
    /// Oscillator type: `0` ANALOG, `1` FM-H, `2` FM-I, `3` WAVE.
    pub osc_type: u8,
    /// Category (3b): ANALOG `PURE/SUB OSC/SYNC/SHAPE/SHAPE SINE/MULTI/SUPER/MISC`; WAVE
    /// `BELLS-TINES/ACOUSTIC/DIGITAL/ORGAN/KEYS`; FM `A..H` letters.
    pub osc_category: u8,
    /// Wave within the category (6b).
    pub osc_wave: u8,
    pub sample_id: u32,
    pub detune: f32,
    pub osc_ctrl_morph: MorphDeltas,
    /// Fine tune, cents, `-64..=63`.
    pub pitch_fine_cents: i8,
    /// Coarse tune, semitones, `-32..=31`.
    pub pitch_coarse_semi: i8,
    pub filter_model: FilterModel,
    pub filter_on: bool,
    pub filter_cutoff_hz: f32,
    pub filter_cutoff_morph: MorphDeltas,
    pub filter_resonance: f32,
    pub filter_resonance_morph: MorphDeltas,
    pub filter_drive: f32,
    /// Filter envelope amount `0..1` (panel `0..10`, unipolar).
    pub filter_env_amount: f32,
    pub kb_track: u8,
    pub env_attack_ms: f32,
    pub env_decay_ms: f32,
    pub env_release_ms: f32,
    /// Amp velocity sensitivity setting `0..=3` (`0` = off).
    pub amp_velocity: u8,
    pub lfo_rate_hz: f32,
    pub lfo_amount: f32,
    pub lfo_shape: crate::synth::LfoShape,
    pub lfo_dest: crate::synth::LfoDestination,
    pub lfo_mst_clk: bool,
    pub arp_enabled: bool,
    pub arp_bpm: f32,
    /// Arpeggiator / Gate pattern words (`cbin::synth_pattern`); the engine imports them as the
    /// layer's custom pattern.
    pub pattern: SynthPatternDecoded,
    /// Arpeggiator mode fields (Arp / Poly / Gate, direction, zig zag, Master Clock, KB sync,
    /// range; `cbin::synth_arp`).
    pub arp: SynthArpDecoded,
    /// `false` = filter envelope mirrors the amp envelope decay (programmatic default).
    pub filter_env_dedicated: bool,
    pub filter_env_attack_ms: f32,
    pub filter_env_decay_ms: f32,
    pub filter_env_release_ms: f32,
    /// Raw amp and filter envelope knobs (`0..=127`) of a decoded program: the ARM writes the rate
    /// index `knob - 64` (`synth/fw_knob_laws.rs`). `None` for a layer built in code.
    pub knobs: Option<StripKnobs>,
    /// Filter envelope depth follows velocity (on/off on the panel).
    pub filter_velocity: bool,
    pub osc_env_attack_ms: f32,
    pub osc_env_decay_ms: f32,
    pub osc_env_release_ms: f32,
    /// Bipolar osc envelope amount `-1..1` (panel `-10..10`).
    pub osc_env_amount: f32,
    pub osc_env_to_pitch: bool,
    pub osc_env_velocity: bool,
    /// `0` = off, `1`/`2`/`3` = stereo unison width.
    pub unison: u8,
    pub mono: bool,
    pub legato: bool,
    /// Voice priority: `0` none, `1` LO, `2` HI.
    pub voice_priority: u8,
    /// Glide knob `0..10`.
    pub glide: f32,
    /// Vibrato source: `0` none, `1` ON, `2` DLY, `3` WHL, `4` A.T., `5` PED.
    pub vibrato_mode: u8,
    /// Vibrato delay, seconds (`0.1 + 2.9 * raw / 29`).
    pub vibrato_delay_sec: f32,
    /// Vibrato rate in Hz. EMPIRICAL: range `2.0..8.0 Hz` from NS4 manual p.37 (Vibrato Menu); the linear
    /// 128-step mapping `2 + 6 * raw / 127` is ASSUMED (ns4decode notes a 0.1 Hz display anomaly).
    pub vibrato_rate_hz: f32,
    /// Vibrato amount `0..10`.
    /// EMPIRICAL: panel range `0..10` from NS4 manual p.37; depth in cents is ASSUMED (`synth_params.rs`).
    pub vibrato_amount: f32,
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
            osc_type: 0,
            osc_category: 0,
            osc_wave: 2,
            sample_id: 0,
            detune: 0.0,
            osc_ctrl_morph: MorphDeltas::default(),
            pitch_fine_cents: 0,
            pitch_coarse_semi: 0,
            filter_model: FilterModel::Ladder24,
            filter_on: true,
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
            amp_velocity: 3,
            lfo_rate_hz: 2.5,
            lfo_amount: 0.0,
            lfo_shape: crate::synth::LfoShape::Triangle,
            lfo_dest: crate::synth::LfoDestination::FilterCutoff,
            lfo_mst_clk: false,
            arp_enabled: false,
            arp_bpm: 120.0,
            pattern: SynthPatternDecoded::default(),
            arp: SynthArpDecoded::default(),
            filter_env_dedicated: false,
            filter_env_attack_ms: 10.0,
            filter_env_decay_ms: 300.0,
            filter_env_release_ms: 250.0,
            knobs: None,
            filter_velocity: false,
            osc_env_attack_ms: 10.0,
            osc_env_decay_ms: 300.0,
            osc_env_release_ms: 250.0,
            osc_env_amount: 0.0,
            osc_env_to_pitch: false,
            osc_env_velocity: false,
            unison: 0,
            mono: false,
            legato: false,
            voice_priority: 0,
            glide: 0.0,
            vibrato_mode: 0,
            vibrato_delay_sec: 0.1,
            vibrato_rate_hz: 5.0,
            vibrato_amount: 0.0,
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
    pub(in crate::cbin) raw_payload: Vec<u8>,
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

/// Canonical `(type, category, wave)` for a programmatically built layer whose
/// type/category/wave fields disagree with `osc_mode`/`waveform`/`wave_id`.
fn canonical_osc_fields(layer: &SynthLayerDecoded) -> (u8, u8, u8) {
    match layer.osc_mode {
        SynthOscMode::Sample => (layer.osc_type, layer.osc_category, layer.osc_wave),
        SynthOscMode::Fm => {
            let t = if layer.osc_type == 2 { 2 } else { 1 };
            (t, layer.osc_category & 0x07, layer.wave_id & 0x3F)
        }
        SynthOscMode::Wavetable => {
            let (cat, wave) = wave_from_flat_index(layer.wave_id);
            (3, cat, wave)
        }
        SynthOscMode::Analog => {
            let (_, wf, _) = map_osc_mode_and_wave(true, 0, layer.osc_category, layer.osc_wave);
            if layer.osc_type == 0 && wf == layer.waveform {
                (0, layer.osc_category & 0x07, layer.osc_wave & 0x3F)
            } else {
                let (cat, wave) = match layer.waveform {
                    VaWaveform::Sine => (0, 0),
                    VaWaveform::Triangle => (0, 1),
                    VaWaveform::Saw => (0, 2),
                    VaWaveform::Pulse => (0, 3),
                    VaWaveform::SuperSaw => (6, 0),
                };
                (0, cat, wave)
            }
        }
    }
}

fn read_signed_bits(r: &mut BitReader, pos: usize, bits: usize) -> Result<i32, CbinError> {
    r.set_bit_pos(pos)?;
    let raw = r.read_bits(bits)? as i32;
    let sign = 1i32 << (bits - 1);
    Ok(if raw & sign != 0 {
        raw - (1 << bits)
    } else {
        raw
    })
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
    let octave = read_signed_bits(r, layer_base + 62, 4)?.clamp(-2, 2) as i8;
    r.set_bit_pos(layer_base + 72)?;
    let vibrato_mode = r.read_bits(3)? as u8;
    r.set_bit_pos(layer_base + 77)?;
    let legato = r.read_bool()?;
    let mono = r.read_bool()?;
    let voice_priority = r.read_bits(2)? as u8;
    r.set_bit_pos(layer_base + 81)?;
    let glide_raw = r.read_bits(7)?;
    r.set_bit_pos(layer_base + 135)?;
    let arp_enabled = r.read_bool()?;
    let (arp_rate_raw, _) = read_morph(r, layer_base + 175, 7, 127)?;
    let arp_bpm = 30.0 + (arp_rate_raw as f32 / 127.0) * 270.0;
    let pattern = SynthPatternDecoded::read(r, layer_base)?;
    let arp = SynthArpDecoded::read(r, layer_base)?;
    r.set_bit_pos(layer_base + 306)?;
    let unison = r.read_bits(2)? as u8;
    r.set_bit_pos(layer_base + 372)?;
    let vib_delay_raw = r.read_bits(5)?;

    // 0x2F Sound block
    r.set_bit_pos(sound_base + 10)?;
    let osc_type = r.read_bits(2)? as u8;
    r.set_bit_pos(sound_base + 15)?;
    let osc_category = r.read_bits(3)? as u8;
    r.set_bit_pos(sound_base + 19)?;
    let osc_wave = r.read_bits(6)? as u8;
    let (osc_ctrl_raw, osc_ctrl_morph) = read_morph(r, sound_base + 25, 7, 127)?;
    let pitch_fine_cents = read_signed_bits(r, sound_base + 56, 7)? as i8;
    let pitch_coarse_semi = read_signed_bits(r, sound_base + 63, 6)? as i8;
    r.set_bit_pos(sound_base + 71)?;
    let oenv_atk = r.read_bits(7)? as usize;
    let oenv_dcy = r.read_bits(7)? as usize;
    let oenv_rel = r.read_bits(7)? as usize;
    let oenv_amt = read_signed_bits(r, sound_base + 92, 7)?;
    r.set_bit_pos(sound_base + 123)?;
    let osc_env_to_pitch = r.read_bool()?;
    let osc_env_velocity = r.read_bool()?;
    r.set_bit_pos(sound_base + 127)?;
    let lfo_target = r.read_bits(2)?;
    let lfo_shape_raw = r.read_bits(3)?;
    let lfo_mst_clk = r.read_bool()?;
    let (lfo_rate_raw, _) = read_morph(r, sound_base + 133, 7, 127)?;
    let (lfo_amt_raw, _) = read_morph(r, sound_base + 164, 7, 127)?;

    r.set_bit_pos(sound_base + 195)?;
    let atk_raw = r.read_bits(7)? as usize;
    let env_attack_ms = ENV_ATTACK_MS_TABLE[atk_raw.min(127)];
    let dcy_raw = r.read_bits(7)? as usize;
    let env_decay_ms = ENV_DECAY_MS_TABLE[dcy_raw.min(127)];
    let rel_raw = r.read_bits(7)? as usize;
    let env_release_ms = ENV_RELEASE_MS_TABLE[rel_raw.min(127)];
    r.set_bit_pos(sound_base + 216)?;
    let amp_velocity = r.read_bits(2)? as u8;

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
    r.set_bit_pos(sound_base + 339)?;
    let filter_velocity = r.read_bool()?;
    let filter_on = r.read_bool()?;
    let vib_rate_raw = r.read_bits(7)?;
    let vib_amt_raw = r.read_bits(6)?;

    let (osc_mode, waveform, wave_id) =
        map_osc_mode_and_wave(is_osc_mode, osc_type, osc_category, osc_wave);

    Ok(SynthLayerDecoded {
        enabled,
        level,
        level_morph,
        pan,
        octave,
        osc_mode,
        waveform,
        wave_id,
        osc_type,
        osc_category,
        osc_wave,
        sample_id,
        detune: (osc_ctrl_raw as f32 / 127.0) * 10.0,
        osc_ctrl_morph,
        pitch_fine_cents,
        pitch_coarse_semi,
        filter_model,
        filter_on,
        filter_cutoff_hz: FILTER_CUTOFF_HZ_TABLE[(flt_freq_raw as usize).min(127)],
        filter_cutoff_morph,
        filter_resonance: (flt_res_raw as f32 / 127.0) * 0.95,
        filter_resonance_morph,
        filter_drive,
        filter_env_amount: flt_env_raw as f32 / 127.0,
        kb_track,
        env_attack_ms,
        env_decay_ms,
        env_release_ms,
        amp_velocity,
        lfo_rate_hz: LFO_RATE_HZ_TABLE[(lfo_rate_raw as usize).min(127)],
        lfo_amount: lfo_amt_raw as f32 / 127.0,
        lfo_shape: match lfo_shape_raw {
            1 => crate::synth::LfoShape::SawDown,
            2 => crate::synth::LfoShape::Saw,
            3 => crate::synth::LfoShape::Square,
            4 => crate::synth::LfoShape::SampleAndHold,
            _ => crate::synth::LfoShape::Triangle,
        },
        lfo_dest: match lfo_target {
            1 => crate::synth::LfoDestination::OscCtrl,
            2 => crate::synth::LfoDestination::OscPitch,
            3 => crate::synth::LfoDestination::FilterCutoff,
            _ => crate::synth::LfoDestination::Off,
        },
        lfo_mst_clk,
        arp_enabled,
        arp_bpm,
        pattern,
        arp,
        filter_env_dedicated: true,
        filter_env_attack_ms: ENV_ATTACK_MS_TABLE[fenv_atk.min(127)],
        filter_env_decay_ms: ENV_DECAY_MS_TABLE[fenv_dcy.min(127)],
        filter_env_release_ms: ENV_RELEASE_MS_TABLE[fenv_rel.min(127)],
        knobs: Some(StripKnobs {
            amp: EnvKnobs {
                attack: atk_raw.min(127) as u8,
                decay: dcy_raw.min(127) as u8,
                release: rel_raw.min(127) as u8,
            },
            filter: EnvKnobs {
                attack: fenv_atk.min(127) as u8,
                decay: fenv_dcy.min(127) as u8,
                release: fenv_rel.min(127) as u8,
            },
        }),
        filter_velocity,
        osc_env_attack_ms: ENV_ATTACK_MS_TABLE[oenv_atk.min(127)],
        osc_env_decay_ms: ENV_DECAY_MS_TABLE[oenv_dcy.min(127)],
        osc_env_release_ms: ENV_RELEASE_MS_TABLE[oenv_rel.min(127)],
        // ns4decode: panel value = signed raw / 6, clamped to +/-10.
        osc_env_amount: (oenv_amt as f32 / 6.0).clamp(-10.0, 10.0) / 10.0,
        osc_env_to_pitch,
        osc_env_velocity,
        unison,
        mono,
        legato,
        voice_priority: voice_priority.min(2),
        glide: glide_raw as f32 * 10.0 / 127.0,
        vibrato_mode: vibrato_mode.min(5),
        vibrato_delay_sec: 0.1 + (3.0 - 0.1) * vib_delay_raw as f32 / 29.0,
        vibrato_rate_hz: 2.0 + (8.0 - 2.0) * vib_rate_raw as f32 / 127.0,
        vibrato_amount: 10.0 * vib_amt_raw as f32 / 50.0,
    })
}

fn decay_idx(ms: f32) -> u32 {
    if ms >= 44_000.0 || !ms.is_finite() {
        127
    } else {
        table_val_to_idx(&ENV_DECAY_MS_TABLE, ms).min(126)
    }
}

fn pack_synth_layer_pair(
    w: &mut BitWriter,
    layer_base: usize,
    sound_base: usize,
    layer: &SynthLayerDecoded,
) -> Result<(), CbinError> {
    let is_osc_mode = layer.osc_mode != SynthOscMode::Sample;
    let (osc_type, osc_cat, osc_wave) = canonical_osc_fields(layer);

    // 0x32 Layer block
    w.set_bit_pos(layer_base)?;
    w.write_bits(0x32, 8)?;
    w.write_bool(is_osc_mode)?;
    w.set_bit_pos(layer_base + 26)?;
    w.write_bits(layer.sample_id, 32)?;
    w.set_bit_pos(layer_base + 62)?;
    w.write_bits(encode_signed(i32::from(layer.octave.clamp(-2, 2)), 4), 4)?;
    w.set_bit_pos(layer_base + 72)?;
    w.write_bits(u32::from(layer.vibrato_mode.min(5)), 3)?;
    w.set_bit_pos(layer_base + 77)?;
    w.write_bool(layer.legato)?;
    w.write_bool(layer.mono)?;
    w.write_bits(u32::from(layer.voice_priority.min(2)), 2)?;
    w.set_bit_pos(layer_base + 81)?;
    w.write_bits(
        ((layer.glide / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
        7,
    )?;
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
    layer.pattern.write(w, layer_base)?;
    layer.arp.write(w, layer_base)?;
    w.set_bit_pos(layer_base + 306)?;
    w.write_bits(u32::from(layer.unison.min(3)), 2)?;
    w.set_bit_pos(layer_base + 372)?;
    let vdelay = (((layer.vibrato_delay_sec - 0.1) / 2.9).clamp(0.0, 1.0) * 29.0).round() as u32;
    w.write_bits(vdelay, 5)?;

    // 0x2F Sound block
    w.set_bit_pos(sound_base)?;
    w.write_bits(0x2F, 8)?;
    w.set_bit_pos(sound_base + 10)?;
    w.write_bits(u32::from(osc_type & 0x03), 2)?;
    w.set_bit_pos(sound_base + 15)?;
    w.write_bits(u32::from(osc_cat & 0x07), 3)?;
    w.set_bit_pos(sound_base + 19)?;
    w.write_bits(u32::from(osc_wave & 0x3F), 6)?;
    write_morph(
        w,
        sound_base + 25,
        ((layer.detune / 10.0).clamp(0.0, 1.0) * 127.0).round() as u32,
        layer.osc_ctrl_morph,
        7,
        127,
    )?;
    w.set_bit_pos(sound_base + 56)?;
    w.write_bits(
        encode_signed(i32::from(layer.pitch_fine_cents.clamp(-64, 63)), 7),
        7,
    )?;
    w.set_bit_pos(sound_base + 63)?;
    w.write_bits(
        encode_signed(i32::from(layer.pitch_coarse_semi.clamp(-32, 31)), 6),
        6,
    )?;
    w.set_bit_pos(sound_base + 71)?;
    w.write_bits(
        table_val_to_idx(&ENV_ATTACK_MS_TABLE, layer.osc_env_attack_ms),
        7,
    )?;
    w.write_bits(decay_idx(layer.osc_env_decay_ms), 7)?;
    w.write_bits(
        table_val_to_idx(&ENV_RELEASE_MS_TABLE, layer.osc_env_release_ms),
        7,
    )?;
    w.set_bit_pos(sound_base + 92)?;
    let oamt = (layer.osc_env_amount.clamp(-1.0, 1.0) * 10.0 * 6.0).round() as i32;
    w.write_bits(encode_signed(oamt.clamp(-64, 63), 7), 7)?;
    w.set_bit_pos(sound_base + 123)?;
    w.write_bool(layer.osc_env_to_pitch)?;
    w.write_bool(layer.osc_env_velocity)?;
    w.set_bit_pos(sound_base + 127)?;
    let target = match layer.lfo_dest {
        crate::synth::LfoDestination::Off => 0,
        crate::synth::LfoDestination::OscCtrl => 1,
        crate::synth::LfoDestination::OscPitch => 2,
        crate::synth::LfoDestination::FilterCutoff => 3,
    };
    w.write_bits(target, 2)?;
    let shape = match layer.lfo_shape {
        crate::synth::LfoShape::Triangle => 0,
        crate::synth::LfoShape::SawDown => 1,
        crate::synth::LfoShape::Saw => 2,
        crate::synth::LfoShape::Square => 3,
        crate::synth::LfoShape::SampleAndHold => 4,
    };
    w.write_bits(shape, 3)?;
    w.write_bool(layer.lfo_mst_clk)?;
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
    w.write_bits(decay_idx(layer.env_decay_ms), 7)?;
    w.write_bits(
        table_val_to_idx(&ENV_RELEASE_MS_TABLE, layer.env_release_ms),
        7,
    )?;
    w.set_bit_pos(sound_base + 216)?;
    w.write_bits(u32::from(layer.amp_velocity.min(3)), 2)?;

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
    let env_amt_raw = (layer.filter_env_amount.clamp(0.0, 1.0) * 127.0).round() as u32;
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
    w.write_bits(decay_idx(layer.filter_env_decay_ms), 7)?;
    w.write_bits(
        table_val_to_idx(&ENV_RELEASE_MS_TABLE, layer.filter_env_release_ms),
        7,
    )?;
    w.set_bit_pos(sound_base + 339)?;
    w.write_bool(layer.filter_velocity)?;
    w.write_bool(layer.filter_on)?;
    let vrate = (((layer.vibrato_rate_hz - 2.0) / 6.0).clamp(0.0, 1.0) * 127.0).round() as u32;
    w.write_bits(vrate, 7)?;
    let vamt = ((layer.vibrato_amount / 10.0).clamp(0.0, 1.0) * 50.0).round() as u32;
    w.write_bits(vamt, 6)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::presets::*;

    /// Writes `val` (`bits` wide) at the absolute bit `pos` of a synth payload.
    fn put(buf: &mut [u8], pos: usize, val: u32, bits: usize) {
        let mut w = BitWriter::new(buf);
        w.set_bit_pos(pos).unwrap();
        w.write_bits(val, bits).unwrap();
    }

    fn blank_payload() -> Vec<u8> {
        SynthPresetDecoded::default().pack().unwrap()
    }

    const LAYER_A: usize = 248;
    const SOUND_A: usize = 1472;

    #[test]
    fn test_layer_0x32_fields_decode_from_ns4decode_offsets() {
        let mut buf = blank_payload();
        put(&mut buf, LAYER_A + 62, 0b1110, 4); // octave -2
        put(&mut buf, LAYER_A + 72, 2, 3); // vibrato mode DLY
        put(&mut buf, LAYER_A + 77, 1, 1); // legato
        put(&mut buf, LAYER_A + 78, 0, 1); // mono off
        put(&mut buf, LAYER_A + 79, 2, 2); // voice priority HI
        put(&mut buf, LAYER_A + 81, 127, 7); // glide max
        put(&mut buf, LAYER_A + 306, 3, 2); // unison 3
        put(&mut buf, LAYER_A + 372, 29, 5); // vibrato delay max
        let l = SynthPresetDecoded::unpack(&buf).unwrap().layer_a;
        assert_eq!(l.octave, -2);
        assert_eq!(l.vibrato_mode, 2);
        assert!(l.legato && !l.mono);
        assert_eq!(l.voice_priority, 2);
        assert!((l.glide - 10.0).abs() < 1e-4);
        assert_eq!(l.unison, 3);
        assert!((l.vibrato_delay_sec - 3.0).abs() < 1e-4);
    }

    #[test]
    fn test_sound_0x2f_fields_decode_from_ns4decode_offsets() {
        let mut buf = blank_payload();
        put(&mut buf, SOUND_A + 10, 3, 2); // WAVE
        put(&mut buf, SOUND_A + 15, 2, 3); // DIGITAL
        put(&mut buf, SOUND_A + 19, 5, 6);
        put(&mut buf, SOUND_A + 56, 0b1111000, 7); // -8 cents
        put(&mut buf, SOUND_A + 63, 0b111101, 6); // -3 semi
        put(&mut buf, SOUND_A + 71, 5, 7);
        put(&mut buf, SOUND_A + 78, 127, 7);
        put(&mut buf, SOUND_A + 85, 4, 7);
        put(&mut buf, SOUND_A + 92, 0b1100110, 7); // -26 / 6 = -4.3
        put(&mut buf, SOUND_A + 123, 1, 1);
        put(&mut buf, SOUND_A + 124, 1, 1);
        put(&mut buf, SOUND_A + 127, 3, 2); // FILTER
        put(&mut buf, SOUND_A + 129, 4, 3); // S/H
        put(&mut buf, SOUND_A + 132, 1, 1);
        put(&mut buf, SOUND_A + 216, 2, 2);
        put(&mut buf, SOUND_A + 339, 1, 1);
        put(&mut buf, SOUND_A + 340, 0, 1);
        put(&mut buf, SOUND_A + 341, 127, 7);
        put(&mut buf, SOUND_A + 348, 50, 6);
        let l = SynthPresetDecoded::unpack(&buf).unwrap().layer_a;
        assert_eq!((l.osc_type, l.osc_category, l.osc_wave), (3, 2, 5));
        assert_eq!(l.osc_mode, SynthOscMode::Wavetable);
        assert_eq!(l.wave_id, 17);
        assert_eq!((l.pitch_fine_cents, l.pitch_coarse_semi), (-8, -3));
        assert_eq!(l.osc_env_attack_ms, ENV_ATTACK_MS_TABLE[5]);
        assert_eq!(l.osc_env_decay_ms, ENV_DECAY_MS_TABLE[127]);
        assert_eq!(l.osc_env_release_ms, ENV_RELEASE_MS_TABLE[4]);
        assert!((l.osc_env_amount + 26.0 / 60.0).abs() < 1e-4);
        assert!(l.osc_env_to_pitch && l.osc_env_velocity);
        assert_eq!(l.lfo_dest, crate::synth::LfoDestination::FilterCutoff);
        assert_eq!(l.lfo_shape, crate::synth::LfoShape::SampleAndHold);
        assert!(l.lfo_mst_clk);
        assert_eq!(l.amp_velocity, 2);
        assert!(l.filter_velocity && !l.filter_on);
        assert!((l.vibrato_rate_hz - 8.0).abs() < 1e-4);
        assert!((l.vibrato_amount - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_lfo_target_and_shape_enums_cover_all_raw_values() {
        use crate::synth::{LfoDestination as D, LfoShape as S};
        let targets = [D::Off, D::OscCtrl, D::OscPitch, D::FilterCutoff];
        for (raw, want) in targets.into_iter().enumerate() {
            let mut buf = blank_payload();
            put(&mut buf, SOUND_A + 127, raw as u32, 2);
            let got = SynthPresetDecoded::unpack(&buf).unwrap().layer_a.lfo_dest;
            assert_eq!(got, want);
        }
        let shapes = [S::Triangle, S::SawDown, S::Saw, S::Square, S::SampleAndHold];
        for (raw, want) in shapes.into_iter().enumerate() {
            let mut buf = blank_payload();
            put(&mut buf, SOUND_A + 129, raw as u32, 3);
            let got = SynthPresetDecoded::unpack(&buf).unwrap().layer_a.lfo_shape;
            assert_eq!(got, want);
        }
    }

    #[test]
    fn test_all_new_synth_fields_pack_unpack_roundtrip() {
        let p = SynthPresetDecoded {
            layer_b: SynthLayerDecoded {
                enabled: true,
                octave: 1,
                osc_mode: SynthOscMode::Fm,
                osc_type: 2,
                osc_category: 3,
                wave_id: 20,
                osc_wave: 20,
                pitch_fine_cents: -8,
                pitch_coarse_semi: 5,
                filter_on: false,
                filter_env_amount: 0.5,
                amp_velocity: 1,
                lfo_shape: crate::synth::LfoShape::Square,
                lfo_dest: crate::synth::LfoDestination::OscCtrl,
                lfo_mst_clk: true,
                filter_env_dedicated: true,
                filter_velocity: true,
                osc_env_amount: -0.4,
                osc_env_to_pitch: true,
                osc_env_velocity: true,
                unison: 2,
                mono: true,
                legato: true,
                voice_priority: 1,
                glide: 5.0,
                vibrato_mode: 4,
                vibrato_amount: 6.0,
                ..SynthLayerDecoded::default()
            },
            ..SynthPresetDecoded::default()
        };
        let packed = p.pack().unwrap();
        let un = SynthPresetDecoded::unpack(&packed).unwrap();
        let (a, b) = (&p.layer_b, &un.layer_b);
        assert_eq!(b.octave, a.octave);
        assert_eq!(b.osc_mode, SynthOscMode::Fm);
        assert_eq!((b.osc_type, b.osc_category, b.wave_id), (2, 3, 20));
        assert_eq!(b.pitch_fine_cents, a.pitch_fine_cents);
        assert_eq!(b.pitch_coarse_semi, a.pitch_coarse_semi);
        assert!(!b.filter_on);
        assert!((b.filter_env_amount - 0.5).abs() < 0.01);
        assert_eq!(b.amp_velocity, 1);
        assert_eq!(b.lfo_shape, a.lfo_shape);
        assert_eq!(b.lfo_dest, a.lfo_dest);
        assert!(b.lfo_mst_clk && b.filter_velocity);
        assert!((b.osc_env_amount - a.osc_env_amount).abs() < 0.02);
        assert!(b.osc_env_to_pitch && b.osc_env_velocity);
        assert_eq!((b.unison, b.voice_priority), (2, 1));
        assert!(b.mono && b.legato);
        assert!((b.glide - 5.0).abs() < 0.05);
        assert_eq!(b.vibrato_mode, 4);
        assert!((b.vibrato_amount - 6.0).abs() < 0.1);
        assert_eq!(un.pack().unwrap(), packed, "re-pack must be bit-identical");
    }

    #[test]
    fn test_pack_canonicalises_osc_fields_from_waveform_and_wave_id() {
        let mut p = SynthPresetDecoded::default();
        p.layer_a.osc_mode = SynthOscMode::Analog;
        p.layer_a.waveform = VaWaveform::SuperSaw;
        p.layer_b.osc_mode = SynthOscMode::Wavetable;
        p.layer_b.wave_id = 20;
        p.layer_c.osc_mode = SynthOscMode::Fm;
        p.layer_c.wave_id = 70;
        let un = SynthPresetDecoded::unpack(&p.pack().unwrap()).unwrap();
        assert_eq!(un.layer_a.waveform, VaWaveform::SuperSaw);
        assert_eq!(un.layer_a.osc_category, 6);
        assert_eq!(un.layer_b.osc_mode, SynthOscMode::Wavetable);
        assert_eq!(un.layer_b.wave_id, 20);
        assert_eq!(un.layer_c.osc_mode, SynthOscMode::Fm);
        assert_eq!(un.layer_c.wave_id, 70 & 0x3F);
    }

    #[test]
    fn test_unpack_rejects_short_payload() {
        assert!(SynthPresetDecoded::unpack(&[0u8; 10]).is_err());
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
}
