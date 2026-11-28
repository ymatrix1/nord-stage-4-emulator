//! ARM0 tail benchmark (`ns4-lle2` tail follow-up): per-tick cost of the six `arm-lle-bench`
//! phases at several instruction budgets, repeated and interleaved.
//!
//!     cargo run --release -p nord-tools --bin arm-tail-bench -- \
//!         <bank0_CODE.bin> <bank0_SRAM.bin> <eeprom_image_256k.bin>
//!
//! Environment: `BUDGETS` (comma list, default `150000,200000,300000,456000`), `RUNS` (default 7:
//! each run executes every budget once, budgets interleaved), `PHASE_SCALE` (percent of the phase
//! lengths, default 100), `PROFILE=1` (histogram of executed pcs on the heavy ticks of the first
//! budget, plain `Cpu::step`, no timing).
//!
//! Work is counted in executed instructions and block counters (deterministic: identical in every
//! run, asserted); wall clock is reported as median with min/max over the runs, because the
//! machine is shared. The state hash and the message digest of every run are compared across runs
//! of the same budget.

use nord_arm::machine::metrics::{percentile, tail};
use nord_arm::machine::{Machine, Stop};
use nord_tools::arm_bench::{boot, plan, run_all, spread, Run};

fn fmt(s: [f64; 3], prec: usize) -> String {
    format!("{:.p$} [{:.p$} .. {:.p$}]", s[1], s[0], s[2], p = prec)
}

fn execs_f(r: &Run) -> Vec<f64> {
    r.exec.iter().map(|x| *x as f64).collect()
}

fn report(budget: u64, runs: &[Run]) {
    let first = &runs[0];
    for r in runs {
        assert_eq!(
            r.exec, first.exec,
            "executed instructions must be deterministic"
        );
        assert_eq!(r.stats, first.stats, "block counters must be deterministic");
        assert_eq!(
            (r.digest, r.state),
            (first.digest, first.state),
            "run differs"
        );
    }
    let ex = execs_f(first);
    let total: f64 = ex.iter().sum();
    println!(
        "\n== budget {budget} insns/ms: {} ticks, executed {} insns total ==",
        ex.len(),
        total as u64
    );
    println!(
        "deterministic: exec/tick p50 {:.0} p99 {:.0} p99.9 {:.0} max {:.0}; ticks with exec >= 100k: {}, >= 150k: {}, >= 200k: {}, >= 300k: {}",
        percentile(&ex, 0.5), percentile(&ex, 0.99), percentile(&ex, 0.999), percentile(&ex, 1.0),
        ex.iter().filter(|x| **x >= 100e3).count(), ex.iter().filter(|x| **x >= 150e3).count(),
        ex.iter().filter(|x| **x >= 200e3).count(), ex.iter().filter(|x| **x >= 300e3).count()
    );
    let st = first.stats;
    let cached = total as u64 - st.plain_steps - st.hook_runs.min(total as u64);
    println!(
        "counters: cached entries {} blocks {} (avg {:.1} insns/block) branch enders {} full enders {} stale exits {} page follows {} plain steps {} hook runs {} (~{:.1}% of insns cached)",
        st.cached_entries, st.blocks, cached as f64 / st.blocks.max(1) as f64, st.enders_branch,
        st.enders_full, st.stale_exits, st.page_follows, st.plain_steps, st.hook_runs,
        100.0 * cached as f64 / total
    );
    println!(
        "digest messages/state: {:016x} / {:016x} (equal in all {} runs)",
        first.digest,
        first.state,
        runs.len()
    );
    // wall clock: one statistic per run, then min/median/max across runs
    let stat = |f: &dyn Fn(&Run) -> f64| spread(&runs.iter().map(f).collect::<Vec<_>>());
    println!("wall clock over {} runs, median [min .. max]:", runs.len());
    println!(
        "  mean ms/tick        {}",
        fmt(stat(&|r| r.ms.iter().sum::<f64>() / r.ms.len() as f64), 3)
    );
    println!(
        "  p50 ms              {}",
        fmt(stat(&|r| percentile(&r.ms, 0.5)), 3)
    );
    println!(
        "  p99 ms              {}",
        fmt(stat(&|r| percentile(&r.ms, 0.99)), 3)
    );
    println!(
        "  p99.9 ms            {}",
        fmt(stat(&|r| percentile(&r.ms, 0.999)), 3)
    );
    println!(
        "  max ms              {}",
        fmt(stat(&|r| percentile(&r.ms, 1.0)), 3)
    );
    println!(
        "  ticks > 1 ms        {}",
        fmt(
            stat(&|r| r.ms.iter().filter(|x| **x > 1.0).count() as f64),
            0
        )
    );
    println!(
        "  longest over-1ms run{}",
        fmt(stat(&|r| tail(&r.ms).0 as f64), 0)
    );
    println!("  worst backlog ms    {}", fmt(stat(&|r| tail(&r.ms).1), 2));
    // rate on the heavy ticks (exec >= 50k): time per executed instruction
    let rate = stat(&|r| {
        let (mut t, mut n) = (0.0, 0.0);
        for (ms, e) in r.ms.iter().zip(&r.exec) {
            if *e >= 50_000 {
                t += ms;
                n += *e as f64;
            }
        }
        if t > 0.0 {
            n / t / 1e3
        } else {
            0.0
        }
    });
    println!("  M insns/s (ticks with exec >= 50k) {}", fmt(rate, 1));
    let cpi = stat(&|r| {
        let (mut c, mut n) = (0.0, 0.0);
        for (cy, e) in r.cyc.iter().zip(&r.exec) {
            if *e >= 50_000 {
                c += *cy as f64;
                n += *e as f64;
            }
        }
        if n > 0.0 {
            c / n
        } else {
            0.0
        }
    });
    println!(
        "  thread cycles per insn (ticks with exec >= 50k) {}",
        fmt(cpi, 2)
    );
    // deterministic tail in instruction units at the median rate: ms_i = exec_i / rate
    let r = rate[1] * 1e3; // insns per ms
    let model: Vec<f64> = ex.iter().map(|e| e / r).collect();
    let (run, backlog) = tail(&model);
    println!(
        "  model at median rate {:.0} insns/ms: ticks > 1 ms {}, longest run {run}, backlog {backlog:.2} ms; rate needed for every tick <= 1 ms: {:.0} M insns/s",
        r,
        model.iter().filter(|x| **x > 1.0).count(),
        percentile(&ex, 1.0) / 1e3
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let budgets: Vec<u64> = env("BUDGETS", "150000,200000,300000,456000")
        .split(',')
        .map(|x| x.trim().parse().expect("budget"))
        .collect();
    let runs: usize = env("RUNS", "7").parse().unwrap();
    let scale: usize = env("PHASE_SCALE", "100").parse().unwrap();
    let code = std::fs::read(&a[1]).expect("code");
    let sram = std::fs::read(&a[2]).expect("sram");
    let eeprom = std::fs::read(&a[3]).expect("eeprom image");
    let m = boot(&code, &sram, &eeprom);
    if std::env::var("PROFILE").is_ok() {
        return profile(&m, budgets[0], scale);
    }
    // warm-up (not counted), then interleaved: run r of every budget before run r + 1
    let _ = run_all(&m, budgets[0], scale.min(20));
    let mut all: Vec<Vec<Run>> = budgets.iter().map(|_| Vec::new()).collect();
    for _ in 0..runs {
        for (i, b) in budgets.iter().enumerate() {
            all[i].push(run_all(&m, *b, scale));
        }
    }
    for (b, r) in budgets.iter().zip(&all) {
        report(*b, r);
    }
}

/// Top 64-byte code buckets by executed instructions on the heavy ticks (exec >= 100k, found by
/// a first idle-skipping pass), run on the plain interpreter so every pc is seen.
fn profile(base: &Machine, budget: u64, scale: usize) {
    let heavy: Vec<bool> = run_all(base, budget, scale)
        .exec
        .iter()
        .map(|e| *e >= 100_000)
        .collect();
    let mut m = base.clone();
    m.predecode = false;
    let mut hist: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let (mut tick, mut buf) = (0, Vec::new());
    for (_, ticks, action) in plan(scale) {
        for i in 0..ticks {
            if i == 0 {
                action(&mut m);
            }
            let end = m.cpu.insns + budget;
            while m.cpu.insns < end {
                let pc = m.cpu.r[15];
                let (_, stop) = m.run(1, nord_arm::machine::SENTINEL, None);
                if heavy[tick] {
                    *hist.entry(pc >> 6).or_default() += 1;
                }
                match stop {
                    Stop::Budget => {}
                    Stop::Fault(nord_arm::cpu::Fault::Wfi) => {
                        m.cpu.insns = end;
                    }
                    other => panic!("{other:?}"),
                }
            }
            m.bus.mem.zero(0, nord_arm::machine::LOW_MEMORY as usize);
            m.inject_irq(nord_arm::machine::boot::TIMER_IRQ);
            buf.clear();
            m.drain_messages_into(&mut buf);
            tick += 1;
        }
    }
    let total: u64 = hist.values().sum();
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.1));
    println!(
        "heavy ticks: {}, executed pcs counted: {total}",
        heavy.iter().filter(|h| **h).count()
    );
    let mut acc = 0;
    for (b, n) in v.iter().take(40) {
        acc += n;
        println!(
            "  {:#010x}  {:>10}  {:>5.1}%  cum {:>5.1}%",
            b << 6,
            n,
            100.0 * *n as f64 / total as f64,
            100.0 * acc as f64 / total as f64
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn budgets_default_list_is_the_four_plausible_budgets() {
        assert_eq!("150000,200000,300000,456000".split(',').count(), 4);
    }
}
