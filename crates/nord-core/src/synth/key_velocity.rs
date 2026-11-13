//! Synth key-down velocity byte of the ARM -> DSP key message (opt-in strip chain voice level).
//!
//! The ARM key handler `0xC01EC690` sends one word per synth key, byte 1 = `0x80 + g(vel)`
//! (`docs/research/keydown-zones.md` s7). The MIDI velocity first goes through the curve table
//! `[0xC02B3128 + 0x18 + 2 (128 curve + vel)]` (curve 6 by default, value `c` 17 .. 656), then through
//! a saturating S-curve to `g`. Table and fit: `tools/synth_velocity.py` (`--check` re-derives both
//! from the two data CSVs).

/// VERIFIED: `g(vel)` for `vel` 1..=127 (index `vel - 1`) = ARM run of the synth key handler at note
/// 60, velocity 1..127, `docs/research/data/keydown-synth-velocity.csv` (byte 1 minus `0x80`),
/// checked by `table_matches_the_arm_run_anchors` and `python tools/synth_velocity.py --check`,
/// max err 0 (derived values, not firmware bytes).
pub const KEY_VELOCITY_G: [u8; 127] = [
    0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 6, 6, 6, 6, 6, 7, 8, 8,
    8, 8, 9, 9, 10, 11, 11, 12, 12, 13, 14, 14, 15, 16, 16, 17, 18, 19, 19, 20, 21, 22, 24, 24, 25,
    26, 27, 29, 30, 31, 32, 32, 35, 35, 37, 38, 39, 41, 42, 45, 46, 48, 49, 51, 52, 54, 57, 58, 60,
    61, 63, 66, 67, 69, 72, 73, 75, 78, 79, 81, 83, 85, 88, 89, 92, 93, 96, 98, 100, 102, 105, 106,
    108, 110, 111, 113, 115, 117, 119, 120, 121, 122, 123, 124, 125, 126, 126, 127, 127, 127, 127,
    127, 127, 127, 127,
];

/// Curve value `c` at which `g` saturates at 127.
/// EMPIRICAL: smoothstep fit of the 127 pairs `(c(vel), g(vel))`, 97 of 127 exact, max err 1
/// (`tools/synth_velocity.py --check`); the formula is not decoded from the DSP.
pub const KEY_CURVE_SATURATION: f32 = 600.0;

/// `g` byte for MIDI velocity `vel` (clamped to 1..=127).
#[inline]
pub fn key_velocity_g(vel: u8) -> u8 {
    KEY_VELOCITY_G[usize::from(vel.clamp(1, 127)) - 1]
}

/// EMPIRICAL: `g = floor(127 s(c / 600))` with `s(x) = 3x^2 - 2x^3` for velocity curve value `c`
/// (97 of 127 exact, max err 1).
#[inline]
pub fn key_velocity_g_fit(curve_value: f32) -> u8 {
    let x = (curve_value / KEY_CURVE_SATURATION).clamp(0.0, 1.0);
    (127.0 * x * x * (3.0 - 2.0 * x)).floor() as u8
}

/// Voice level of a key at normalised velocity `vel_norm = vel / 127`.
/// ASSUMED: the DSP applies `g / 127` as a linear amplitude (the velocity byte of the message is
/// not followed into the stage code); confirm with a level sweep over velocity on the hardware.
#[inline]
pub fn key_velocity_gain(vel_norm: f32) -> f32 {
    let vel = (vel_norm.clamp(0.0, 1.0) * 127.0).round() as u8;
    f32::from(key_velocity_g(vel)) / 127.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_arm_run_anchors() {
        // docs/research/keydown-zones.md s7: g = 0, 3, 8, 17, 32, 58, 98, 127 at velocity
        // 1, 16, 32, 48, 64, 80, 100, 127.
        let at = |v: u8| key_velocity_g(v);
        assert_eq!(
            [1, 16, 32, 48, 64, 80, 100, 127].map(at),
            [0, 3, 8, 17, 32, 58, 98, 127]
        );
        assert!(KEY_VELOCITY_G.windows(2).all(|w| w[0] <= w[1]), "monotone");
        let distinct = KEY_VELOCITY_G
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(distinct.len(), 85, "85 distinct values over 127 velocities");
    }

    #[test]
    fn fit_is_within_one_step_at_the_observed_curve_points() {
        // (c, g) pairs of curve 6 from keydown-velocity-curve.csv and keydown-synth-velocity.csv.
        for (c, g) in [
            (17.0, 0u8),
            (93.0, 8),
            (203.0, 32),
            (284.0, 58),
            (416.0, 98),
            (590.0, 127),
        ] {
            let d = i32::from(key_velocity_g_fit(c)) - i32::from(g);
            assert!(d.abs() <= 1, "c={c}: fit {} vs {g}", key_velocity_g_fit(c));
        }
    }

    #[test]
    fn gain_is_monotone_unit_range_and_clamps() {
        let mut prev = 0.0;
        for v in 1..=127u8 {
            let g = key_velocity_gain(f32::from(v) / 127.0);
            assert!((0.0..=1.0).contains(&g) && g >= prev);
            prev = g;
        }
        assert_eq!(key_velocity_gain(0.0), 0.0);
        assert_eq!(key_velocity_gain(2.0), 1.0);
        assert!(
            key_velocity_gain(0.5) < 0.3,
            "mid velocity is far below the linear 0.5"
        );
    }
}
