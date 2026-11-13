//! Regression fixture of the firmware-instruction census diff.
//!
//! VERIFIED: `census_measured.txt` holds the CCES 2.12.1 ADSP-21489 simulator's output for every case
//! of `census_cases.txt` (`docs/research/sharc-seq/census_regress.py` picks them from a full
//! `census_diff.py` run): one instruction per firmware shape that ever showed a model-versus-simulator
//! difference (long-word moves, SIMD register and memory moves, per-element conditions, shifter flags,
//! sticky and ASTAT writes, multifunction flags ...) with six random core states each, plus a sweep of
//! every condition code over all combinations of AZ AV AN AC and AF. Every line is the dumped state
//! (107 words) and the scratch words the instruction changed.
use super::census::{compact, run_cases, scratch_of};

const FIXTURE: &str = include_str!("../../../../docs/research/sharc-seq/census_fixture.txt");
const CASES: &str = include_str!("../../../../docs/research/sharc-seq/census_cases.txt");
const MEASURED: &str = include_str!("../../../../docs/research/sharc-seq/census_measured.txt");
const N_STATE: usize = 107;

#[test]
fn model_matches_the_simulator_on_every_census_regression_case() {
    let scr = scratch_of(CASES);
    let got = run_cases(FIXTURE, CASES).unwrap();
    let want: Vec<&str> = MEASURED.lines().collect();
    let got: Vec<String> = got.lines().map(|l| compact(l, &scr, N_STATE)).collect();
    assert_eq!(got.len(), want.len(), "one output line per case");
    let cases: Vec<&str> = CASES.lines().skip(1).collect();
    let bad: Vec<String> = (0..got.len())
        .filter(|&i| got[i] != want[i])
        .map(|i| {
            format!(
                "case {i} ({}): got {} want {}",
                &cases[i][..12],
                &got[i][..40],
                &want[i][..40]
            )
        })
        .collect();
    let shown = bad[..bad.len().min(20)].join("\n");
    assert!(
        bad.is_empty(),
        "{} cases differ (first shown):\n{shown}",
        bad.len()
    );
}
