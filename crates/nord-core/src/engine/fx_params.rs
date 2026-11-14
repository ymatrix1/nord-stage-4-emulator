//! Allocation-light copy of the user-facing parameters of a [`LayerFxChain`].
//!
//! A chain owns about a megabyte of delay and reverb buffers. Program snapshots, the
//! clipboard and Live slots only need its panel parameters, so they store this struct and
//! leave the DSP state of the destination chain untouched. Firmware opt-ins
//! (`firmware_fx`, `amp_fw_coeffs`) are engine settings, not program data, and are not copied.

use crate::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, DelaySubdivision, FxMorphSet, LayerFxChain,
    Mod1Type, Mod2Type, ReverbColor, ReverbModel,
};

macro_rules! fx_params {
    ($($field:ident : $ty:ty),* $(,)?) => {
        /// Panel parameters of one layer FX chain.
        #[derive(Debug, Clone, Copy)]
        pub struct FxParams { $(pub $field: $ty),* }

        impl FxParams {
            /// Reads the parameters of `chain`.
            pub fn capture(chain: &LayerFxChain) -> Self {
                Self { $($field: chain.$field),* }
            }

            /// Writes the parameters into `chain` (no allocation; DSP state is kept).
            pub fn apply(&self, chain: &mut LayerFxChain) {
                $(chain.$field = self.$field;)*
            }
        }
    };
}

fx_params! {
    mod1_type: Mod1Type, mod1_rate_hz: f32, mod1_amount: f32,
    mod2_type: Mod2Type, mod2_rate_hz: f32, mod2_amount: f32,
    amp_cabinet: AmpCabinet, amp_variation: bool, eq_filter_res: f32, drive_amount: f32,
    eq_bass_db: f32, eq_mid_db: f32, eq_mid_freq_hz: f32, eq_treble_db: f32,
    comp_enabled: bool, comp_amount: f32, comp_fast: bool,
    delay_enabled: bool, delay_time_ms: f32, delay_clock_sync: bool,
    delay_subdivision: DelaySubdivision, delay_sync_beats: Option<f32>,
    delay_analog: bool, delay_ping_pong: bool, delay_filter: DelayFilterMode,
    delay_fx: DelayFeedbackFx, delay_fx_variation: bool, delay_feedback: f32, delay_mix: f32,
    reverb_model: ReverbModel, reverb_color: ReverbColor, reverb_variation: bool, reverb_mix: f32,
    morph: FxMorphSet, delay_rate_raw: Option<u16>, mod1_rate_raw: Option<u8>, mod1_type_raw: Option<u8>,
}

impl FxParams {
    /// Parameters of a freshly created chain (all effects off, default values).
    pub fn defaults(sample_rate: f32) -> Self {
        Self::capture(&LayerFxChain::new(sample_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_apply_round_trips() {
        let mut a = LayerFxChain::new(48_000.0);
        a.delay_mix = 0.9;
        a.reverb_mix = 0.7;
        a.morph.mod1_rate.delta_wheel = 0.3;
        let p = FxParams::capture(&a);
        let mut b = LayerFxChain::new(48_000.0);
        p.apply(&mut b);
        assert_eq!(b.delay_mix, 0.9);
        assert_eq!(b.reverb_mix, 0.7);
        assert_eq!(b.morph.mod1_rate.delta_wheel, 0.3);
    }

    #[test]
    fn defaults_equal_a_new_chain() {
        let d = FxParams::defaults(44_100.0);
        let c = LayerFxChain::new(44_100.0);
        assert_eq!(d.delay_time_ms, c.delay_time_ms);
        assert!(!d.delay_enabled && !d.comp_enabled);
        assert_eq!(d.reverb_mix, c.reverb_mix);
    }
}
