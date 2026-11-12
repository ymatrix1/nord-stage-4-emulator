//! The SIMD (PEYEN) probe of `docs/research/sharc-seq/` against the simulator.
//!
//! VERIFIED: `simd_measured.txt` is the output of `simd.asm` (generator `gen_simd_probe.py`) on the
//! CCES 2.12.1 ADSP-21489 simulator, run headless with `sim_run.py`: 9 x 8 operand pairs
//! (ordinary, tie-prone, extreme, NaN/Inf) through float and fixed add, subtract, multiply,
//! average, multiply-accumulate and the dual-compute forms with SIMD on, RND32 set. Every result register
//! and ASTAT is stored as a (PEx, PEy) pair; `simd_names.json` gives the operations in order.
use super::probe_fixture::{measured, Fixture};
use super::Core;

const FIXTURE: &str = include_str!("../../../../docs/research/sharc-seq/simd_fixture.txt");
const MEASURED: &str = include_str!("../../../../docs/research/sharc-seq/simd_measured.txt");
const NAMES: &str = include_str!("../../../../docs/research/sharc-seq/simd_names.json");

#[test]
fn model_runs_the_simd_probe_like_the_simulator() {
    let f = Fixture::parse(FIXTURE);
    let want = measured(MEASURED);
    let mut c = Core::new(&[]);
    let got = f
        .run(&mut c, 2_000_000)
        .unwrap_or_else(|e| panic!("model did not finish the probe: {e}"));
    assert_eq!(got.len(), want.len());
    let ops: Vec<&str> = NAMES
        .split("\"ops\":")
        .nth(1)
        .unwrap_or("")
        .split(']')
        .next()
        .unwrap_or("")
        .split('"')
        .skip(1)
        .step_by(2)
        .collect();
    let per_op = 8;
    let per_iter = per_op * ops.len();
    let regs = ["r4", "r8", "r6", "astat"];
    let bad: Vec<String> = (0..got.len() - 1)
        .filter(|&i| got[i] != want[i])
        .map(|i| {
            let (iter, k) = (i / per_iter, i % per_iter);
            format!(
                "pair a{} b{} {} {}.{}: got {:#x} want {:#x}",
                iter / 8,
                iter % 8,
                ops[k / per_op],
                regs[k % per_op / 2],
                ["x", "y"][k % 2],
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
    assert_eq!(got.last(), want.last());
}
