//! Global Sound-menu settings an Organ layer honours (manual OS v1.6x p.58).
//!
//! Every field defaults to the value the engine hard-coded before the menu was modelled, so a layer
//! that never receives [`OrganGlobals`] renders bit-identically (`1.0` factors and `0.0` cents are
//! exact no-ops in `f32`). The engine pushes one copy to each Organ layer through
//! `Stage4Engine::set_global_settings`.

/// Factors and offsets applied on top of the per-layer Organ state.
///
/// ASSUMED: all non-default values below are not measured on hardware. The manual (p.58) names the
/// menu entries and their choices, never the resulting gains or speeds. What would confirm them:
/// a recording of the same B3 chord with each menu choice, compared against `render` output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrganGlobals {
    /// Multiplier on `crosstalk_level` (B3 Tonewheel Mode: Clean 0.0, Vintage 1 1.0, Vintage 2 1.5).
    pub crosstalk_scale: f32,
    /// Multiplier on the key-click excitation (Click Level: Normal 1.0, High 2.0).
    pub click_scale: f32,
    /// Master Fine Tune in cents, added to the layer's `fine_tune_cents` (System menu 3).
    pub master_tune_cents: f32,
    /// Woofer rotor speed factor (Rotary Rotor Speed Low/Normal/High).
    pub rotor_speed_scale: f32,
    /// Treble horn speed factor (Rotary Horn Speed Low/Normal/High).
    pub horn_speed_scale: f32,
    /// Woofer rotor acceleration time-constant factor (`> 1` = slower ramp).
    pub rotor_tau_scale: f32,
    /// Treble horn acceleration time-constant factor (`> 1` = slower ramp).
    pub horn_tau_scale: f32,
    /// Gain of the bass (rotor) band; `2 * bass_fraction` of the Bass/Horn Level setting.
    pub bass_gain: f32,
    /// Gain of the treble (horn) band; `2 * horn_fraction` of the Bass/Horn Level setting.
    pub horn_gain: f32,
}

impl Default for OrganGlobals {
    fn default() -> Self {
        Self {
            crosstalk_scale: 1.0,
            click_scale: 1.0,
            master_tune_cents: 0.0,
            rotor_speed_scale: 1.0,
            horn_speed_scale: 1.0,
            rotor_tau_scale: 1.0,
            horn_tau_scale: 1.0,
            bass_gain: 1.0,
            horn_gain: 1.0,
        }
    }
}

impl super::OrganLayer {
    /// Layer Fine Tune plus the global master tune (`+ 0.0` at the default, an exact no-op).
    pub(super) fn effective_fine_cents(&self) -> f32 {
        self.fine_tune_cents + self.globals.master_tune_cents
    }
}

#[cfg(test)]
mod tests {
    use super::super::OrganLayer;
    use super::*;

    fn settled_rates(globals: OrganGlobals) -> (f32, f32) {
        let mut o = OrganLayer::new(48_000.0);
        o.globals = globals;
        o.rotary_speed = super::super::RotarySpeed::Fast;
        for _ in 0..48_000 * 25 {
            o.process_rotary(0.0, 1.0 / 48_000.0);
        }
        (o.horn_rate_hz, o.drum_rate_hz)
    }

    #[test]
    fn speed_factors_scale_the_settled_rotor_rates() {
        let (horn, drum) = settled_rates(OrganGlobals::default());
        assert!(
            (horn - 6.85).abs() < 0.05 && (drum - 5.70).abs() < 0.05,
            "{horn} {drum}"
        );
        let fast = OrganGlobals {
            horn_speed_scale: 1.15,
            rotor_speed_scale: 0.85,
            ..OrganGlobals::default()
        };
        let (horn2, drum2) = settled_rates(fast);
        assert!((horn2 / horn - 1.15).abs() < 5e-3, "{horn2}");
        assert!((drum2 / drum - 0.85).abs() < 5e-3, "{drum2}");
    }

    #[test]
    fn slower_acceleration_lags_behind_after_one_second() {
        let after_one_second = |tau: f32| {
            let mut o = OrganLayer::new(48_000.0);
            o.globals.horn_tau_scale = tau;
            o.rotary_speed = super::super::RotarySpeed::Fast;
            for _ in 0..48_000 {
                o.process_rotary(0.0, 1.0 / 48_000.0);
            }
            o.horn_rate_hz
        };
        assert!(after_one_second(1.5) < after_one_second(1.0));
        assert!(after_one_second(0.67) > after_one_second(1.0));
    }

    #[test]
    fn bass_horn_gains_weight_the_bands() {
        let band_energy = |g: OrganGlobals| {
            let mut o = OrganLayer::new(48_000.0);
            o.globals = g;
            let mut sum = 0.0f32;
            for n in 0..48_000 {
                // 100 Hz sits in the bass band, 3 kHz in the horn band of the 800 Hz crossover
                let x = 0.3 * (n as f32 * std::f32::consts::TAU * 3000.0 / 48_000.0).sin();
                let [l, r] = o.process_rotary(x, 1.0 / 48_000.0);
                sum += l * l + r * r;
            }
            sum
        };
        let mute_horn = OrganGlobals {
            horn_gain: 0.0,
            ..OrganGlobals::default()
        };
        assert!(band_energy(mute_horn) < 0.1 * band_energy(OrganGlobals::default()));
    }

    #[test]
    fn master_tune_adds_to_the_layer_fine_tune() {
        let mut o = OrganLayer::new(48_000.0);
        o.fine_tune_cents = 10.0;
        assert_eq!(o.effective_fine_cents(), 10.0);
        o.globals.master_tune_cents = -4.0;
        assert_eq!(o.effective_fine_cents(), 6.0);
    }

    #[test]
    fn defaults_are_exact_no_ops() {
        let g = OrganGlobals::default();
        for x in [0.37f32, -1.25, 1e-7, 123.456] {
            assert_eq!(x * g.crosstalk_scale, x);
            assert_eq!(x * g.click_scale, x);
            assert_eq!(x + g.master_tune_cents, x);
            assert_eq!(x * g.bass_gain, x);
            assert_eq!(x * g.horn_gain, x);
        }
        assert_eq!(0.7f32 * g.horn_tau_scale, 0.7);
        assert_eq!(4.2f32 * g.rotor_tau_scale, 4.2);
        assert_eq!(6.85f32 * g.horn_speed_scale, 6.85);
        assert_eq!(5.70f32 * g.rotor_speed_scale, 5.70);
    }
}
