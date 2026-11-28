//! ARM0 LLE benchmark (`ns4-lle2`): boot the user's OS v1.66 ARM image with panel, I2C EEPROM
//! (program memory) and UART2 MIDI in the native Rust core, then time 1 ms ticks through phases
//! like `tools/arm_org_stream.py` (load, idle, one key, eight keys). Each timed tick contains the
//! MIDI send (`action`, first tick of a phase), the tick run (idle skipping on), and the message
//! drain with the DSP return sync. The same phases run again with full execution on a clone;
//! the state hash AND the message content are compared after every phase.
//!
//!     cargo run --release -p nord-tools --bin arm-lle-bench -- \
//!         <bank0_CODE.bin> <bank0_SRAM.bin> <eeprom_image_256k.bin>
//!
//! Environment: `TICK_INSNS` (instructions per tick, default 200000), `BOOT_ONLY=1` (boot, print
//! the rate, stop), `PHASE_SCALE` (percent of the tick counts, default 100), `NO_FULL=1` skips the
//! full-execution comparison, `NO_RESERVE=1` skips the arena reservation. `PREDECODE=0` runs the
//! whole benchmark on the plain `Cpu::step` path. The "full" comparison above clones the machine,
//! so it checks idle skipping against full execution on ONE interpreter; to compare the old
//! interpreter with the predecode cache, run once with `PREDECODE=0` and once without and diff the
//! `digest` and `boot rate ... state hash` lines (same `TICK_INSNS`).

use nord_arm::machine::idle::IdleStats;
use nord_arm::machine::metrics::{
    digest_messages, percentile, simulate_queue, tail, weighted_percentiles,
};
use nord_arm::machine::paced::{consume, produce, Flight};
use nord_arm::machine::system::Message;
use nord_arm::machine::{Machine, Stop};
use nord_sharc::cpu_clock::ThreadClock;
use nord_tools::arm_bench::{plan, RESERVE_PAGES};
use std::time::Instant;

/// ASSUMED: ARM instructions per 1 ms tick (`tools/arm_steady.py` INSNS_PER_TICK); the
/// environment variable `TICK_INSNS` overrides it (budget sensitivity).
const TICK_DEFAULT: u64 = 200_000;

fn tick_insns() -> u64 {
    std::env::var("TICK_INSNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(TICK_DEFAULT)
}

/// Per-tick measurements of one phase.
struct Row {
    name: &'static str,
    /// Wall time per tick: the deadline statistics (>1 ms, backlog) use this.
    ms: Vec<f64>,
    /// Thread CPU time per tick (wall time where `ThreadClock::is_cpu_time()` is false).
    cpu_ms: Vec<f64>,
    executed: Vec<u64>,
    device_reads: Vec<u64>,
    arena_growth: Vec<usize>,
    msgs: Vec<Message>,
    msgs_per_tick: Vec<usize>,
}

fn run_phase(
    m: &mut Machine,
    name: &'static str,
    ticks: usize,
    idle: Option<&mut IdleStats>,
    action: &dyn Fn(&mut Machine),
) -> Row {
    let mut row = Row {
        name,
        ms: Vec::with_capacity(ticks),
        cpu_ms: Vec::with_capacity(ticks),
        executed: Vec::new(),
        device_reads: Vec::new(),
        arena_growth: Vec::new(),
        msgs: Vec::new(),
        msgs_per_tick: Vec::new(),
    };
    let mut buf: Vec<Message> = Vec::with_capacity(4096);
    let mut none = IdleStats::default();
    let stats = idle.unwrap_or(&mut none);
    for i in 0..ticks {
        let (dev0, arena0, exec0) = (m.bus.device_reads, m.bus.mem.arena_bytes(), stats.executed);
        buf.clear();
        let t = ThreadClock::start();
        if i == 0 {
            action(m);
        }
        let s = m.run_tick_idle(tick_insns(), stats);
        m.drain_messages_into(&mut buf);
        row.cpu_ms.push(t.elapsed_ms());
        row.ms.push(t.wall_ns() as f64 / 1e6);
        assert!(
            matches!(s, Stop::Budget),
            "{name} tick {i}: {s:?} pc={:#x}",
            m.cpu.pc()
        );
        row.executed.push(stats.executed - exec0);
        row.device_reads.push(m.bus.device_reads - dev0);
        row.arena_growth.push(m.bus.mem.arena_bytes() - arena0);
        row.msgs_per_tick.push(buf.len());
        row.msgs.extend_from_slice(&buf);
    }
    row
}

/// Full execution (no skipping) of one phase on the comparison clone.
fn run_phase_full(
    m: &mut Machine,
    name: &'static str,
    ticks: usize,
    action: &dyn Fn(&mut Machine),
) -> Row {
    let mut row = Row {
        name,
        ms: Vec::new(),
        cpu_ms: Vec::new(),
        executed: Vec::new(),
        device_reads: Vec::new(),
        arena_growth: Vec::new(),
        msgs: Vec::new(),
        msgs_per_tick: Vec::new(),
    };
    let mut buf = Vec::new();
    for i in 0..ticks {
        buf.clear();
        let t = ThreadClock::start();
        if i == 0 {
            action(m);
        }
        let s = m.run_tick(tick_insns());
        m.drain_messages_into(&mut buf);
        row.cpu_ms.push(t.elapsed_ms());
        row.ms.push(t.wall_ns() as f64 / 1e6);
        assert!(matches!(s, Stop::Budget), "{name} tick {i}: {s:?}");
        row.msgs_per_tick.push(buf.len());
        row.msgs.extend_from_slice(&buf);
    }
    row
}

/// `PACED=1`: the six phases on a producer thread at one tick per wall-clock ms, messages through
/// a bounded queue (`PACED_CAP` ticks, default 64) to a consumer that drains once per ms.
fn paced(m: Machine, scale: usize) {
    let cap = std::env::var("PACED_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    let plan = plan(scale);
    let total: usize = plan.iter().map(|p| p.1).sum();
    let (tx, rx) = std::sync::mpsc::sync_channel(cap);
    let t0 = Instant::now() + std::time::Duration::from_millis(50);
    let insns = tick_insns();
    let flight = std::sync::Arc::new(Flight::default());
    let producer_flight = flight.clone();
    let h = std::thread::spawn(move || {
        let (mut m, mut stats, mut first, mut blocked) = (m, IdleStats::default(), 0, 0.0);
        for (_, ticks, action) in plan {
            blocked += produce(
                &mut m,
                &mut stats,
                t0,
                first,
                ticks,
                insns,
                action,
                &tx,
                &producer_flight,
            );
            first += ticks;
        }
        blocked
    });
    let rep = consume(&rx, t0, total, &flight);
    let blocked = h.join().unwrap();
    println!("paced run, queue capacity {cap} ticks: producer blocked {blocked:.2} ms, max queue {} msgs / {} ticks per drain loop, peak {} ticks in flight, {} out of order", rep.max_queue_msgs, rep.max_queue_ticks, rep.max_inflight, rep.out_of_order);
    println!(
        "phase      ticks  msg lateness ms: p50    p99    p99.9  max     late ticks (>0 / >1 ms)"
    );
    let mut from = 0;
    for (name, ticks, _) in plan {
        let sl = &rep.per_tick[from..from + ticks];
        let p = weighted_percentiles(sl);
        let late = sl.iter().filter(|x| x.0 > 1e-9).count();
        let late1 = sl.iter().filter(|x| x.0 > 1.0).count();
        println!("{name:<9} {ticks:>6}                  {:>5.2}  {:>5.2}  {:>5.2}  {:>5.2}   {late} / {late1}", p[0], p[1], p[2], p[3]);
        from += ticks;
    }
    let p = weighted_percentiles(&rep.per_tick);
    println!(
        "all       {total:>6}                  {:>5.2}  {:>5.2}  {:>5.2}  {:>5.2}",
        p[0], p[1], p[2], p[3]
    );
}

fn phases(
    m: &mut Machine,
    scale: usize,
    mut stats: Option<&mut IdleStats>,
) -> (Vec<Row>, Vec<u64>) {
    let (mut rows, mut hashes) = (Vec::new(), Vec::new());
    for (name, ticks, action) in plan(scale) {
        let row = match stats.as_deref_mut() {
            Some(s) => run_phase(m, name, ticks, Some(s), &action),
            None => run_phase_full(m, name, ticks, &action),
        };
        rows.push(row);
        hashes.push(m.state_hash());
    }
    (rows, hashes)
}

fn cpu_model() -> String {
    let id = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown CPU".into());
    let n = std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_else(|_| "?".into());
    format!("{id} ({n} logical), {}", std::env::consts::OS)
}

fn report(rows: &[Row]) {
    println!("\nphase      ticks  mean ms  p50     p99     max     >1ms          run  backlog ms  exec insns/tick  msgs");
    let all: Vec<f64> = rows.iter().flat_map(|r| r.ms.iter().copied()).collect();
    for r in rows.iter() {
        let mean = r.ms.iter().sum::<f64>() / r.ms.len() as f64;
        let over = r.ms.iter().filter(|x| **x > 1.0).count();
        let (run, backlog) = tail(&r.ms);
        println!(
            "{:<9} {:>6}  {:>7.3}  {:>6.3}  {:>6.3}  {:>6.3}  {:>6.3}  {:>3} ({:>4.1}%)  {:>4}  {:>10.2}  {:>14.0}  {:>6}  {:>10.3}  {:>10.3}",
            r.name,
            r.ms.len(),
            mean,
            percentile(&r.ms, 0.5),
            percentile(&r.ms, 0.99),
            percentile(&r.ms, 0.999),
            percentile(&r.ms, 1.0),
            over,
            100.0 * over as f64 / r.ms.len() as f64,
            run,
            backlog,
            r.executed.iter().sum::<u64>() as f64 / r.ms.len() as f64,
            r.msgs.len(),
            r.cpu_ms.iter().sum::<f64>() / r.cpu_ms.len().max(1) as f64,
            r.cpu_ms.iter().copied().fold(0.0, f64::max)
        );
    }
    let (run, backlog) = tail(&all);
    println!(
        "all phases back to back: longest over-1ms run {run} ticks, worst backlog {backlog:.2} ms"
    );
    let per_tick: Vec<usize> = rows
        .iter()
        .flat_map(|r| r.msgs_per_tick.iter().copied())
        .collect();
    for (name, ms, n) in rows
        .iter()
        .map(|r| (r.name, r.ms.clone(), r.msgs_per_tick.clone()))
        .chain([("all", all.clone(), per_tick)])
    {
        let q = simulate_queue(&ms, &n, 1.0);
        println!(
            "queue {name:<9} msg latency ms p50 {:.2} p99 {:.2} p99.9 {:.2} max {:.2}, late ticks {}, max queue {} msgs, max {} ticks per 1 ms drain",
            q.latency[0], q.latency[1], q.latency[2], q.latency[3], q.late_ticks, q.max_queue, q.max_ticks_per_block
        );
    }
    for r in rows.iter() {
        println!(
            "digest {:<9} messages {:016x}",
            r.name,
            digest_messages(&r.msgs)
        );
    }
    // what the over-1 ms ticks are: real instructions, device-window polling, or arena growth
    let mut heavy = Vec::new();
    for r in rows {
        for (i, t) in r.ms.iter().enumerate() {
            if *t > 1.0 {
                heavy.push((
                    r.name,
                    i,
                    *t,
                    r.executed[i],
                    r.device_reads[i],
                    r.arena_growth[i],
                ));
            }
        }
    }
    let by_dev = heavy.iter().filter(|h| h.4 > 1_000).count();
    let by_arena = heavy.iter().filter(|h| h.5 > 0).count();
    println!(
        "over-1ms ticks: {} (device-window reads > 1000: {by_dev}, arena grew: {by_arena})",
        heavy.len()
    );
    heavy.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    for h in heavy.iter().take(8) {
        println!(
            "  {:<9} tick {:>4}  {:>6.2} ms  exec {:>7}  device reads {:>7}  arena +{} KB",
            h.0,
            h.1,
            h.2,
            h.3,
            h.4,
            h.5 / 1024
        );
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if std::env::var("MICRO").is_ok() {
        return nord_arm::machine::micro::print_dispatch_rates(50_000_000);
    }
    let code = std::fs::read(&a[1]).expect("code");
    let sram = std::fs::read(&a[2]).expect("sram");
    let eeprom = std::fs::read(&a[3]).expect("eeprom image");
    let scale: usize = std::env::var("PHASE_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    println!("cpu: {}", cpu_model());
    if std::env::var("STEADY_BOOT").is_ok() {
        // same boot as `tools/arm_steady.py` (display stubs on, no board): null-call comparison
        let mut m = Machine::new();
        m.prepare(&code, &sram).expect("init array");
        let (n, stop) = m.boot_to_main_loop(1_500_000_000);
        println!(
            "steady boot: {stop:?} pc={:#x} ~{n} instructions, nulls={}",
            m.cpu.pc(),
            m.null_call_count
        );
        let mut by_lr = std::collections::BTreeMap::new();
        for &(pc, lr) in &m.null_calls {
            *by_lr.entry((pc, lr)).or_insert(0u32) += 1;
        }
        for ((pc, lr), k) in by_lr {
            println!("null {pc:#x} {lr:#x} {k}");
        }
        // the Python run checks its stop address once per 200k chunk and counts whole chunks:
        // run on to the next chunk boundary to compare like with like
        let more = m.run_chunked(None, 600_000, 200_000).0;
        println!(
            "steady boot + {more} insns past the main loop: nulls={}",
            m.null_call_count
        );
        return;
    }
    let mut m = Machine::new();
    m.predecode = std::env::var("PREDECODE").map_or(true, |v| v != "0");
    println!(
        "predecode: {}",
        if m.predecode {
            "on"
        } else {
            "off (plain Cpu::step path)"
        }
    );
    let t = Instant::now();
    m.prepare_system(&code, &sram).expect("init array");
    m.bus.i2c.as_mut().unwrap().load_image(&eeprom);
    let mut boot_stats = IdleStats::default();
    let (n, irqs, stop) = m.boot_system(1_500_000_000, &mut boot_stats);
    println!(
        "boot: {stop:?} pc={:#x} {n} virtual insns ({} executed, {} skipped), {irqs} wait-loop irqs, {:.2}s wall",
        m.cpu.pc(),
        boot_stats.executed,
        boot_stats.skipped,
        t.elapsed().as_secs_f64()
    );
    println!(
        "boot rate: {:.1} M executed insns/s, {boot_stats:?}, state hash {:016x}",
        boot_stats.executed as f64 / t.elapsed().as_secs_f64() / 1e6,
        m.state_hash()
    );
    if std::env::var("BOOT_ONLY").is_ok() {
        return;
    }
    m.install_dsp_ports();
    m.install_dsp_return();
    m.enable_real_timer();
    if std::env::var("NO_RESERVE").is_err() {
        m.bus.mem.reserve_pages(RESERVE_PAGES);
    }
    println!(
        "after boot: arena {} KB, heap_top {:#x}, auto-mapped pages {}, null calls {}, wfi {}, mode changes {}",
        m.bus.mem.arena_bytes() / 1024,
        m.heap_top,
        m.bus.mem.auto_mapped_count,
        m.null_call_count,
        m.wfi,
        m.cpu.mode_changes
    );
    if std::env::var("PACED").is_ok() {
        return paced(m, scale);
    }
    let mut full = m.clone();
    let mut stats = IdleStats::default();
    let (rows, h_idle) = phases(&mut m, scale, Some(&mut stats));
    report(&rows);
    for (r, h) in rows.iter().zip(&h_idle) {
        println!("digest {:<9} state {h:016x}", r.name);
    }
    println!("idle stats: {stats:?}");
    println!(
        "after run: arena {} KB, heap_top {:#x}, auto-mapped pages {}, null calls {}, wfi {} (stats {}), irq injected with I bit set {}, dropped messages {}, pending words {}, DSP return reads {}",
        m.bus.mem.arena_bytes() / 1024,
        m.heap_top,
        m.bus.mem.auto_mapped_count,
        m.null_call_count,
        m.wfi,
        stats.wfi,
        m.irq_while_masked,
        m.bus.dropped_messages,
        m.bus.grouper.pending_words(),
        m.dsp_return.as_ref().map_or(0, |r| r.reads)
    );
    if std::env::var("NO_FULL").is_err() {
        let (frows, h_full) = phases(&mut full, scale, None);
        for ((r, f), (a, b)) in rows.iter().zip(&frows).zip(h_idle.iter().zip(&h_full)) {
            let mi = r.ms.iter().sum::<f64>() / r.ms.len() as f64;
            let mf = f.ms.iter().sum::<f64>() / f.ms.len() as f64;
            println!(
                "{:<9} full {:.3} ms/tick, skipping {:.3} ms/tick, state hash {}, messages {} vs {} content {}",
                r.name,
                mf,
                mi,
                if a == b { "EQUAL" } else { "DIFFERENT" },
                f.msgs.len(),
                r.msgs.len(),
                if f.msgs == r.msgs { "EQUAL" } else { "DIFFERENT" }
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_scales_the_phase_lengths_and_keeps_every_phase() {
        let full = plan(100);
        assert_eq!(full.map(|p| p.1), [800, 300, 200, 200, 200, 200]);
        let tiny = plan(1);
        assert_eq!(tiny.map(|p| p.1), [8, 3, 2, 2, 2, 2]);
        assert!(
            plan(0).iter().all(|p| p.1 == 1),
            "a phase never has zero ticks"
        );
    }

    #[test]
    fn tick_budget_defaults_to_the_assumed_200k() {
        assert_eq!(TICK_DEFAULT, 200_000);
    }
}
