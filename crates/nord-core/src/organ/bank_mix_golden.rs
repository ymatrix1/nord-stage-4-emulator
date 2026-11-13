//! Golden vectors for the two output lanes of the firmware mix stage: `ToneBank::advance_lanes`
//! against the Python model of `tools/organ_oracle_audio.py` (class of `model_render`, steady-state
//! gains, level 1) on a synthetic closed-form table.
//!
//! Regenerate with `python tools/organ_oracle_audio.py golden`. The model reproduces the SHARC
//! oracle chain (bank 0x122620 + mix stage 0x12268F) to 1.8e-5 relative rms
//! (`python tools/organ_oracle_audio.py verify`, `docs/research/organ-oracle-audio.md` section 3);
//! the vectors contain no firmware data.

pub(super) const COPY_INCS: [[u32; 2]; 6] = [
    [0x02000000, 0x02001000],
    [0x02468ACF, 0x02440000],
    [0x00633E00, 0x00635300],
    [0x0633E000, 0x06340000],
    [0x00001235, 0x00001300],
    [0x01234568, 0x01230000],
];
pub(super) const WEIGHTS: [f32; 6] = [1.0, 0.5, 0.25, 0.8, 0.3, 0.1];
pub(super) const OUT_EXACT: [[f32; 2]; 16] = [
    [-0.3049744, -0.3049744],
    [-0.19658229, -0.19658229],
    [-0.086230315, -0.086230315],
    [0.028016932, 0.028016932],
    [0.14468607, 0.14468607],
    [0.2596439, 0.2596439],
    [0.375539, 0.375539],
    [0.48855433, 0.48855433],
    [0.60048026, 0.60048026],
    [0.71051234, 0.71051234],
    [0.81391096, 0.81391096],
    [0.9138267, 0.9138267],
    [1.0085858, 1.0085858],
    [1.0970484, 1.0970484],
    [1.1815093, 1.1815093],
    [1.2585332, 1.2585332],
];
pub(super) const OUT_DETUNED: [[f32; 2]; 16] = [
    [-0.34309626, -0.34309626],
    [-0.2211551, -0.2211551],
    [-0.097009115, -0.097009115],
    [0.03151905, 0.03151905],
    [0.16245627, 0.16214071],
    [0.29177943, 0.29145944],
    [0.42215747, 0.4218335],
    [0.55012786, 0.550632],
    [0.6755404, 0.6755404],
    [0.7989304, 0.79853433],
    [0.9153152, 0.9149805],
    [1.0277191, 1.0273832],
    [1.1343225, 1.133986],
    [1.233843, 1.2335063],
    [1.3285261, 1.3278542],
    [1.4157178, 1.4155856],
];

#[cfg(test)]
mod tests {
    use super::super::firmware_bank::{ToneBank, DEFAULT_TUNE_WORD};
    use super::super::types::NUM_TONEWHEELS;
    use super::super::vibrato_bank::BANK_WHEELS;
    use super::*;

    fn lanes(detuned: bool) -> Vec<[f32; 2]> {
        let mut bank = ToneBank::new();
        bank.retune(&[0.0; NUM_TONEWHEELS], 48_000.0, detuned);
        for (w, inc) in COPY_INCS.iter().enumerate() {
            for (c, &word) in inc.iter().enumerate() {
                bank.set_increment(c, w, word, DEFAULT_TUNE_WORD);
            }
        }
        let (mut x, mut y) = ([0.0f32; BANK_WHEELS], [0.0f32; BANK_WHEELS]);
        (0..OUT_EXACT.len())
            .map(|_| {
                bank.advance_lanes(&mut x, &mut y);
                let sum = |l: &[f32; BANK_WHEELS]| {
                    WEIGHTS
                        .iter()
                        .zip(l.iter())
                        .map(|(w, v)| w * v)
                        .sum::<f32>()
                };
                [sum(&x), sum(&y)]
            })
            .collect()
    }

    fn check(got: &[[f32; 2]], want: &[[f32; 2]]) {
        for (f, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            for (l, (gv, wv)) in g.iter().zip(w.iter()).enumerate() {
                let tol = 1e-5 * wv.abs().max(0.1);
                assert!((gv - wv).abs() <= tol, "frame {f} lane {l}: {gv} vs {wv}");
            }
        }
    }

    #[test]
    fn exact_table_lanes_match_the_model() {
        check(&lanes(false), &OUT_EXACT);
    }

    #[test]
    fn detuned_table_lanes_match_the_model() {
        check(&lanes(true), &OUT_DETUNED);
    }

    #[test]
    fn exact_lanes_are_identical_and_detuned_differs_from_exact() {
        let e = lanes(false);
        assert!(e
            .iter()
            .all(|f| (f[0] - f[1]).abs() <= 1e-6 * f[0].abs().max(1.0)));
        let d = lanes(true);
        assert!(d
            .iter()
            .zip(e.iter())
            .any(|(a, b)| (a[0] - b[0]).abs() > 1e-3));
    }
}
