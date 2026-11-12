//! The arithmetic-loop probes of `docs/research/sharc-seq/` against the simulator.
//!
//! VERIFIED: `loops_measured.txt` is the output of `loops.asm` (generator `gen_loops_probe.py`)
//! on the CCES 2.12.1 ADSP-21489 simulator: for every combination of loop kind, condition, body
//! length 1..6, position of the flag-setting instruction and start value, the final value of a
//! counter decremented once per pass (the number of passes), stored by two instructions right
//! after the loop (an unexecuted one leaves its zero-initialised word). `entry_measured.txt` does
//! the same for conditions that already hold at loop entry with six stores after the loop. The
//! tests run the same machine code on a `Core`.
use super::probe_fixture::{measured, Fixture};
use super::Core;

const FIXTURE: &str = include_str!("../../../../docs/research/sharc-seq/loops_fixture.txt");
const MEASURED: &str = include_str!("../../../../docs/research/sharc-seq/loops_measured.txt");
const ENTRY_FIXTURE: &str = include_str!("../../../../docs/research/sharc-seq/entry_fixture.txt");
const ENTRY_MEASURED: &str = include_str!("../../../../docs/research/sharc-seq/entry_measured.txt");

/// Runs `fixture` and compares each case's `stores` output words with the measured ones.
fn check(fixture: &str, measured_text: &str, stores: usize) -> Vec<String> {
    let f = Fixture::parse(fixture);
    let want = measured(measured_text);
    let mut c = Core::new(&[]);
    let got = match f.run(&mut c, 5_000_000) {
        Ok(g) => g,
        Err(e) => panic!("model did not finish the probe: {e}"),
    };
    assert_eq!(got.len(), want.len());
    let mut bad: Vec<String> = (0..got.len() / stores)
        .filter(|&i| got[stores * i..stores * (i + 1)] != want[stores * i..stores * (i + 1)])
        .map(|i| {
            format!(
                "case {i}: got {:x?} want {:x?}",
                &got[stores * i..stores * (i + 1)],
                &want[stores * i..stores * (i + 1)]
            )
        })
        .collect();
    if got.last() != want.last() {
        bad.push("done flag differs".into());
    }
    bad
}

fn assert_no_difference(bad: &[String]) {
    let shown = bad[..bad.len().min(40)].join("\n");
    assert!(
        bad.is_empty(),
        "{} cases differ (first shown):\n{shown}",
        bad.len()
    );
}

#[test]
fn model_runs_the_loop_probe_like_the_simulator() {
    assert_no_difference(&check(FIXTURE, MEASURED, 2));
}

#[test]
fn model_runs_the_entry_probe_like_the_simulator() {
    assert_no_difference(&check(ENTRY_FIXTURE, ENTRY_MEASURED, 6));
}
