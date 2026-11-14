//! Measured line gain of the Farfisa processor `0x122B85`, per weighted sum (not yet wired).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.8. Research: `docs/research/organ-farfisa-bass.md`
//! s8. The processor is linear and time invariant in the ring words and the key-down weights, so a
//! ring slot driven through sum `s` leaves with `G_s(f)` at every harmonic
//! (`tools/organ_farfisa_proc.py` over `sharc-chain`: generator + processor from the boot-plus-install
//! state, one weight word 1.0, 4,000 calls, 22 ring slots per sum). Sum 0 carries the key's 16' row
//! (`note - 12`), sum 1 its 8' row (`note`); sums 2 and 3 give no output with the installed gains.
//!
//! EMPIRICAL: median per 1/6 octave bin of the line gains
//! (`docs/research/data/farfisa-proc-gain.csv`, test `tables_match_the_derived_csv`); linearity
//! (weight 1.0 against 0.5) 0.00 dB, spread of the slots inside a bin median 0.9 dB. Phase is not
//! recorded (as for `b3_tail.rs`).

pub(super) const SUM0_DB: [(f32, f32); 57] = [
    (15.89, -8.63),
    (21.19, -6.15),
    (26.49, -4.09),
    (37.09, -2.59),
    (47.68, -1.58),
    (52.98, -1.20),
    (58.28, -1.05),
    (68.88, -1.00),
    (74.17, -0.92),
    (90.07, -1.13),
    (111.26, -1.49),
    (116.56, -1.61),
    (137.75, -2.12),
    (148.35, -2.28),
    (169.54, -2.80),
    (185.44, -3.12),
    (222.52, -3.85),
    (233.12, -4.10),
    (275.50, -4.87),
    (291.40, -5.12),
    (344.38, -5.74),
    (370.87, -5.90),
    (434.45, -5.85),
    (466.24, -5.58),
    (551.01, -4.02),
    (593.39, -2.95),
    (694.06, -0.67),
    (757.64, 0.32),
    (868.90, 1.23),
    (953.67, 1.23),
    (1086.12, -0.04),
    (1202.68, -1.45),
    (1366.93, -3.25),
    (1525.87, -4.62),
    (1727.20, -6.13),
    (1923.24, -7.42),
    (2172.25, -8.97),
    (2421.26, -10.55),
    (2739.15, -12.66),
    (3057.04, -14.67),
    (3449.11, -16.57),
    (3851.77, -17.98),
    (4336.55, -19.24),
    (4858.42, -20.35),
    (5457.11, -21.46),
    (6119.39, -22.58),
    (6845.23, -23.68),
    (7761.82, -24.99),
    (8643.96, -26.20),
    (9682.40, -27.52),
    (10855.95, -28.99),
    (12212.28, -30.70),
    (13801.73, -32.79),
    (15285.22, -34.95),
    (17298.52, -38.55),
    (19200.56, -43.72),
    (20721.14, -51.17),
];

pub(super) const SUM1_DB: [(f32, f32); 53] = [
    (26.49, -7.40),
    (37.09, -4.63),
    (47.68, -2.54),
    (58.28, -1.21),
    (74.17, -0.54),
    (90.07, -0.49),
    (111.26, -0.79),
    (116.56, -0.93),
    (140.40, -1.57),
    (148.35, -1.70),
    (174.84, -2.46),
    (185.44, -2.73),
    (222.52, -3.62),
    (233.12, -3.93),
    (275.50, -4.88),
    (291.40, -5.19),
    (349.68, -6.00),
    (370.87, -6.17),
    (434.45, -6.18),
    (466.24, -5.90),
    (551.01, -4.24),
    (588.10, -3.29),
    (694.06, -0.74),
    (741.74, 0.05),
    (874.20, 1.24),
    (943.07, 1.26),
    (1102.02, -0.21),
    (1202.68, -1.45),
    (1377.52, -3.33),
    (1515.28, -4.55),
    (1727.20, -6.13),
    (1915.29, -7.37),
    (2172.25, -8.96),
    (2405.37, -10.43),
    (2739.15, -12.66),
    (3051.75, -14.63),
    (3465.00, -16.63),
    (3846.47, -17.95),
    (4349.80, -19.26),
    (4879.61, -20.39),
    (5478.31, -21.50),
    (6114.09, -22.56),
    (6845.23, -23.68),
    (7751.22, -24.97),
    (8662.51, -26.20),
    (9690.35, -27.52),
    (10845.35, -28.97),
    (12214.93, -30.69),
    (13801.73, -32.79),
    (15285.22, -34.95),
    (17298.52, -38.55),
    (19200.56, -43.59),
    (20721.14, -51.17),
];

/// Linear amplitude gain of sum `sum` (0 or 1) at `hz`: dB linear in `log2(hz)` between the table
/// points, flat beyond the ends (extrapolation ASSUMED).
pub fn farfisa_proc_gain(sum: usize, hz: f32) -> f32 {
    let t: &[(f32, f32)] = if sum == 0 { &SUM0_DB } else { &SUM1_DB };
    let (first, last) = (t[0], t[t.len() - 1]);
    let db = if hz <= first.0 {
        first.1
    } else if hz >= last.0 {
        last.1
    } else {
        let i = t.partition_point(|&(f, _)| f <= hz);
        let ((f0, g0), (f1, g1)) = (t[i - 1], t[i]);
        g0 + (g1 - g0) * (hz / f0).log2() / (f1 / f0).log2()
    };
    10.0f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_the_derived_csv() {
        let csv = include_str!("../../../../docs/research/data/farfisa-proc-gain.csv");
        for (s, t) in [(0usize, &SUM0_DB[..]), (1, &SUM1_DB[..])] {
            let rows: Vec<(f32, f32)> = csv
                .lines()
                .skip(1)
                .map(|l| l.split(',').collect::<Vec<_>>())
                .filter(|f| f[0].parse::<usize>().unwrap() == s)
                .map(|f| (f[1].parse().unwrap(), f[2].parse().unwrap()))
                .collect();
            assert_eq!(rows.len(), t.len());
            for (r, p) in rows.iter().zip(t) {
                assert!((r.0 - p.0).abs() < 0.01 && (r.1 - p.1).abs() < 0.01);
            }
            assert!(t.windows(2).all(|w| w[0].0 < w[1].0));
        }
    }

    #[test]
    fn gain_interpolates_in_log_frequency_and_clamps() {
        for (s, t) in [(0usize, &SUM0_DB[..]), (1, &SUM1_DB[..])] {
            for &(hz, db) in t {
                let g = 20.0 * farfisa_proc_gain(s, hz).log10();
                assert!((g - db).abs() < 1e-3, "{s} {hz}: {g} vs {db}");
            }
            assert_eq!(farfisa_proc_gain(s, 1.0), farfisa_proc_gain(s, t[0].0));
            assert_eq!(
                farfisa_proc_gain(s, 1e6),
                farfisa_proc_gain(s, t[t.len() - 1].0)
            );
        }
        // sum 0 is about -5 dB at 275 Hz, +1 dB near 900 Hz and falls above 1 kHz
        let (a, b) = (farfisa_proc_gain(0, 275.0), farfisa_proc_gain(0, 900.0));
        assert!(b > 1.5 * a);
        assert!(farfisa_proc_gain(0, 6000.0) < 0.1);
    }
}
