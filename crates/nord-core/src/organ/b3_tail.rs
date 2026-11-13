//! Frequency response of the B3 processor `0x121E2B` tail, as seen from the ring words.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. Research: `docs/research/organ-b3-tail.md`.
//! The processor adds filtered branches (FIR / lattice sections programmed by the engine
//! constructor) to the dry sum `s = sum W ring`; the whole chain is linear and time invariant, so a
//! sustained wheel line of frequency `f` leaves with the amplitude gain `G(f)` relative to the
//! dry `A W h` line. The table is `tools/organ_b3_proc.py` output over the oracle (`sharc-pass`)
//! with the constructor loaded and `DM(0xB44C0) = 1.0`: 82 points from single wheels and their
//! harmonics, lines from different notes at one frequency agree within 0.1 dB.

/// `(Hz, dB)` points of `G(f)`, ascending in Hz (at the firmware frame rate, host independent).
///
/// EMPIRICAL: measured gain of the oracle chain per line (`docs/research/data/b3-tail-gain.csv`,
/// test `table_matches_the_derived_csv`); the phase of the response is not recorded, so wheel
/// harmonics keep the firmware table phases (partial amplitudes are right, waveform shape is not).
pub(super) const TAIL_DB: [(f32, f32); 82] = [
    (65.41, 0.39),
    (69.30, 0.19),
    (73.40, -0.16),
    (77.75, -0.91),
    (82.37, -1.04),
    (87.29, -1.46),
    (92.47, -1.78),
    (97.99, -2.21),
    (103.81, -2.67),
    (109.98, -3.06),
    (116.50, -3.60),
    (123.45, -3.99),
    (130.83, -4.49),
    (138.59, -4.99),
    (146.79, -5.54),
    (155.51, -6.09),
    (164.74, -6.66),
    (174.58, -7.18),
    (184.93, -7.73),
    (196.11, -8.18),
    (207.76, -9.02),
    (220.04, -9.57),
    (233.09, -10.28),
    (247.00, -11.02),
    (261.72, -11.82),
    (277.26, -12.33),
    (293.68, -13.07),
    (311.12, -13.88),
    (329.58, -14.62),
    (349.27, -15.60),
    (370.03, -16.39),
    (391.96, -17.35),
    (415.26, -18.22),
    (439.94, -19.16),
    (466.00, -20.05),
    (493.79, -21.30),
    (523.30, -22.19),
    (554.37, -23.14),
    (587.16, -24.05),
    (622.03, -24.90),
    (658.96, -25.53),
    (698.32, -25.80),
    (739.74, -25.97),
    (783.92, -26.11),
    (830.52, -26.31),
    (879.88, -25.09),
    (932.01, -24.74),
    (987.58, -24.33),
    (1046.61, -24.10),
    (1108.74, -23.97),
    (1174.33, -24.01),
    (1244.06, -24.14),
    (1317.93, -24.48),
    (1396.63, -24.89),
    (1479.47, -25.46),
    (1567.84, -26.25),
    (1661.04, -25.20),
    (1759.77, -24.84),
    (1864.01, -23.95),
    (1975.16, -23.19),
    (2093.22, -22.84),
    (2217.49, -22.85),
    (2348.66, -23.53),
    (2488.11, -23.70),
    (2635.85, -23.55),
    (2793.26, -22.49),
    (2958.95, -21.68),
    (3137.76, -21.06),
    (3322.09, -20.88),
    (3521.26, -20.56),
    (3728.03, -19.69),
    (3952.05, -18.78),
    (4186.44, -18.23),
    (4434.97, -17.89),
    (4697.31, -17.36),
    (4976.23, -16.75),
    (5271.71, -15.99),
    (6271.37, -13.77),
    (6649.70, -13.53),
    (7039.07, -13.40),
    (7460.20, -12.74),
    (7904.11, -12.83),
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
        // 440 Hz is -19.1 dB in the oracle, 880 Hz about -25 dB
        assert!((20.0 * tail_gain(440.0).log10() + 19.14).abs() < 0.1);
        assert_eq!(tail_gain(10.0), tail_gain(TAIL_DB[0].0));
        assert_eq!(tail_gain(20_000.0), tail_gain(TAIL_DB[TAIL_DB.len() - 1].0));
        // between two points the dB value lies between theirs
        let mid = 20.0 * tail_gain(300.0).log10();
        assert!(mid < -12.0 && mid > -15.0, "{mid}");
    }
}
