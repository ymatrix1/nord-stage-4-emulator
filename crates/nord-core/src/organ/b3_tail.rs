//! Frequency response of the B3 processor `0x121E2B` tail, as seen from the ring words.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. Research: `docs/research/organ-b3-tail.md`.
//! The processor adds filtered branches (FIR / lattice sections programmed by the engine
//! constructor) to the dry sum `s = sum W ring`; the whole chain is linear and time invariant, so a
//! sustained wheel line of frequency `f` leaves with the amplitude gain `G(f)` relative to the
//! dry `A W h` line. The table is `tools/organ_b3_proc.py` output over the oracle (`sharc-pass`)
//! with the full ARM boot log loaded (`docs/research/arm-boot-state.md`) and `DM(0xB44C0) = 1.0`: 34 points from single wheels and their
//! harmonics, lines from different notes at one frequency agree within 0.1 dB.

/// `(Hz, dB)` points of `G(f)`, ascending in Hz (at the firmware frame rate, host independent).
///
/// EMPIRICAL: measured gain of the oracle chain per line (`docs/research/data/b3-tail-gain.csv`,
/// test `table_matches_the_derived_csv`); the phase of the response is not recorded, so wheel
/// harmonics keep the firmware table phases (partial amplitudes are right, waveform shape is not).
pub(super) const TAIL_DB: [(f32, f32); 34] = [
    (65.41, 7.68),
    (82.37, 8.03),
    (97.99, 8.25),
    (109.98, 8.26),
    (130.83, 8.25),
    (164.74, 8.09),
    (174.58, 8.00),
    (196.15, 7.86),
    (219.97, 7.53),
    (233.00, 7.41),
    (247.11, 6.88),
    (261.65, 6.93),
    (293.78, 6.54),
    (311.01, 6.22),
    (329.72, 6.04),
    (349.16, 5.71),
    (415.26, 4.74),
    (439.94, 4.42),
    (466.00, 4.14),
    (523.30, 3.37),
    (587.16, 2.54),
    (622.03, 2.09),
    (739.74, 0.83),
    (830.52, -0.02),
    (1046.61, -2.24),
    (1317.93, -3.32),
    (1479.47, -4.15),
    (2093.22, -6.09),
    (2635.85, -7.49),
    (3322.09, -8.70),
    (3953.78, -9.54),
    (4976.23, -10.77),
    (6644.17, -12.52),
    (9952.45, -15.71),
];

/// Linear amplitude gain at `hz`: dB linear in `log2(hz)` between the table points, flat beyond
/// the first (65 Hz) and last point (extrapolation ASSUMED).
pub(super) fn tail_gain(hz: f32) -> f32 {
    let first = TAIL_DB[0];
    let last = TAIL_DB[TAIL_DB.len() - 1];
    let db = if hz <= first.0 {
        first.1
    } else if hz >= last.0 {
        last.1
    } else {
        let i = TAIL_DB.partition_point(|&(f, _)| f <= hz);
        let (f0, g0) = TAIL_DB[i - 1];
        let (f1, g1) = TAIL_DB[i];
        g0 + (g1 - g0) * (hz / f0).log2() / (f1 / f0).log2()
    };
    10.0f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_derived_csv() {
        let csv = include_str!("../../../../docs/research/data/b3-tail-gain.csv");
        let rows: Vec<(f32, f32)> = csv
            .lines()
            .skip(1)
            .map(|l| {
                let (a, b) = l.split_once(',').unwrap();
                (a.parse().unwrap(), b.parse().unwrap())
            })
            .collect();
        assert_eq!(rows.len(), TAIL_DB.len());
        for (r, t) in rows.iter().zip(TAIL_DB.iter()) {
            assert!((r.0 - t.0).abs() < 0.01 && (r.1 - t.1).abs() < 0.01);
        }
        assert!(TAIL_DB.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn gain_interpolates_in_log_frequency_and_clamps() {
        for &(hz, db) in &TAIL_DB {
            let g = 20.0 * tail_gain(hz).log10();
            assert!((g - db).abs() < 1e-3, "{hz}: {g} vs {db}");
        }
        // 440 Hz is +4.4 dB in the oracle, 1 kHz about -2 dB
        assert!((20.0 * tail_gain(440.0).log10() - 4.42).abs() < 0.1);
        assert_eq!(tail_gain(10.0), tail_gain(TAIL_DB[0].0));
        assert_eq!(tail_gain(20_000.0), tail_gain(TAIL_DB[TAIL_DB.len() - 1].0));
        // between two points the dB value lies between theirs
        let mid = 20.0 * tail_gain(300.0).log10();
        assert!(mid > 6.0 && mid < 7.0, "{mid}");
    }
}
