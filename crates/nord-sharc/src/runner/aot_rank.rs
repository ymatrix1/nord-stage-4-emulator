//! Ranking of the straight-line regions of a real pass for the multi-block AOT study.
//!
//! The interpreter runs boot, warm-up and measured passes while every executed step is counted
//! and timed per pc (the clock read around each step inflates every step by about the same
//! amount, so shares are meaningful, absolute times are not). From the observed control flow the
//! executed code is cut into *chains* (straight-line stretches: each step always falls through to
//! the next executed pc, and no jump lands inside), and each chain into *runs* of instructions the
//! lowering supports. Runs are ranked by steps; the instructions that break runs are summed by
//! shape so the lowering gaps are known.
use super::aot_hot::AotRun;
use super::msg;
use crate::core::{Core, InsnClass};
use std::collections::BTreeMap;
use std::time::Instant;

/// Addressable pc range of the per-pc tables (VISA parcels).
const PC_SPACE: usize = 1 << 22;

/// Per-pc counters of the measured passes.
pub struct Profile {
    pub count: Vec<u32>,
    pub ns: Vec<u64>,
    /// Parcels of the instruction at this pc (0 = never executed).
    pub len: Vec<u8>,
    /// Some execution continued somewhere else than `pc + len` (a taken or possible jump).
    pub nonseq: Vec<bool>,
    /// Some execution arrived from somewhere else than the previous instruction.
    pub leader: Vec<bool>,
    pub passes: u32,
    pub steps: u64,
}

impl Profile {
    fn new(passes: u32) -> Profile {
        Profile {
            count: vec![0; PC_SPACE],
            ns: vec![0; PC_SPACE],
            len: vec![0; PC_SPACE],
            nonseq: vec![false; PC_SPACE],
            leader: vec![false; PC_SPACE],
            passes,
            steps: 0,
        }
    }
}

/// A straight-line run of lowerable instructions.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub start: u32,
    /// Start pc of the last instruction.
    pub last: u32,
    pub insns: u32,
    /// Times one pass enters the run (mean).
    pub calls_per_pass: f64,
    pub steps_per_pass: f64,
    pub ns_per_pass: f64,
    /// Steps per pass of the run's instructions that stay interpreter items inside the block.
    pub interp_steps_per_pass: f64,
}

/// Everything the ranking found.
#[derive(Debug, Clone, Default)]
pub struct RankReport {
    pub passes: u32,
    pub steps_per_pass: f64,
    pub ns_per_pass: f64,
    /// Runs of at least `min_len` instructions, most steps first.
    pub runs: Vec<Run>,
    /// Steps per pass in lowerable runs shorter than `min_len`.
    pub short_steps: f64,
    /// Steps per pass of every shape that ends a block (never lowered): `(shape, steps, ns)`.
    pub gaps: Vec<(String, f64, f64)>,
    /// Steps per pass of every shape run as an interpreter item inside blocks (all executions,
    /// also those in runs too short to be ranked): `(shape, steps, ns)`.
    pub interp: Vec<(String, f64, f64)>,
}

/// Cuts the profile into chains and runs. `class(pc)` says how the lowering sees the instruction
/// at `pc`: native and interpreter-item instructions extend a run, a break ends it.
pub fn build_report(
    p: &Profile,
    class: &mut dyn FnMut(u32) -> InsnClass,
    min_len: u32,
) -> RankReport {
    let n = f64::from(p.passes.max(1));
    let mut rep = RankReport {
        passes: p.passes,
        steps_per_pass: p.steps as f64 / n,
        ns_per_pass: p.ns.iter().sum::<u64>() as f64 / n,
        ..RankReport::default()
    };
    let mut gaps: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut interp: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut seen = vec![false; PC_SPACE];
    for s in 0..PC_SPACE {
        if p.count[s] == 0 || seen[s] {
            continue;
        }
        // a chain starts at a pc nothing falls into (or that a jump also reaches); walk it
        let mut run: Option<OpenRun> = None;
        let mut pc = s;
        loop {
            seen[pc] = true;
            let calls = f64::from(p.count[pc]);
            let (steps, ns) = (calls / n, p.ns[pc] as f64 / n);
            match class(pc as u32) {
                c @ (InsnClass::Native | InsnClass::Interp(_)) => {
                    let r = run.get_or_insert(OpenRun {
                        start: pc as u32,
                        ..OpenRun::default()
                    });
                    (r.last, r.insns, r.ns) = (pc as u32, r.insns + 1, r.ns + ns);
                    if let InsnClass::Interp(shape) = c {
                        r.interp_steps += steps;
                        let g = interp.entry(shape).or_default();
                        (g.0, g.1) = (g.0 + steps, g.1 + ns);
                    }
                }
                InsnClass::Break(why) => {
                    close_run(&mut rep, &mut run, p, min_len, n);
                    let g = gaps.entry(why).or_default();
                    (g.0, g.1) = (g.0 + steps, g.1 + ns);
                }
            }
            let next = pc + usize::from(p.len[pc]);
            if p.nonseq[pc]
                || next >= PC_SPACE
                || p.count[next] == 0
                || p.leader[next]
                || seen[next]
            {
                break;
            }
            pc = next;
        }
        close_run(&mut rep, &mut run, p, min_len, n);
    }
    rep.runs
        .sort_by_key(|r| std::cmp::Reverse((r.steps_per_pass * 1000.0) as u64));
    let sorted = |m: BTreeMap<String, (f64, f64)>| {
        let mut g: Vec<_> = m.into_iter().map(|(k, (s, t))| (k, s, t)).collect();
        g.sort_by_key(|x| std::cmp::Reverse((x.1 * 1000.0) as u64));
        g
    };
    rep.gaps = sorted(gaps);
    rep.interp = sorted(interp);
    rep
}

/// A run being walked: start, last instruction start, instructions, ns, interpreter-item steps.
#[derive(Default)]
struct OpenRun {
    start: u32,
    last: u32,
    insns: u32,
    ns: f64,
    interp_steps: f64,
}

/// Records the open run (entered `count[start]` times per pass) if it is long enough.
fn close_run(rep: &mut RankReport, run: &mut Option<OpenRun>, p: &Profile, min_len: u32, n: f64) {
    let Some(OpenRun {
        start,
        last,
        insns,
        ns,
        interp_steps,
    }) = run.take()
    else {
        return;
    };
    let calls = f64::from(p.count[start as usize]) / n;
    let steps = calls * f64::from(insns);
    if insns >= min_len {
        rep.runs.push(Run {
            start,
            last,
            insns,
            calls_per_pass: calls,
            steps_per_pass: steps,
            ns_per_pass: ns,
            interp_steps_per_pass: interp_steps,
        });
    } else {
        rep.short_steps += steps;
    }
}

/// Runs the pass loop of `r` on the interpreter, profiling every step of the measured passes.
pub fn profile(r: &AotRun) -> Result<(Core, Profile), String> {
    let mut c = Core::new(r.image);
    if let Some(pc) = r.pc {
        c.pc = pc;
    }
    c.lenient = r.lenient;
    for &(a, v) in r.devs {
        c.preset_device(a, v);
    }
    c.run(r.boot);
    let mut prof = Profile::new(r.passes);
    for pass in 0..r.warmup + r.passes {
        for m in r.msgs.iter().filter(|m| m.pass == pass) {
            msg::deliver(&mut c, r.msg_base, m)?;
        }
        let measured = pass >= r.warmup;
        pass_profiled(&mut c, r, measured.then_some(&mut prof))?;
    }
    Ok((c, prof))
}

fn pass_profiled(c: &mut Core, r: &AotRun, mut prof: Option<&mut Profile>) -> Result<(), String> {
    c.dm_write(r.cfg.trigger, 1)
        .map_err(|a| format!("bad trigger 0x{a:x}"))?;
    let limit = c.steps + r.cfg.max_steps;
    let mut left_idle = false;
    let mut prev: Option<usize> = None;
    while c.steps < limit {
        let pc = c.pc;
        let t0 = Instant::now();
        c.step().map_err(|f| format!("fault: {f:?}"))?;
        let ns = t0.elapsed().as_nanos() as u64;
        if let Some(p) = prof.as_deref_mut() {
            let i = pc as usize;
            let len = c.aot_insn_class(pc).0 as u8;
            p.count[i] += 1;
            p.ns[i] += ns;
            p.len[i] = len;
            p.steps += 1;
            match prev {
                Some(q) if q + usize::from(p.len[q]) == i => {}
                Some(q) => {
                    p.leader[i] = true;
                    p.nonseq[q] = true;
                }
                None => p.leader[i] = true,
            }
            prev = Some(i);
        }
        if c.pc != r.cfg.idle_pc {
            left_idle = true;
        } else if left_idle && c.dm_read(r.cfg.trigger).unwrap_or(1) == 0 {
            break;
        }
    }
    Ok(())
}

/// Text of the report: the pass, the top `top` runs with cumulative coverage, and the gaps.
pub fn format_report(rep: &RankReport, top: usize) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "pass: {:.1} steps, {:.1} us profiled interpreter time ({} passes); lowerable runs >= min: {}, short runs: {:.1} steps/pass",
        rep.steps_per_pass,
        rep.ns_per_pass / 1e3,
        rep.passes,
        rep.runs.len(),
        rep.short_steps
    );
    let (mut cs, mut ct) = (0.0, 0.0);
    for (i, r) in rep.runs.iter().take(top).enumerate() {
        cs += r.steps_per_pass;
        ct += r.ns_per_pass;
        let _ = writeln!(
            s,
            "run {:3}: 0x{:x}..0x{:x} {:3} insns x {:6.2}/pass = {:7.1} steps ({:4.1}% pass, cum {:4.1}%), {:7.1} ns ({:4.1}% time, cum {:4.1}%)",
            i + 1,
            r.start,
            r.last,
            r.insns,
            r.calls_per_pass,
            r.steps_per_pass,
            100.0 * r.steps_per_pass / rep.steps_per_pass,
            100.0 * cs / rep.steps_per_pass,
            r.ns_per_pass,
            100.0 * r.ns_per_pass / rep.ns_per_pass,
            100.0 * ct / rep.ns_per_pass,
        );
    }
    for (shape, steps, ns) in &rep.interp {
        let _ = writeln!(
            s,
            "interp {shape:26} {steps:8.1} steps/pass ({:4.1}%) {:8.1} ns ({:4.1}% time)",
            100.0 * steps / rep.steps_per_pass,
            ns,
            100.0 * ns / rep.ns_per_pass
        );
    }
    for (shape, steps, ns) in &rep.gaps {
        let _ = writeln!(
            s,
            "gap {shape:28} {steps:8.1} steps/pass ({:4.1}%) {:8.1} ns ({:4.1}% time)",
            100.0 * steps / rep.steps_per_pass,
            ns,
            100.0 * ns / rep.ns_per_pass
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Straight line 100..=104 (1-parcel insns), then a jump at 105 to 200; 200..=201 a second
    /// chain entered by the jump; pc 150 is executed only after a jump (a leader).
    fn profile_of_two_chains() -> Profile {
        let mut p = Profile::new(2);
        for pc in (100..=105).chain(200..=201) {
            p.count[pc] = 10;
            p.ns[pc] = 100;
            p.len[pc] = 1;
        }
        p.steps = 80;
        p.nonseq[105] = true;
        p.leader[100] = true;
        p.leader[200] = true;
        p
    }

    #[test]
    fn chains_split_at_jumps_and_unlowerable_ops_become_gaps() {
        let p = profile_of_two_chains();
        let mut shape = |pc: u32| {
            if pc == 105 {
                InsnClass::Break("Flow::JumpDirect".to_string())
            } else {
                InsnClass::Native
            }
        };
        let rep = build_report(&p, &mut shape, 3);
        // run 100..=104 (5 insns, 10 entries over 2 passes) and the short run 200..=201
        assert_eq!(rep.runs.len(), 1);
        let r = &rep.runs[0];
        assert_eq!((r.start, r.last, r.insns), (100, 104, 5));
        assert_eq!((r.calls_per_pass, r.steps_per_pass), (5.0, 25.0));
        assert_eq!(rep.short_steps, 10.0);
        assert_eq!(rep.gaps, vec![("Flow::JumpDirect".to_string(), 5.0, 50.0)]);
        assert_eq!(rep.steps_per_pass, 40.0);
    }

    #[test]
    fn a_leader_inside_a_chain_splits_it_and_unsupported_ops_cut_runs() {
        let mut p = profile_of_two_chains();
        p.leader[103] = true; // a jump lands on 103
        let mut shape = |_| InsnClass::Native;
        let rep = build_report(&p, &mut shape, 2);
        let starts: Vec<u32> = rep.runs.iter().map(|r| r.start).collect();
        assert!(starts.contains(&100) && starts.contains(&103), "{starts:?}");
        let mut cut = |pc: u32| {
            if pc == 102 {
                InsnClass::Break("Move::Other".into())
            } else {
                InsnClass::Native
            }
        };
        let rep = build_report(&profile_of_two_chains(), &mut cut, 2);
        let spans: Vec<(u32, u32)> = rep.runs.iter().map(|r| (r.start, r.last)).collect();
        assert!(
            spans.contains(&(100, 101)) && spans.contains(&(103, 105)),
            "{spans:?}"
        );
        assert!(format_report(&rep, 5).contains("gap Move::Other"));
    }

    #[test]
    fn interpreter_items_extend_runs_and_are_summed_by_shape() {
        let mut interp_at_102 = |pc: u32| {
            if pc == 102 {
                InsnClass::Interp("Move::DirectMem".into())
            } else {
                InsnClass::Native
            }
        };
        let rep = build_report(&profile_of_two_chains(), &mut interp_at_102, 3);
        let r = rep.runs.iter().find(|r| r.start == 100).unwrap();
        assert_eq!((r.last, r.insns), (105, 6));
        assert_eq!(r.interp_steps_per_pass, 5.0);
        assert_eq!(rep.interp, vec![("Move::DirectMem".to_string(), 5.0, 50.0)]);
        assert!(format_report(&rep, 5).contains("interp Move::DirectMem"));
    }
}
