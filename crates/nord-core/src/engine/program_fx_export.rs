//! Inverse of `Stage4Engine::apply_fx_chain`: the panel parameters of a layer FX chain
//! (`FxParams`) back into the decoded `0x36` block (`FxDecoded`), for STORE of a program or a
//! preset (OQ30).
//!
//! The decoded block is the *base*. Where the engine value is many to one (an effect that is
//! "on" with a zero amount, the odd / even variation bit of a type, a delay time index, the Amp
//! cabinet mode) a base value that still projects onto the engine value is kept, so an unedited
//! chain returns the base and only edits change bits.
//!
//! Left with the base, never written (`LOSSY_FX_FIELDS`): the clock-sync flags of Mod 1, the
//! morph bases (the engine keeps the base of each morph in the parameter itself), the amp filter
//! resonance (carried by the Mid dB field), the Mod 1 types the file cannot name (Auto Wah, Pump)
//! and the unknown raw values 3 of the reverb color.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.7. Control thread only.

use super::fx_params::FxParams;
use crate::cbin::{FxDecoded, MorphDeltas};
use crate::fx::{
    delay_sync_beats_from_index, delay_time_ms_from_index, eq_freq_hz_from_norm, AmpCabinet,
    DelayFeedbackFx, DelayFilterMode, Mod1Type, Mod2Type, ReverbColor, ReverbModel, EQ_FREQ_HZ,
};
use crate::morph::MorphParameter;

/// Decoded FX fields the engine cannot return (see the module docs).
pub const LOSSY_FX_FIELDS: &[&str] = &[
    "mod1_clock_sync",
    "morph bases",
    "amp filter resonance (eq_mid_db of LP / HP)",
    "mod1 Auto Wah / Pump",
    "reverb_color raw 3",
];

fn deltas(m: &MorphParameter) -> MorphDeltas {
    MorphDeltas {
        wheel: m.delta_wheel,
        aftertouch: m.delta_aftertouch,
        ctrl_pedal: m.delta_pedal,
    }
}

/// Writes `inverse` into `field` unless the base value already projects onto `engine`.
fn keep(field: &mut f32, projected: f32, engine: f32, inverse: f32) {
    if projected != engine {
        *field = inverse;
    }
}

fn morph(slot: &MorphParameter, enabled: bool, out: &mut MorphDeltas) {
    if enabled {
        *out = deltas(slot);
    }
}

fn invert_mods(fx: &FxParams, dec: &mut FxDecoded) {
    if fx.mod1_type == Mod1Type::Off {
        dec.mod1_enabled = false;
    } else {
        let canon = match fx.mod1_type {
            Mod1Type::AutoPan => Some(0),
            Mod1Type::Tremolo => Some(2),
            Mod1Type::RingMod => Some(4),
            Mod1Type::Wah => Some(6),
            _ => None,
        };
        if !(dec.mod1_enabled && dec.mod1_type() == fx.mod1_type) {
            if let Some(raw) = canon {
                dec.mod1_type_raw = raw | (dec.mod1_type_raw & 1);
                dec.mod1_enabled = true;
            }
        }
    }
    let rate = 0.1 + dec.mod1_rate.clamp(0.0, 1.0) * 14.9;
    keep(
        &mut dec.mod1_rate,
        rate,
        fx.mod1_rate_hz,
        (fx.mod1_rate_hz - 0.1) / 14.9,
    );
    if dec.mod1_enabled {
        dec.mod1_amount = fx.mod1_amount;
    }

    let on2 = fx.mod2_type != Mod2Type::Off;
    if on2 && !(dec.mod2_enabled && dec.mod2_type() == fx.mod2_type) {
        let canon = match fx.mod2_type {
            Mod2Type::Phaser => 0,
            Mod2Type::Flanger => 2,
            Mod2Type::Chorus => 4,
            Mod2Type::Vibe => 6,
            Mod2Type::Ensemble => 8,
            _ => 10,
        };
        dec.mod2_type_raw = canon | (dec.mod2_type_raw & 1);
    }
    dec.mod2_enabled = on2;
    let rate = 0.1 + dec.mod2_rate.clamp(0.0, 1.0) * 9.9;
    keep(
        &mut dec.mod2_rate,
        rate,
        fx.mod2_rate_hz,
        (fx.mod2_rate_hz - 0.1) / 9.9,
    );
    if on2 {
        dec.mod2_amount = fx.mod2_amount;
    }
}

fn cabinet_of(mode: u8) -> AmpCabinet {
    match mode {
        1 | 2 => AmpCabinet::Twin,
        3 | 4 => AmpCabinet::Jc,
        5 | 6 => AmpCabinet::Small,
        7 => AmpCabinet::ToRotary,
        8 => AmpCabinet::LpFilter,
        9 => AmpCabinet::HpFilter,
        _ => AmpCabinet::Off,
    }
}

fn invert_amp(fx: &FxParams, dec: &mut FxDecoded) {
    let projected = if dec.amp_enabled {
        cabinet_of(dec.amp_mode())
    } else {
        AmpCabinet::Off
    };
    let variation_ok = fx.amp_variation == matches!(dec.amp_mode(), 2 | 4 | 6);
    if projected != fx.amp_cabinet || (fx.amp_cabinet != AmpCabinet::Off && !variation_ok) {
        let base = match fx.amp_cabinet {
            AmpCabinet::Off => 0,
            AmpCabinet::Twin => 1,
            AmpCabinet::Jc => 3,
            AmpCabinet::Small => 5,
            AmpCabinet::ToRotary => 7,
            AmpCabinet::LpFilter => 8,
            AmpCabinet::HpFilter => 9,
        };
        let variation = u8::from(fx.amp_variation && base != 0 && base < 7);
        let mode = base + variation;
        dec.amp_enabled = fx.amp_cabinet != AmpCabinet::Off;
        if dec.amp_enabled {
            dec.amp_type_new = (dec.amp_type_new & !0x0F) | u16::from(mode);
        }
    }
    if !dec.amp_enabled {
        return;
    }
    let drive = (dec.amp_drive / 10.0).clamp(0.0, 1.0);
    keep(
        &mut dec.amp_drive,
        drive,
        fx.drive_amount,
        fx.drive_amount * 10.0,
    );
    dec.eq_bass_db = fx.eq_bass_db;
    dec.eq_mid_db = fx.eq_mid_db;
    dec.eq_treble_db = fx.eq_treble_db;
    if eq_freq_hz_from_norm(dec.eq_mid_freq_norm) != fx.eq_mid_freq_hz {
        let nearest = (0..EQ_FREQ_HZ.len())
            .min_by(|&a, &b| {
                let d = |i: usize| (EQ_FREQ_HZ[i] / fx.eq_mid_freq_hz).ln().abs();
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(0);
        dec.eq_mid_freq_norm = nearest as f32 / 127.0;
    }
}

/// The delay time index `0..=127` whose table value is closest to `target` (log distance).
fn nearest_index(target: f32, value: impl Fn(usize) -> f32) -> usize {
    let d = |i: usize| (value(i) / target).ln().abs();
    (0..128).min_by(|&a, &b| d(a).total_cmp(&d(b))).unwrap_or(0)
}

fn invert_delay(fx: &FxParams, dec: &mut FxDecoded) {
    dec.delay_enabled = fx.delay_enabled;
    dec.delay_clock_sync = fx.delay_clock_sync;
    let (now, target, value): (f32, f32, fn(usize) -> f32) = match fx.delay_sync_beats {
        Some(beats) if fx.delay_clock_sync => {
            (dec.delay_sync_beats(), beats, delay_sync_beats_from_index)
        }
        _ => (
            dec.delay_time_ms(),
            fx.delay_time_ms,
            delay_time_ms_from_index,
        ),
    };
    if now != target {
        let idx = nearest_index(target, value);
        dec.delay_rate_norm = (idx << 7) as f32 / 16383.0;
    }
    if fx.delay_enabled {
        dec.delay_mix = fx.delay_mix;
    }
    let bits = (u8::from(fx.delay_analog) << 1) | u8::from(fx.delay_ping_pong);
    dec.delay_mode = (dec.delay_mode & !0b11) | bits;
    let filter = match fx.delay_filter {
        DelayFilterMode::Off => 0,
        DelayFilterMode::Lp => 1,
        DelayFilterMode::Hp => 2,
        DelayFilterMode::Bp => 3,
    };
    dec.delay_ping_pong = filter & 2 != 0;
    dec.delay_fb_filter = filter & 1 != 0;
    let fb_fx = match fx.delay_fx {
        DelayFeedbackFx::Off => 0,
        DelayFeedbackFx::Ensemble => 2,
        DelayFeedbackFx::Vibe => 4,
        DelayFeedbackFx::Chorus => 6,
        DelayFeedbackFx::Flam => 8,
        DelayFeedbackFx::Space => 10,
    };
    let current = super::program_fx::feedback_fx_of(dec.delay_fb_fx_type);
    if current != fx.delay_fx
        || (fx.delay_fx != DelayFeedbackFx::Off
            && fx.delay_fx_variation != (dec.delay_fb_fx_type & 1 == 1))
    {
        dec.delay_fb_fx_type = fb_fx + u8::from(fx.delay_fx_variation && fb_fx != 0);
    }
    let projected = dec.delay_feedback.clamp(0.0, 0.95);
    keep(
        &mut dec.delay_feedback,
        projected,
        fx.delay_feedback,
        fx.delay_feedback,
    );
}

fn invert_reverb(fx: &FxParams, dec: &mut FxDecoded) {
    let index = match fx.reverb_model {
        ReverbModel::Off => None,
        ReverbModel::Spring => Some(0),
        ReverbModel::Booth => Some(1),
        ReverbModel::Room => Some(2),
        ReverbModel::Stage => Some(3),
        ReverbModel::Hall => Some(4),
        ReverbModel::Cathedral => Some(5),
    };
    let Some(index) = index else {
        dec.reverb_enabled = false;
        return;
    };
    dec.reverb_enabled = true;
    let type4 = (index << 1) | u8::from(fx.reverb_variation);
    // Raw types 6 and 7 play as Cathedral: keep them while the model and variation still match.
    let same =
        (dec.reverb_type4() >> 1).min(5) == index && dec.reverb_variation() == fx.reverb_variation;
    if !same {
        dec.reverb_type_raw = type4 >> 1;
        dec.amp_type_new = (dec.amp_type_new & !0x100) | (u16::from(type4 & 1) << 8);
    }
    match fx.reverb_color {
        ReverbColor::Dark => dec.reverb_color = 1,
        ReverbColor::Bright => dec.reverb_color = 2,
        ReverbColor::Normal | ReverbColor::Chorale => {
            if matches!(dec.reverb_color, 1 | 2) {
                dec.reverb_color = 0;
            }
        }
    }
    dec.reverb_amount = fx.reverb_mix;
}

/// Writes the panel parameters `fx` of one chain into the decoded block `dec` (the base).
pub fn invert_fx_chain(fx: &FxParams, dec: &mut FxDecoded) {
    invert_mods(fx, dec);
    invert_amp(fx, dec);
    dec.comp_enabled = fx.comp_enabled;
    dec.comp_fast = fx.comp_fast;
    if fx.comp_enabled {
        dec.comp_amount = fx.comp_amount;
    }
    invert_delay(fx, dec);
    invert_reverb(fx, dec);
    let m = &fx.morph;
    morph(&m.mod1_rate, dec.mod1_enabled, &mut dec.mod1_rate_morph);
    morph(&m.mod1_amount, dec.mod1_enabled, &mut dec.mod1_amount_morph);
    morph(&m.mod2_rate, dec.mod2_enabled, &mut dec.mod2_rate_morph);
    morph(&m.mod2_amount, dec.mod2_enabled, &mut dec.mod2_amount_morph);
    morph(&m.eq_mid_freq, dec.amp_enabled, &mut dec.eq_mid_freq_morph);
    morph(&m.amp_drive, dec.amp_enabled, &mut dec.amp_drive_morph);
    morph(&m.delay_mix, dec.delay_enabled, &mut dec.delay_mix_morph);
    morph(
        &m.delay_feedback,
        dec.delay_enabled,
        &mut dec.delay_feedback_morph,
    );
    morph(
        &m.reverb_amount,
        dec.reverb_enabled,
        &mut dec.reverb_amount_morph,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Stage4Engine;
    use crate::fx::LayerFxChain;

    /// Deterministic xorshift `0x36` payloads (every bit pattern the decoder accepts).
    fn random_fx(seed: u64) -> FxDecoded {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let bytes: Vec<u8> = (0..FxDecoded::PAYLOAD_LEN)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        FxDecoded::unpack(&bytes).unwrap()
    }

    fn params(dec: &FxDecoded, chain: &mut LayerFxChain) -> FxParams {
        Stage4Engine::apply_fx_chain(chain, dec);
        FxParams::capture(chain)
    }

    #[test]
    fn the_inverse_of_apply_returns_the_same_bits_for_random_chains() {
        let mut chain = LayerFxChain::new(48_000.0);
        for seed in 1..=6000 {
            let dec = random_fx(seed);
            let fx = params(&dec, &mut chain);
            let mut back = dec.clone();
            invert_fx_chain(&fx, &mut back);
            assert_eq!(back.pack().unwrap(), dec.pack().unwrap(), "seed {seed}");
        }
    }

    #[test]
    fn panel_edits_survive_pack_unpack_and_apply() {
        let mut chain = LayerFxChain::new(48_000.0);
        let base = FxDecoded::default();
        let mut fx = params(&base, &mut chain);
        fx.reverb_model = ReverbModel::Hall;
        fx.reverb_mix = 0.5;
        fx.delay_enabled = true;
        fx.delay_mix = 0.4;
        fx.delay_time_ms = delay_time_ms_from_index(40);
        fx.mod2_type = Mod2Type::Flanger;
        fx.mod2_amount = 0.6;
        fx.comp_enabled = true;
        fx.comp_amount = 0.3;
        fx.amp_cabinet = AmpCabinet::Jc;
        fx.amp_variation = true;
        fx.drive_amount = 0.5;
        fx.morph.reverb_amount.delta_wheel = 0.25;
        let mut dec = base.clone();
        invert_fx_chain(&fx, &mut dec);
        let back = FxDecoded::unpack(&dec.pack().unwrap()).unwrap();
        let r = params(&back, &mut chain);
        assert_eq!(r.reverb_model, ReverbModel::Hall);
        assert!((r.reverb_mix - 0.5).abs() < 0.01);
        assert!(r.delay_enabled && (r.delay_mix - 0.4).abs() < 0.01);
        assert_eq!(r.delay_time_ms, delay_time_ms_from_index(40));
        assert_eq!(r.mod2_type, Mod2Type::Flanger);
        assert!(r.comp_enabled && (r.comp_amount - 0.3).abs() < 0.01);
        assert_eq!((r.amp_cabinet, r.amp_variation), (AmpCabinet::Jc, true));
        assert!((r.drive_amount - 0.5).abs() < 0.01);
        assert!((r.morph.reverb_amount.delta_wheel - 0.25).abs() < 0.02);
    }

    #[test]
    fn switching_effects_off_keeps_the_knob_positions() {
        let mut chain = LayerFxChain::new(48_000.0);
        let mut base = FxDecoded::default();
        base.reverb_enabled = true;
        base.reverb_amount = 0.5;
        base.comp_enabled = true;
        base.comp_amount = 0.5;
        let mut fx = params(&base, &mut chain);
        fx.reverb_model = ReverbModel::Off;
        fx.comp_enabled = false;
        let mut dec = base.clone();
        invert_fx_chain(&fx, &mut dec);
        assert!(!dec.reverb_enabled && !dec.comp_enabled);
        assert_eq!((dec.reverb_amount, dec.comp_amount), (0.5, 0.5));
        assert!(LOSSY_FX_FIELDS.contains(&"mod1_clock_sync"));
    }
}
