//! Differential check and benchmark of the hot-block AOT prototype ([`Core::enable_aot_hot`]):
//! the interpreter ([`Core::run`], oracle) against the same image run with the generated block
//! ([`Core::run_aot`]), over boot and audio passes, comparing the strict state after every chunk.
use super::jit_diff::{diff_pass_with, diff_run_with};
use super::msg::{self, Msg};
use super::pass::PassConfig;
use crate::core::{AotHotStats, Core};
use std::collections::HashMap;
use std::time::Instant;

/// Everything one AOT differential or benchmark run needs.
pub struct AotRun<'a> {
    pub image: &'a [u8],
    pub pc: Option<u32>,
    pub devs: &'a [(u32, u32)],
    pub msgs: &'a [Msg],
    pub msg_base: u32,
    pub boot: u64,
    pub passes: u32,
    pub warmup: u32,
    pub chunk: u64,
    pub cfg: PassConfig,
    /// Block ranges: instruction starts `start..=last` of each enabled block.
    pub ranges: &'a [(u32, u32)],
    pub lenient: bool,
}

impl AotRun<'_> {
    fn core(&self, aot: bool) -> Result<Core, String> {
        let mut c = Core::new(self.image);
        if let Some(pc) = self.pc {
            c.pc = pc;
        }
        c.lenient = self.lenient;
        for &(a, v) in self.devs {
            c.preset_device(a, v);
        }
        if aot {
            c.enable_aot_hot(self.ranges)?;
        }
        Ok(c)
    }

    fn deliver(&self, cores: &mut [&mut Core], pass: u32) -> Result<(), String> {
        for m in self.msgs.iter().filter(|m| m.pass == pass) {
            for c in cores.iter_mut() {
                msg::deliver(c, self.msg_base, m)?;
            }
        }
        Ok(())
    }
}

/// Result of a differential run that matched.
#[derive(Debug, Clone)]
pub struct AotDiffReport {
    pub boot_steps: u64,
    pub passes: u32,
    pub pass_steps: u64,
    /// Counters of the whole run (boot included).
    pub aot: AotHotStats,
    /// Counters of the audio passes only (boot excluded).
    pub aot_passes: AotHotStats,
    /// `(start, last, steps, native steps, counters)` of each enabled block, whole run.
    pub blocks: Vec<(u32, u32, u32, u32, AotHotStats)>,
}

/// Boots both cores, then runs `passes` audio passes in lockstep and compares the strict state
/// every `chunk` steps (no `--lenient` unless asked). The first divergence is the error.
pub fn diff(r: &AotRun) -> Result<AotDiffReport, String> {
    let (mut interp, mut aot) = (r.core(false)?, r.core(true)?);
    let mut run_aot = |c: &mut Core, n| c.run_aot(n);
    diff_run_with(&mut interp, &mut aot, r.boot, r.chunk, true, &mut run_aot)
        .map_err(|m| format!("boot: {m}"))?;
    let boot_steps = interp.steps;
    let boot_stats = aot.aot_hot_stats();
    let mut pass_steps = 0;
    for p in 0..r.passes {
        r.deliver(&mut [&mut interp, &mut aot], p)?;
        let rep = diff_pass_with(
            &mut interp,
            &mut aot,
            &r.cfg,
            r.chunk,
            false,
            true,
            &mut run_aot,
        )
        .map_err(|m| format!("pass {p}: {m}"))?;
        pass_steps += rep.steps;
    }
    Ok(AotDiffReport {
        boot_steps,
        passes: r.passes,
        pass_steps,
        aot: aot.aot_hot_stats(),
        aot_passes: aot.aot_hot_stats().since(&boot_stats),
        blocks: aot.aot_block_stats(),
    })
}

/// Timing of one pass loop variant over all measured passes.
#[derive(Debug, Clone, Copy, Default)]
pub struct PassTiming {
    pub steps: u64,
    pub total_s: f64,
    pub worst_s: f64,
}

/// Timing of the hot block alone (separate sweep with a clock read around each block).
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockTiming {
    pub calls: u64,
    pub steps: u64,
    pub total_s: f64,
}

/// Measured numbers of one state (idle, 1 voice, 8 voices).
#[derive(Debug, Clone, Copy)]
pub struct AotBench {
    pub passes: u32,
    /// Enabled blocks.
    pub blocks: usize,
    pub interp: PassTiming,
    pub aot: PassTiming,
    pub interp_block: BlockTiming,
    pub aot_block: BlockTiming,
    pub aot_stats: AotHotStats,
}

/// One pass with the same loop for both variants; `use_aot` only adds the block hook.
pub(super) fn run_pass(
    c: &mut Core,
    cfg: &PassConfig,
    use_aot: bool,
) -> Result<(u64, f64), String> {
    c.dm_write(cfg.trigger, 1)
        .map_err(|a| format!("bad trigger 0x{a:x}"))?;
    let (start, limit) = (c.steps, c.steps + cfg.max_steps);
    let mut left_idle = false;
    let t0 = Instant::now();
    while c.steps < limit {
        if use_aot && c.aot_try(limit) {
            if let Some(f) = c.aot_take_fault() {
                return Err(format!("fault: {f:?}"));
            }
        } else {
            c.step().map_err(|f| format!("fault: {f:?}"))?;
        }
        if c.pc != cfg.idle_pc {
            left_idle = true;
        } else if left_idle && c.dm_read(cfg.trigger).unwrap_or(1) == 0 {
            break;
        }
    }
    Ok((c.steps - start, t0.elapsed().as_secs_f64()))
}

/// [`run_pass`] reading the clock around each block: the interpreter times the `n` steps that
/// start at the block pc, the AOT core times the generated call.
fn run_pass_block_timed(
    c: &mut Core,
    cfg: &PassConfig,
    blocks: &HashMap<u32, u32>,
    use_aot: bool,
    bt: &mut BlockTiming,
) -> Result<(), String> {
    c.dm_write(cfg.trigger, 1)
        .map_err(|a| format!("bad trigger 0x{a:x}"))?;
    let limit = c.steps + cfg.max_steps;
    let mut left_idle = false;
    while c.steps < limit {
        let n = blocks
            .get(&c.pc)
            .copied()
            .filter(|&n| c.steps + u64::from(n) <= limit);
        if let Some(n) = n {
            let before = c.steps;
            let t0 = Instant::now();
            let ran = if use_aot {
                c.aot_try(limit)
            } else {
                c.run(u64::from(n)).is_none()
            };
            let dt = t0.elapsed().as_secs_f64();
            if ran {
                bt.calls += 1;
                bt.steps += c.steps - before;
                bt.total_s += dt;
            } else {
                c.step().map_err(|f| format!("fault: {f:?}"))?;
            }
        } else {
            c.step().map_err(|f| format!("fault: {f:?}"))?;
        }
        if c.pc != cfg.idle_pc {
            left_idle = true;
        } else if left_idle && c.dm_read(cfg.trigger).unwrap_or(1) == 0 {
            break;
        }
    }
    Ok(())
}

/// Warmed-up benchmark: whole-pass timings of interpreter and AOT (interleaved pass by pass),
/// then a block-timing sweep over the same number of passes.
/// `exact` = the final state of both cores must match (false for the approximate tier).
pub fn bench(r: &AotRun, exact: bool) -> Result<AotBench, String> {
    let (mut interp, mut aot) = (r.core(false)?, r.core(true)?);
    let ranges = aot.aot_hot_ranges();
    if ranges.is_empty() {
        return Err("no block".into());
    }
    let blocks: HashMap<u32, u32> = ranges.iter().map(|&(s, _, n)| (s, n)).collect();
    interp.run(r.boot);
    aot.run_aot(r.boot);
    if (interp.pc, interp.steps) != (aot.pc, aot.steps) {
        return Err("boot diverged in bench setup".into());
    }
    r.deliver(&mut [&mut interp, &mut aot], 0)?;
    for _ in 0..r.warmup {
        run_pass(&mut interp, &r.cfg, false)?;
        run_pass(&mut aot, &r.cfg, true)?;
    }
    let (mut ti, mut ta) = (PassTiming::default(), PassTiming::default());
    for _ in 0..r.passes {
        for (c, use_aot, t) in [(&mut interp, false, &mut ti), (&mut aot, true, &mut ta)] {
            let (s, dt) = run_pass(c, &r.cfg, use_aot)?;
            t.steps += s;
            t.total_s += dt;
            t.worst_s = t.worst_s.max(dt);
        }
    }
    if ti.steps != ta.steps {
        return Err(format!("step counts differ: {} vs {}", ti.steps, ta.steps));
    }
    let (mut bi, mut ba) = (BlockTiming::default(), BlockTiming::default());
    for _ in 0..r.passes {
        run_pass_block_timed(&mut interp, &r.cfg, &blocks, false, &mut bi)?;
        run_pass_block_timed(&mut aot, &r.cfg, &blocks, true, &mut ba)?;
    }
    if let Some(m) = interp.state_mismatch(&aot).filter(|_| exact) {
        return Err(format!("state diverged during bench: {m}"));
    }
    Ok(AotBench {
        passes: r.passes,
        blocks: ranges.len(),
        interp: ti,
        aot: ta,
        interp_block: bi,
        aot_block: ba,
        aot_stats: aot.aot_hot_stats(),
    })
}

impl AotBench {
    /// One report line (numbers for the research-note table) for a state label.
    pub fn report(&self, label: &str) -> String {
        let n = f64::from(self.passes);
        let ms = |s: f64| s / n * 1e3;
        let rate = |t: &PassTiming| t.steps as f64 / 1e6 / t.total_s;
        let blk = |b: &BlockTiming| (b.steps as f64 / n, b.total_s / n * 1e6);
        let (is, it) = blk(&self.interp_block);
        let (as_, at) = blk(&self.aot_block);
        let pass_steps = self.interp.steps as f64 / n;
        format!(
            "{label}: steps/pass={pass_steps:.1} | interp mean={:.3} ms worst={:.3} ms ({:.2} Msteps/s) | aot mean={:.3} ms worst={:.3} ms ({:.2} Msteps/s) | speedup x{:.3} | {} blocks: interp {is:.1} steps/pass ({:.1}% of pass) {it:.1} us/pass ({:.1}% of time) | aot {as_:.1} steps/pass {at:.1} us/pass | block speedup x{:.2} | aot calls={} misses={}",
            ms(self.interp.total_s),
            self.interp.worst_s * 1e3,
            rate(&self.interp),
            ms(self.aot.total_s),
            self.aot.worst_s * 1e3,
            rate(&self.aot),
            self.interp.total_s / self.aot.total_s,
            self.blocks,
            100.0 * is / pass_steps,
            100.0 * it / (ms(self.interp.total_s) * 1e3),
            it / at.max(1e-9),
            self.aot_stats.calls,
            self.aot_stats.misses,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_and_bench_refuse_without_a_generated_block() {
        let r = AotRun {
            image: &[],
            pc: None,
            devs: &[],
            msgs: &[],
            msg_base: 0,
            boot: 10,
            passes: 1,
            warmup: 0,
            chunk: 5,
            cfg: PassConfig {
                trigger: 0xB_00E2,
                idle_pc: 0x12_0000,
                max_steps: 100,
                irq: None,
            },
            ranges: &[(0x12_5021, 0x12_50aa)],
            lenient: false,
        };
        assert!(diff(&r).unwrap_err().contains("generated AOT block"));
        assert!(bench(&r, true).is_err());
    }

    #[test]
    fn report_formats_speedups_and_fractions() {
        let t = |steps, total_s, worst_s| PassTiming {
            steps,
            total_s,
            worst_s,
        };
        let b = AotBench {
            passes: 10,
            blocks: 79,
            interp: t(1000, 0.02, 0.003),
            aot: t(1000, 0.01, 0.002),
            interp_block: BlockTiming {
                calls: 10,
                steps: 790,
                total_s: 0.005,
            },
            aot_block: BlockTiming {
                calls: 10,
                steps: 790,
                total_s: 0.001,
            },
            aot_stats: AotHotStats::default(),
        };
        let text = b.report("idle");
        assert!(text.contains("speedup x2.000"), "{text}");
        assert!(text.contains("block speedup x5.00"), "{text}");
        assert!(text.contains("(79.0% of pass)"), "{text}");
    }
}
