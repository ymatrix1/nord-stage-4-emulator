//! JIT-vs-interpreter lockstep over a whole system run (D119): the passes of
//! [`super::system::run_system`] (ARM messages before their pass, ping-pong toggle, P4I or mailbox
//! start) are run on two cores, the interpreter and the Cranelift JIT, and
//! [`super::jit_diff::diff_pass_with`] compares full architectural state plus SRAM every `chunk`
//! steps. The first divergence ends the run. Read-only use of `jit_diff`; opt-in, no default change.
use super::jit_diff::DiffMismatch;
use super::msg::{deliver, Msg};
use super::pass::PassConfig;
use super::system::set_ping_pong;
use crate::core::Core;

/// What a lockstep system run replays.
#[derive(Clone, Copy)]
pub struct SysRun<'a> {
    pub cfg: &'a PassConfig,
    pub msgs: &'a [Msg],
    pub total_passes: u32,
    pub ping_pong: Option<(u32, u32, u32)>,
    /// Steps between state comparisons.
    pub chunk: u64,
    /// Also compare PEy flags, float extensions, ASTAT extras, STKY and interrupt latches.
    pub strict: bool,
}

/// Outcome of a lockstep system run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysDiffReport {
    /// Passes that matched completely.
    pub passes_ok: u32,
    /// Steps of every completed pass, in order.
    pub steps: Vec<u64>,
    /// Union of the executed pc ranges over all passes (inclusive `(first, last)`).
    pub coverage: Vec<(u32, u32)>,
    /// `(pass, mismatch)` of the first divergence, if any.
    pub first_divergence: Option<(u32, DiffMismatch)>,
}

/// Merges inclusive `(first, last)` ranges: sorted, overlapping or adjacent ranges joined.
pub fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (a, b) in ranges {
        match out.last_mut() {
            Some((_, last)) if a <= last.saturating_add(1) => *last = (*last).max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Total number of pcs covered by inclusive ranges.
pub fn range_len(ranges: &[(u32, u32)]) -> u64 {
    ranges.iter().map(|&(a, b)| u64::from(b - a) + 1).sum()
}

/// Runs `total_passes` passes on `interp` (interpreter) and `jit` (`run_other`) in lockstep with
/// the same messages and ping-pong pointer, comparing state every `chunk` steps (`strict` adds the
/// PEy flags, float extensions, ASTAT extras, STKY and interrupt latches). Both cores must start
/// from identical state.
pub fn diff_system_with(
    interp: &mut Core,
    jit: &mut Core,
    run: &SysRun,
    run_other: &mut dyn FnMut(&mut Core, u64) -> Option<crate::core::Fault>,
) -> Result<SysDiffReport, String> {
    let SysRun {
        cfg,
        msgs,
        total_passes,
        ping_pong,
        chunk,
        strict,
    } = *run;
    let mut due: Vec<&Msg> = msgs.iter().collect();
    due.sort_by_key(|m| m.pass);
    let (mut next, mut cover) = (0, Vec::new());
    let mut report = SysDiffReport {
        passes_ok: 0,
        steps: Vec::new(),
        coverage: Vec::new(),
        first_divergence: None,
    };
    for p in 0..total_passes {
        for c in [&mut *interp, &mut *jit] {
            if let Some(pp) = ping_pong {
                set_ping_pong(c, pp, p);
            }
        }
        while next < due.len() && due[next].pass <= p {
            deliver(interp, 0, due[next])?;
            deliver(jit, 0, due[next])?;
            next += 1;
        }
        match super::jit_diff::diff_pass_with(interp, jit, cfg, chunk, true, strict, run_other) {
            Ok(r) => {
                report.passes_ok += 1;
                report.steps.push(r.steps);
                cover.extend(r.coverage);
            }
            Err(m) => {
                report.first_divergence = Some((p, m));
                break;
            }
        }
    }
    report.coverage = merge_ranges(cover);
    Ok(report)
}

/// [`diff_system_with`] for the Cranelift JIT.
#[cfg(feature = "jit")]
pub fn diff_system(
    interp: &mut Core,
    jit: &mut Core,
    run: &SysRun,
) -> Result<SysDiffReport, String> {
    diff_system_with(interp, jit, run, &mut |c, n| c.run_jit(n))
}

#[cfg(test)]
mod tests {
    use super::super::pass::tests::clearing_core;
    use super::*;

    fn cfg() -> PassConfig {
        PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0002,
            max_steps: 10,
            irq: None,
        }
    }

    #[test]
    fn ranges_merge_and_count() {
        let m = merge_ranges(vec![(10, 12), (1, 3), (4, 5), (11, 20), (30, 30)]);
        assert_eq!(m, vec![(1, 5), (10, 20), (30, 30)]);
        assert_eq!(range_len(&m), 5 + 11 + 1);
        assert!(merge_ranges(vec![]).is_empty());
    }

    #[test]
    fn identical_cores_match_pass_after_pass() {
        let (mut a, mut b) = (clearing_core(), clearing_core());
        // `run_other` is the interpreter itself: the harness plumbing (messages, ping-pong,
        // coverage) is tested here, the JIT is covered by the jit_diff tests
        let msgs = [Msg {
            pass: 0,
            word1: 0xB_0010,
            word2: 7,
        }];
        let cfg = cfg();
        let run = SysRun {
            cfg: &cfg,
            msgs: &msgs,
            total_passes: 1,
            ping_pong: Some((0xB_0158, 1, 2)),
            chunk: 4,
            strict: true,
        };
        let r = diff_system_with(&mut a, &mut b, &run, &mut |c, n| c.run(n)).unwrap();
        // the tiny core never leaves its poll pc, so the pass runs to the step limit
        assert_eq!((r.passes_ok, r.steps.as_slice()), (1, &[10u64][..]));
        assert_eq!(r.coverage.first().map(|r| r.0), Some(0x12_0000));
        assert!(r.first_divergence.is_none());
        assert_eq!(b.dm_read(0xB_0010), Ok(7));
        assert_eq!(b.dm_read(0xB_0158), Ok(1));
    }

    #[test]
    fn a_wrong_other_core_reports_the_first_divergence() {
        let (mut a, mut b) = (clearing_core(), clearing_core());
        // the "other" core runs nothing: pc and steps differ after the first chunk
        let cfg = cfg();
        let run = SysRun {
            cfg: &cfg,
            msgs: &[],
            total_passes: 2,
            ping_pong: None,
            chunk: 1,
            strict: false,
        };
        let r = diff_system_with(&mut a, &mut b, &run, &mut |_, _| None).unwrap();
        assert_eq!(r.passes_ok, 0);
        let (pass, m) = r.first_divergence.unwrap();
        assert_eq!(pass, 0);
        assert!(m.detail.contains("pc") || m.detail.contains("steps"), "{m}");
    }

    #[cfg(feature = "jit")]
    #[test]
    fn the_jit_matches_the_interpreter_on_the_tiny_core() {
        let (mut a, mut b) = (clearing_core(), clearing_core());
        let cfg = cfg();
        let run = SysRun {
            cfg: &cfg,
            msgs: &[],
            total_passes: 1,
            ping_pong: None,
            chunk: 4,
            strict: true,
        };
        let r = diff_system(&mut a, &mut b, &run).unwrap();
        assert_eq!(r.passes_ok, 1, "{:?}", r.first_divergence);
    }
}
