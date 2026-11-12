//! The stack probe of `docs/research/sharc-seq/` against the simulator.
//!
//! VERIFIED: `stack_measured.txt` is the output of `stack.asm` (generator `gen_stack_probe.py`) on
//! the CCES 2.12.1 ADSP-21489 simulator: PCSTKP, PCSTK, LADDR, CURLCNTR and STKY snapshots along a
//! call chain of 30 levels, nested counter loops, 15 status-stack pushes and pops, `PUSH`/`POP`
//! of the loop and PC stacks, and LADDR/CURLCNTR values inside loops of 1 to 5 instructions. The
//! test runs the same machine code on a `Core`; `stack_names.json` names each output word.
use super::probe_fixture::{measured, Fixture};
use super::Core;

const FIXTURE: &str = include_str!("../../../../docs/research/sharc-seq/stack_fixture.txt");
const MEASURED: &str = include_str!("../../../../docs/research/sharc-seq/stack_measured.txt");
const NAMES: &str = include_str!("../../../../docs/research/sharc-seq/stack_names.json");

#[test]
fn model_runs_the_stack_probe_like_the_simulator() {
    let f = Fixture::parse(FIXTURE);
    let want = measured(MEASURED);
    let mut c = Core::new(&[]);
    let got = f
        .run(&mut c, 2_000_000)
        .unwrap_or_else(|e| panic!("model did not finish the probe: {e}"));
    assert_eq!(got.len(), want.len());
    let names: Vec<&str> = NAMES.split('"').skip(1).step_by(2).collect();
    let name = |i: usize| {
        names
            .iter()
            .skip_while(|n| **n != "names")
            .nth(i + 1)
            .copied()
            .unwrap_or("?")
    };
    // NOT MODELLED (simulator timing): CURLCNTR and LADDR read in the first instruction of a loop
    // body of 1 to 4 instructions (`stk*`, see docs/research/sharc-stacks.md: the loop leaves the
    // stack when its last instruction is fetched, three instructions before it runs), and the
    // STKY empty bits of the first loop of a nest, which the simulator updates late.
    let skipped = |i: usize| name(i).starts_with("stk") || name(i) == "nest1.stky";
    let bad: Vec<String> = (0..got.len())
        .filter(|&i| !skipped(i) && got[i] != want[i])
        .map(|i| {
            format!(
                "word {i} {}: got {:#x} want {:#x}",
                name(i),
                got[i],
                want[i]
            )
        })
        .collect();
    let shown = bad[..bad.len().min(60)].join("\n");
    assert!(
        bad.is_empty(),
        "{} words differ (first shown):\n{shown}",
        bad.len()
    );
}
