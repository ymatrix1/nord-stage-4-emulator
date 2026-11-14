//! Continuous and on/off controls of a layer FX chain addressed by `FxEdit`.

use nord_core::fx::LayerFxChain;

/// Continuous controls of a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxFloat {
    Mod1Rate,
    Mod1Amount,
    Mod1PedPos,
    Mod2Rate,
    Mod2Amount,
    Drive,
    EqBass,
    EqMid,
    EqMidFreq,
    EqTreble,
    FilterRes,
    CompAmount,
    DelayTime,
    DelayFeedback,
    DelayMix,
    ReverbMix,
}

impl FxFloat {
    /// Every variant (used by the tests).
    pub const ALL: [FxFloat; 16] = [
        FxFloat::Mod1Rate,
        FxFloat::Mod1Amount,
        FxFloat::Mod1PedPos,
        FxFloat::Mod2Rate,
        FxFloat::Mod2Amount,
        FxFloat::Drive,
        FxFloat::EqBass,
        FxFloat::EqMid,
        FxFloat::EqMidFreq,
        FxFloat::EqTreble,
        FxFloat::FilterRes,
        FxFloat::CompAmount,
        FxFloat::DelayTime,
        FxFloat::DelayFeedback,
        FxFloat::DelayMix,
        FxFloat::ReverbMix,
    ];

    /// Clamped write into `fx`.
    ///
    /// ASSUMED: rate ranges follow the plugin (`fx_selectors.rs`: 0.1..20 Hz Mod 1, 0.1..10 Hz Mod 2);
    /// EQ +-16 dB and Mid 200..8000 Hz follow `fx_params.rs` / `fx_extra_params.rs` (VERIFIED there);
    /// delay feedback uses the engine's own `0..0.95` stability bound.
    pub(super) fn set(self, fx: &mut LayerFxChain, v: f32) {
        match self {
            Self::Mod1Rate => fx.mod1_rate_hz = v.clamp(0.1, 20.0),
            Self::Mod1Amount => fx.mod1_amount = v.clamp(0.0, 1.0),
            Self::Mod1PedPos => fx.mod1_ped_pos = v.clamp(0.0, 1.0),
            Self::Mod2Rate => fx.mod2_rate_hz = v.clamp(0.1, 10.0),
            Self::Mod2Amount => fx.mod2_amount = v.clamp(0.0, 1.0),
            Self::Drive => fx.drive_amount = v.clamp(0.0, 1.0),
            Self::EqBass => fx.eq_bass_db = v.clamp(-16.0, 16.0),
            Self::EqMid => fx.eq_mid_db = v.clamp(-16.0, 16.0),
            Self::EqMidFreq => fx.eq_mid_freq_hz = v.clamp(200.0, 8000.0),
            Self::EqTreble => fx.eq_treble_db = v.clamp(-16.0, 16.0),
            Self::FilterRes => fx.eq_filter_res = v.clamp(0.0, 10.0),
            Self::CompAmount => fx.comp_amount = v.clamp(0.0, 1.0),
            Self::DelayTime => fx.delay_time_ms = v.clamp(20.0, 1500.0),
            Self::DelayFeedback => fx.delay_feedback = v.clamp(0.0, 0.95),
            Self::DelayMix => fx.delay_mix = v.clamp(0.0, 1.0),
            Self::ReverbMix => fx.reverb_mix = v.clamp(0.0, 1.0),
        }
    }
}

/// On/off controls of a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxFlag {
    Mod1Variation,
    Mod1Ped,
    Mod2Variation,
    AmpVariation,
    CompEnabled,
    CompFast,
    DelayEnabled,
    DelayAnalog,
    DelayPingPong,
    DelayFxVariation,
}

impl FxFlag {
    /// Every variant (used by the tests).
    pub const ALL: [FxFlag; 10] = [
        FxFlag::Mod1Variation,
        FxFlag::Mod1Ped,
        FxFlag::Mod2Variation,
        FxFlag::AmpVariation,
        FxFlag::CompEnabled,
        FxFlag::CompFast,
        FxFlag::DelayEnabled,
        FxFlag::DelayAnalog,
        FxFlag::DelayPingPong,
        FxFlag::DelayFxVariation,
    ];

    pub(super) fn set(self, fx: &mut LayerFxChain, on: bool) {
        match self {
            Self::Mod1Variation => fx.mod1_variation = on,
            Self::Mod1Ped => fx.mod1_ped = on,
            Self::Mod2Variation => fx.mod2_variation = on,
            Self::AmpVariation => fx.amp_variation = on,
            Self::CompEnabled => fx.comp_enabled = on,
            Self::CompFast => fx.comp_fast = on,
            Self::DelayEnabled => fx.delay_enabled = on,
            Self::DelayAnalog => fx.delay_analog = on,
            Self::DelayPingPong => fx.delay_ping_pong = on,
            Self::DelayFxVariation => fx.delay_fx_variation = on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_have_unique_entries() {
        for (i, a) in FxFloat::ALL.iter().enumerate() {
            assert!(FxFloat::ALL[i + 1..].iter().all(|b| b != a));
        }
        for (i, a) in FxFlag::ALL.iter().enumerate() {
            assert!(FxFlag::ALL[i + 1..].iter().all(|b| b != a));
        }
    }

    #[test]
    fn setters_clamp_into_the_engine_ranges() {
        let mut fx = LayerFxChain::new(48_000.0);
        FxFloat::DelayFeedback.set(&mut fx, 5.0);
        assert_eq!(fx.delay_feedback, 0.95);
        FxFloat::EqMidFreq.set(&mut fx, 1.0);
        assert_eq!(fx.eq_mid_freq_hz, 200.0);
        FxFlag::CompFast.set(&mut fx, true);
        assert!(fx.comp_fast);
    }
}
