//! Rotary ANGLE (Shift + STOP MODE): where the horn and rotor rest once the rotary is stopped.
//!
//! Manual p.53: with STOP MODE active, `ANGLE` picks one of a list of positions so the rotor and
//! horn always stop at the same place; the default `Free` leaves the stop position to chance.
//! Course video ORG 10:08-10:22 shows the list `Free`, `1: Front Center`, `2: Front Left`,
//! `4: Back Left`, `6: Back Center` (`docs/research/dsoundman-course-findings.md`).

use std::f32::consts::{PI, TAU};

/// Number of stop positions after `Free` (value `0`).
pub const STOP_ANGLE_COUNT: u8 = 6;

/// Display text of each stop angle, index `0` = `Free`.
///
/// VERIFIED (course video ORG 10:08-10:22, read by eye): `Free`, `1: Front Center`, `2: Front
/// Left`, `4: Back Left`, `6: Back Center`. ASSUMED: `3: Front Right` and `5: Back Right` were not
/// on screen; they follow from the left/right pairing of the shown entries.
pub const STOP_ANGLE_LABELS: [&str; 7] = [
    "Free",
    "1: Front Center",
    "2: Front Left",
    "3: Front Right",
    "4: Back Left",
    "5: Back Right",
    "6: Back Center",
];

/// Rest phase (radians) of the rotor for each angle, index `0` unused (`Free`).
///
/// Phase `0` points the horn at the microphone (the Doppler delay is minimal and the beam gain
/// maximal there, `rotary.rs`), so `1: Front Center` is `0`; positive phase swings to the left.
/// ASSUMED: the 60 degree front/back left/right offsets are a guess from the position names; a
/// recording of the hardware stopped at each angle would confirm them.
pub const STOP_ANGLE_RAD: [f32; 7] = [
    0.0,
    0.0,
    PI / 3.0,
    -PI / 3.0,
    2.0 * PI / 3.0,
    -2.0 * PI / 3.0,
    PI,
];

/// Clamp a raw index to `0..=STOP_ANGLE_COUNT` (`0` = `Free`).
pub fn clamp_stop_angle(i: u8) -> u8 {
    i.min(STOP_ANGLE_COUNT)
}

/// Label of a stop angle index (out of range clamps to the last entry).
pub fn stop_angle_label(i: u8) -> &'static str {
    STOP_ANGLE_LABELS[usize::from(clamp_stop_angle(i))]
}

/// One step of a phase easing toward the angle `target`, along the shortest arc.
///
/// `k` is the per-sample blend (`0.0..=1.0`). Allocation free.
#[inline]
pub fn ease_phase(phase: f32, target: f32, k: f32) -> f32 {
    let mut d = (target - phase) % TAU;
    if d > PI {
        d -= TAU;
    } else if d < -PI {
        d += TAU;
    }
    (phase + d * k.clamp(0.0, 1.0)).rem_euclid(TAU)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cover_free_and_six_positions() {
        assert_eq!(STOP_ANGLE_LABELS.len(), usize::from(STOP_ANGLE_COUNT) + 1);
        assert_eq!(stop_angle_label(0), "Free");
        assert_eq!(stop_angle_label(1), "1: Front Center");
        assert_eq!(stop_angle_label(6), "6: Back Center");
        assert_eq!(stop_angle_label(200), "6: Back Center");
        assert_eq!(clamp_stop_angle(9), 6);
    }

    #[test]
    fn easing_takes_the_short_way_round_and_converges() {
        // from 350 deg to 10 deg is a 20 deg step forward, not 340 back
        let p = ease_phase(350.0_f32.to_radians(), 10.0_f32.to_radians(), 0.5);
        let want = 0.0_f32;
        let e = (p - want).rem_euclid(TAU);
        assert!(e.min(TAU - e) < 1e-4, "p={p}");
        let mut p = 1.0;
        for _ in 0..200 {
            p = ease_phase(p, 3.0, 0.1);
        }
        assert!((p - 3.0).abs() < 1e-3);
    }
}
