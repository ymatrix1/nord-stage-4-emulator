//! Decoded FX chain and FX morph application for `Stage4Engine::apply_program`.

use super::*;

impl Stage4Engine {
    pub(super) fn apply_fx_chain(fx: &mut LayerFxChain, dec: &crate::cbin::FxDecoded) {
        fx.mod1_type = dec.mod1_type();
        fx.mod1_rate_hz = 0.1 + dec.mod1_rate.clamp(0.0, 1.0) * 14.9;
        fx.mod1_amount = if dec.mod1_enabled {
            dec.mod1_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.mod2_type = dec.mod2_type();
        fx.mod2_rate_hz = 0.1 + dec.mod2_rate.clamp(0.0, 1.0) * 9.9;
        fx.mod2_amount = if dec.mod2_enabled {
            dec.mod2_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if dec.amp_enabled {
            let mode = dec.amp_mode();
            fx.amp_cabinet = match mode {
                1 | 2 => AmpCabinet::Twin,
                3 | 4 => AmpCabinet::Jc,
                5 | 6 => AmpCabinet::Small,
                7 => AmpCabinet::ToRotary,
                8 => AmpCabinet::LpFilter,
                9 => AmpCabinet::HpFilter,
                _ => AmpCabinet::Off,
            };
            fx.amp_variation = matches!(mode, 2 | 4 | 6);
            fx.eq_filter_res = dec.amp_filter_res();
            fx.drive_amount = (dec.amp_drive / 10.0).clamp(0.0, 1.0);
            fx.eq_bass_db = dec.eq_bass_db;
            fx.eq_mid_db = dec.eq_mid_db;
            fx.eq_mid_freq_hz = dec.eq_mid_freq_hz();
            fx.eq_treble_db = dec.eq_treble_db;
        } else {
            fx.amp_cabinet = AmpCabinet::Off;
            fx.drive_amount = 0.0;
            fx.eq_bass_db = 0.0;
            fx.eq_mid_db = 0.0;
            fx.eq_treble_db = 0.0;
        }
        fx.comp_enabled = dec.comp_enabled;
        fx.comp_amount = if dec.comp_enabled {
            dec.comp_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.comp_fast = dec.comp_fast;
        fx.delay_enabled = dec.delay_enabled;
        fx.delay_time_ms = dec.delay_time_ms();
        fx.delay_clock_sync = dec.delay_clock_sync;
        fx.delay_sync_beats = dec.delay_clock_sync.then(|| dec.delay_sync_beats());
        fx.delay_analog = dec.delay_analog_on();
        fx.delay_ping_pong = dec.delay_ping_pong_on();
        fx.delay_filter = match dec.delay_filter_type() {
            1 => DelayFilterMode::Lp,
            2 => DelayFilterMode::Hp,
            3 => DelayFilterMode::Bp,
            _ => DelayFilterMode::Off,
        };
        // `Delay Feedback Effect`: 0/1 none, 2/3 ENS, 4/5 VIBE, 6/7 CHOR, 8/9 FLAM, 10/11 SPACE (odd = variation).
        fx.delay_fx = match dec.delay_fb_fx_type {
            2 | 3 => crate::fx::DelayFeedbackFx::Ensemble,
            4 | 5 => crate::fx::DelayFeedbackFx::Vibe,
            6 | 7 => crate::fx::DelayFeedbackFx::Chorus,
            8 | 9 => crate::fx::DelayFeedbackFx::Flam,
            10 | 11 => crate::fx::DelayFeedbackFx::Space,
            _ => crate::fx::DelayFeedbackFx::Off,
        };
        fx.delay_fx_variation = dec.delay_fb_fx_type >= 2 && (dec.delay_fb_fx_type & 1) == 1;
        fx.delay_feedback = dec.delay_feedback.clamp(0.0, 0.95);
        fx.delay_mix = if dec.delay_enabled {
            dec.delay_mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
        fx.reverb_model = dec.reverb_model();
        // Reverb Color: 0 none, 1 DARK, 2 BRIGHT (EMPIRICAL, `ns4decode`; the string `Dark` is in the
        // firmware at 0x2cc938). The variation of Stage/Hall/Cathedral is `Chorale` (VERIFIED names).
        fx.reverb_color = match dec.reverb_color {
            1 => ReverbColor::Dark,
            2 => ReverbColor::Bright,
            _ if dec.reverb_variation() && dec.reverb_type4() >= 6 => ReverbColor::Chorale,
            _ => ReverbColor::Normal,
        };
        fx.reverb_mix = if dec.reverb_enabled {
            dec.reverb_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self::apply_fx_morph(fx, dec);
    }

    /// Loads `Wheel` / `A.T.` / `CtrlPed` morph bases and deltas; disabled slots stay un-morphed.
    fn apply_fx_morph(fx: &mut LayerFxChain, dec: &crate::cbin::FxDecoded) {
        fn set(
            slot: &mut crate::morph::MorphParameter,
            on: bool,
            base: f32,
            d: crate::cbin::MorphDeltas,
        ) {
            *slot = crate::morph::MorphParameter::new(base.clamp(0.0, 1.0));
            if on {
                slot.delta_wheel = d.wheel;
                slot.delta_aftertouch = d.aftertouch;
                slot.delta_pedal = d.ctrl_pedal;
            }
        }
        let m = &mut fx.morph;
        set(
            &mut m.mod1_rate,
            dec.mod1_enabled,
            dec.mod1_rate,
            dec.mod1_rate_morph,
        );
        set(
            &mut m.mod1_amount,
            dec.mod1_enabled,
            dec.mod1_amount,
            dec.mod1_amount_morph,
        );
        set(
            &mut m.mod2_rate,
            dec.mod2_enabled,
            dec.mod2_rate,
            dec.mod2_rate_morph,
        );
        set(
            &mut m.mod2_amount,
            dec.mod2_enabled,
            dec.mod2_amount,
            dec.mod2_amount_morph,
        );
        set(
            &mut m.eq_mid_freq,
            dec.amp_enabled,
            dec.eq_mid_freq_norm,
            dec.eq_mid_freq_morph,
        );
        set(
            &mut m.amp_drive,
            dec.amp_enabled,
            dec.amp_drive / 10.0,
            dec.amp_drive_morph,
        );
        set(
            &mut m.delay_mix,
            dec.delay_enabled,
            dec.delay_mix,
            dec.delay_mix_morph,
        );
        set(
            &mut m.delay_feedback,
            dec.delay_enabled,
            dec.delay_feedback,
            dec.delay_feedback_morph,
        );
        set(
            &mut m.reverb_amount,
            dec.reverb_enabled,
            dec.reverb_amount,
            dec.reverb_amount_morph,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_decoded_fx_applies_without_panicking() {
        let mut fx = LayerFxChain::new(48_000.0);
        let dec = crate::cbin::FxDecoded::default();
        Stage4Engine::apply_fx_chain(&mut fx, &dec);
    }
}
