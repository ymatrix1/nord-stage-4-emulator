//! Velocity to level of the hardware piano sampler, opt-in (not wired into the default render).
//!
//! Evidence: `docs/research/piano-sound-load.md` sections 6 and 7 (PR on `research/piano-velocity-law`),
//! captured by running the real firmware in `tools/arm_piano_load.py`. For a note-on the ARM calls
//! the level method of the piano class, `0xC00A1EB4(this, P2, rec, key, X, ..)`, which stores the Q23
//! voice amplitude computed by [`super::level_q23`]:
//!
//! ```text
//! n = rec[0x5E] + 2 * X + 60 - (b1 + b2 + b3)         amplitude = level_q23(n)
//! ```
//!
//! * `X` is the velocity attenuation, a clamped `0..=249` word: `X = clamp(d + ((128 - c) * T) >> 7)`
//!   where `T` is a byte of a velocity curve table (`0xC0532664`, 1024 bytes per curve, curve 11 in
//!   the settled state) indexed by an internal velocity `r8` in `0..=1023` and `(c, d)` are two bytes
//!   per zone (`0x13, 0x00` for the zone used here).
//! * `rec[0x5E]` is a signed halfword of the stroke record the velocity selects (the velocity layer).
//! * `b1 + b2 + b3` are three signed per-key bytes of the sound object.
//!
//! The firmware tables are not copied: the arrays below are the observed values of real note-on runs
//! (VERIFIED where swept). `X` was identical for the three sounds swept (Clavinet D6, Bright Grand,
//! Amber Upright: same curve and zone), so it is one table; the stroke offsets and key sums belong to
//! the sound file. A velocity or note that was not swept is interpolated ([`x_at`], [`key_sum_at`],
//! EMPIRICAL); the stroke offset is a step function ([`stroke_at`], EMPIRICAL: the true switch
//! velocity lies between two swept points).

use super::level_q23::{level_q23, level_table};

/// `n = stroke + 2 X + 60 - key_sum`, the attenuation index of `0xC00A1EB4` (before `max(0, n)`).
pub const fn level_index(stroke: i16, x: i16, key_sum: i16) -> i32 {
    stroke as i32 + 2 * x as i32 + 60 - key_sum as i32
}

/// Q23 amplitude of a note-on from its stroke offset, `X` and per-key byte sum.
pub fn amplitude_q23(stroke: i16, x: i16, key_sum: i16) -> u32 {
    level_q23(level_table(), level_index(stroke, x, key_sum))
}

/// Linear interpolation of `(position, value)` pairs sorted by position: exact at a swept
/// position, rounded half up in between, clamped to the first and last pair.
fn interpolate(points: &[(u8, i16)], at: u8) -> i16 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0;
    };
    if at <= first.0 {
        return first.1;
    }
    if at >= last.0 {
        return last.1;
    }
    let upper = points.partition_point(|p| p.0 < at);
    let (hi, lo) = (points[upper], points[upper - 1]);
    let span = i32::from(hi.0) - i32::from(lo.0);
    let along = i32::from(at) - i32::from(lo.0);
    let sum = i32::from(lo.1) * span + (i32::from(hi.1) - i32::from(lo.1)) * along;
    i16::try_from((2 * sum + span).div_euclid(2 * span)).unwrap_or(0)
}

/// `X` at `velocity` (1..=127): swept value, else interpolated (EMPIRICAL between sweep points).
pub fn x_at(velocity: u8) -> i16 {
    interpolate(&X_SWEEP, velocity)
}

/// Stroke offset `rec[0x5E]` at `velocity` from a sound's `(velocity, offset)` points: the value
/// of the last swept point at or below it (EMPIRICAL step function).
pub fn stroke_at(strokes: &[(u8, i16)], velocity: u8) -> i16 {
    let below = strokes.partition_point(|p| p.0 <= velocity);
    strokes.get(below.saturating_sub(1)).map_or(0, |p| p.1)
}

/// Per-key byte sum `b1 + b2 + b3` at `note`: swept value, else interpolated (EMPIRICAL).
pub fn key_sum_at(keys: &[(u8, i16)], note: u8) -> i16 {
    interpolate(keys, note)
}

/// Q23 amplitude of a note-on at `velocity` and `note` for a sound given by its stroke points
/// and key sums. Opt-in model: nothing in the default render calls it.
pub fn note_amplitude_q23(
    strokes: &[(u8, i16)],
    keys: &[(u8, i16)],
    velocity: u8,
    note: u8,
) -> u32 {
    amplitude_q23(
        stroke_at(strokes, velocity),
        x_at(velocity),
        key_sum_at(keys, note),
    )
}

/// `X` by velocity (curve 11, zone 1), identical for the three sounds swept; 79 velocities.
pub const X_SWEEP: [(u8, i16); 79] = [
    (1, 170),
    (3, 166),
    (5, 162),
    (7, 158),
    (8, 156),
    (9, 154),
    (11, 150),
    (13, 146),
    (15, 143),
    (16, 140),
    (17, 138),
    (19, 135),
    (21, 131),
    (23, 128),
    (24, 126),
    (25, 124),
    (27, 121),
    (29, 118),
    (31, 114),
    (32, 112),
    (33, 111),
    (35, 108),
    (37, 104),
    (39, 102),
    (40, 100),
    (41, 98),
    (43, 96),
    (45, 92),
    (47, 90),
    (48, 88),
    (49, 86),
    (51, 84),
    (53, 81),
    (55, 78),
    (56, 77),
    (57, 75),
    (59, 73),
    (61, 70),
    (63, 68),
    (64, 66),
    (65, 65),
    (67, 63),
    (69, 60),
    (71, 57),
    (72, 56),
    (73, 55),
    (75, 52),
    (77, 50),
    (79, 47),
    (80, 45),
    (81, 45),
    (83, 42),
    (85, 40),
    (87, 38),
    (88, 36),
    (89, 35),
    (91, 33),
    (93, 30),
    (95, 28),
    (96, 27),
    (97, 26),
    (99, 23),
    (101, 22),
    (103, 19),
    (104, 18),
    (105, 17),
    (107, 15),
    (109, 12),
    (111, 11),
    (112, 9),
    (113, 8),
    (115, 6),
    (117, 4),
    (119, 2),
    (120, 1),
    (121, 0),
    (123, 0),
    (125, 0),
    (127, 0),
];

/// CLAVINET_D6 (class byte 4): stroke offset `rec[0x5E]` per swept velocity (step function, first swept velocity of each value), per-key byte sum at note 60 = 40.
pub const CLAVINET_D6_STROKES: [(u8, i16); 3] = [(1, 36), (87, 56), (109, 66)];

/// BRIGHT_GRAND (class byte 5): stroke offset `rec[0x5E]` per swept velocity (step function, first swept velocity of each value), per-key byte sum at note 60 = 49.
pub const BRIGHT_GRAND_STROKES: [(u8, i16); 7] = [
    (1, -10),
    (61, -1),
    (69, -2),
    (81, 8),
    (93, 19),
    (101, 26),
    (117, 34),
];

/// AMBER_UPRIGHT (class byte 6): stroke offset `rec[0x5E]` per swept velocity (step function, first swept velocity of each value), per-key byte sum at note 60 = 54.
pub const AMBER_UPRIGHT_STROKES: [(u8, i16); 7] = [
    (1, -1),
    (49, 8),
    (69, 18),
    (77, 28),
    (89, 36),
    (109, 44),
    (117, 43),
];

/// CLAVINET_D6: per-key byte sum at velocity 64, `(note, b1 + b2 + b3)`.
pub const CLAVINET_D6_KEY_SUMS: [(u8, i16); 8] = [
    (24, 40),
    (36, 40),
    (48, 40),
    (60, 40),
    (72, 40),
    (84, 40),
    (96, 40),
    (108, 40),
];

/// BRIGHT_GRAND: per-key byte sum at velocity 64, `(note, b1 + b2 + b3)`.
pub const BRIGHT_GRAND_KEY_SUMS: [(u8, i16); 8] = [
    (24, 49),
    (36, 49),
    (48, 45),
    (60, 49),
    (72, 63),
    (84, 49),
    (96, 49),
    (108, 49),
];

/// AMBER_UPRIGHT: per-key byte sum at velocity 64, `(note, b1 + b2 + b3)`.
pub const AMBER_UPRIGHT_KEY_SUMS: [(u8, i16); 8] = [
    (24, 49),
    (36, 49),
    (48, 49),
    (60, 54),
    (72, 63),
    (84, 58),
    (96, 19),
    (108, 49),
];

#[cfg(test)]
mod tests {
    use super::*;

    // Goldens: `(velocity, Q23 amplitude)` stored by the firmware's `0xC00A1EB4` in the real runs
    // (note 60, `tools/arm_piano_load.py`, `docs/research/piano-sound-load.md` sections 6 and 7).
    const CLAVINET_GOLDEN: [(u8, u32); 80] = [
        (1, 0x151cb),
        (3, 0x17280),
        (5, 0x1965f),
        (7, 0x1bdb8),
        (8, 0x1d2cd),
        (9, 0x1e8e1),
        (11, 0x21836),
        (13, 0x24c22),
        (15, 0x27658),
        (16, 0x2a396),
        (17, 0x2c38a),
        (19, 0x2f652),
        (21, 0x33fbf),
        (23, 0x37b71),
        (24, 0x3a59a),
        (25, 0x3d1c2),
        (27, 0x417ef),
        (29, 0x46326),
        (31, 0x4cfe5),
        (32, 0x50a28),
        (33, 0x52850),
        (35, 0x58714),
        (37, 0x61018),
        (39, 0x6597f),
        (40, 0x6a660),
        (41, 0x6f6e3),
        (43, 0x74b35),
        (45, 0x80000),
        (47, 0x860db),
        (48, 0x8c64c),
        (49, 0x93088),
        (51, 0x99fcb),
        (53, 0xa50a1),
        (55, 0xb0e28),
        (56, 0xb504f),
        (57, 0xbd949),
        (59, 0xc68be),
        (61, 0xd4cc0),
        (63, 0xdedc6),
        (64, 0xe966a),
        (65, 0xeedb4),
        (67, 0xfa272),
        (69, 0x10c1b7),
        (71, 0x11f59a),
        (72, 0x126111),
        (73, 0x12cf0b),
        (75, 0x1428a3),
        (77, 0x151cb4),
        (79, 0x16a09e),
        (80, 0x17b293),
        (81, 0x17b293),
        (83, 0x1965fe),
        (85, 0x1a9980),
        (87, 0x161c50),
        (88, 0x172803),
        (89, 0x17b293),
        (91, 0x18d17d),
        (93, 0x1a9980),
        (95, 0x1bdb8c),
        (96, 0x1c823e),
        (97, 0x1d2cd4),
        (99, 0x1f44e5),
        (101, 0x200000),
        (103, 0x224bf8),
        (104, 0x231930),
        (105, 0x23eb35),
        (107, 0x259e16),
        (109, 0x23eb35),
        (111, 0x24c223),
        (112, 0x267f2e),
        (113, 0x276589),
        (115, 0x294285),
        (117, 0x2b3611),
        (119, 0x2d413d),
        (120, 0x2e5007),
        (121, 0x2f6526),
        (123, 0x2f6526),
        (125, 0x2f6526),
        (127, 0x2f6526),
        (127, 0x2f6526),
    ];
    const GRAND_GOLDEN: [(u8, u32); 33] = [
        (1, 0x27dab),
        (5, 0x2ff22),
        (9, 0x39ae1),
        (13, 0x4563f),
        (17, 0x537a8),
        (21, 0x62220),
        (25, 0x735c2),
        (29, 0x84839),
        (33, 0x9bc6c),
        (37, 0xb71f6),
        (41, 0xd25a4),
        (45, 0xf1a1c),
        (49, 0x1158fe),
        (53, 0x1378d8),
        (57, 0x165e15),
        (61, 0x16a09e),
        (65, 0x1965fe),
        (69, 0x1cd70b),
        (73, 0x205f2f),
        (77, 0x24560d),
        (81, 0x24560d),
        (85, 0x28c933),
        (89, 0x2dc7da),
        (93, 0x2d413d),
        (97, 0x31a2fa),
        (101, 0x3236a0),
        (105, 0x385cd4),
        (109, 0x3f43d0),
        (113, 0x4563f9),
        (117, 0x4563f9),
        (121, 0x4c1bf8),
        (125, 0x4c1bf8),
        (127, 0x4c1bf8),
    ];
    const UPRIGHT_GOLDEN: [(u8, u32); 33] = [
        (1, 0x260df),
        (5, 0x2dc7d),
        (9, 0x37134),
        (13, 0x4241c),
        (17, 0x4fb57),
        (21, 0x5db39),
        (25, 0x6e268),
        (29, 0x7e87a),
        (33, 0x94bde),
        (37, 0xaeda6),
        (41, 0xc8da8),
        (45, 0xe6b85),
        (49, 0xeedb4),
        (53, 0x10c1b7),
        (57, 0x133f97),
        (61, 0x159b08),
        (65, 0x184060),
        (69, 0x184060),
        (73, 0x1b38aa),
        (77, 0x1b38aa),
        (81, 0x1e8e10),
        (85, 0x224bf8),
        (89, 0x231930),
        (93, 0x276589),
        (97, 0x2b3611),
        (101, 0x2f6526),
        (105, 0x353300),
        (109, 0x367154),
        (113, 0x3bb6d0),
        (117, 0x4241c8),
        (121, 0x48ac1b),
        (125, 0x48ac1b),
        (127, 0x48ac1b),
    ];

    fn check(golden: &[(u8, u32)], strokes: &[(u8, i16)], key60: i16) {
        for &(velocity, amplitude) in golden {
            let got = note_amplitude_q23(strokes, &[(60, key60)], velocity, 60);
            assert_eq!(got, amplitude, "velocity {velocity}");
        }
    }

    #[test]
    fn clavinet_sweep_reproduces_the_firmware_amplitudes() {
        check(&CLAVINET_GOLDEN, &CLAVINET_D6_STROKES, 40);
    }

    #[test]
    fn bright_grand_sweep_reproduces_the_firmware_amplitudes() {
        check(&GRAND_GOLDEN, &BRIGHT_GRAND_STROKES, 49);
    }

    #[test]
    fn amber_upright_sweep_reproduces_the_firmware_amplitudes() {
        check(&UPRIGHT_GOLDEN, &AMBER_UPRIGHT_STROKES, 54);
    }

    #[test]
    fn index_formula_matches_the_listing() {
        assert_eq!(level_index(36, 170, 40), 396);
        assert_eq!(level_index(66, 0, 40), 86);
        assert_eq!(amplitude_q23(66, 0, 40), 0x2F_6526);
    }

    #[test]
    fn x_is_monotone_non_increasing_and_ends_at_zero() {
        let mut previous = i16::MAX;
        for velocity in 1..=127u8 {
            let x = x_at(velocity);
            assert!(x <= previous, "velocity {velocity}");
            previous = x;
        }
        assert_eq!(x_at(1), 170);
        assert_eq!(x_at(127), 0);
    }

    #[test]
    fn interpolation_is_exact_at_swept_points_and_between_neighbours_elsewhere() {
        for &(velocity, x) in &X_SWEEP {
            assert_eq!(x_at(velocity), x);
        }
        // 2 is between (1, 170) and (3, 166).
        assert_eq!(x_at(2), 168);
        // 100 is between (99, 23) and (101, 22): half rounds up.
        assert_eq!(x_at(100), 23);
        assert_eq!(x_at(0), 170);
        assert_eq!(x_at(255), 0);
    }

    #[test]
    fn stroke_is_a_step_function_of_velocity() {
        assert_eq!(stroke_at(&CLAVINET_D6_STROKES, 1), 36);
        assert_eq!(stroke_at(&CLAVINET_D6_STROKES, 86), 36);
        assert_eq!(stroke_at(&CLAVINET_D6_STROKES, 87), 56);
        assert_eq!(stroke_at(&CLAVINET_D6_STROKES, 127), 66);
        assert_eq!(stroke_at(&[], 64), 0);
    }

    #[test]
    fn key_sums_are_interpolated_between_swept_notes() {
        let keys = [(48, 45), (60, 49), (72, 63)];
        assert_eq!(key_sum_at(&keys, 60), 49);
        assert_eq!(key_sum_at(&keys, 66), 56);
        assert_eq!(key_sum_at(&keys, 10), 45);
        assert_eq!(key_sum_at(&keys, 120), 63);
        assert_eq!(key_sum_at(&[], 60), 0);
    }

    #[test]
    fn louder_velocity_gives_a_larger_amplitude_within_a_stroke() {
        let soft = note_amplitude_q23(&CLAVINET_D6_STROKES, &[(60, 40)], 10, 60);
        let loud = note_amplitude_q23(&CLAVINET_D6_STROKES, &[(60, 40)], 80, 60);
        assert!(loud > soft);
    }
}
