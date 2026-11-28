//! ARM0 on a decoupled producer thread (`ns4-lle2` tail follow-up, D254): the six benchmark
//! phases with the real OS image, per instruction budget, in the two modes of
//! `nord_arm::machine::producer`.
//!
//!     cargo run --release -p nord-tools --bin arm-rt-bench -- \
//!         <bank0_CODE.bin> <bank0_SRAM.bin> <eeprom_image_256k.bin>
//!
//! Per budget: a single-thread reference run (per-tick cost series and message digest), the queue
//! size derived from its backlog ([`queue_ticks_for`]), two deterministic threaded runs (no
//! clock; message streams and state hashes must equal each other and the reference) and one
//! real-time run (consumer on the wall clock: underruns, lateness, producer back-pressure).
//! The real-time consumer takes ONE tick per 1 ms deadline and starts cold (`PREFILL_MS`, default 0,
//! delays its first deadline after the producer start). Environment: `BUDGETS` (default
//! `150000,200000,300000,456000`), `PHASE_SCALE`, `RT_RUNS`, `RATE` (M insns/s for the queue), `QUEUE` (forces
//! the queue size in ticks).

use nord_arm::machine::consumer::{collect_blocking, consume_realtime, queue_ticks_for};
use nord_arm::machine::metrics::digest_messages;
use nord_arm::machine::producer::{Config, Producer};
use nord_tools::arm_bench::{boot, run_all, script, total_ticks};
use std::time::{Duration, Instant};

fn digest(ticks: &[nord_arm::machine::producer::TickMsgs]) -> u64 {
    let all: Vec<_> = ticks.iter().flat_map(|t| t.msgs.iter().cloned()).collect();
    digest_messages(&all)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let budgets: Vec<u64> = env("BUDGETS", "150000,200000,300000,456000")
        .split(',')
        .map(|x| x.trim().parse().expect("budget"))
        .collect();
    let scale: usize = env("PHASE_SCALE", "100").parse().unwrap();
    let forced: Option<usize> = std::env::var("QUEUE").ok().and_then(|v| v.parse().ok());
    let m = boot(
        &std::fs::read(&a[1]).expect("code"),
        &std::fs::read(&a[2]).expect("sram"),
        &std::fs::read(&a[3]).expect("eeprom image"),
    );
    let ticks = total_ticks(scale);
    for budget in budgets {
        // queue from the DETERMINISTIC executed-instruction series and a conservative rate
        // (RATE, M insns/s, default 110 = the lowest heavy-tick rate seen on the loaded host)
        let reference = run_all(&m, budget, scale);
        let rate = env("RATE", "110").parse::<f64>().unwrap() * 1e3;
        let model: Vec<f64> = reference.exec.iter().map(|e| *e as f64 / rate).collect();
        let queue = forced.unwrap_or_else(|| queue_ticks_for(&model));
        let cfg = Config {
            budget,
            queue_ticks: queue,
            ticks,
            script: script(scale),
            tick_delay: Default::default(),
        };
        println!("\n== budget {budget} insns/ms, {ticks} ticks, queue {queue} ticks ==");
        println!(
            "reference (1 thread): digest messages {:016x} state {:016x}",
            reference.digest, reference.state
        );
        let mut det = Vec::new();
        for i in 0..2 {
            let t = Instant::now();
            let p = Producer::spawn(m.clone(), cfg.clone());
            let got = collect_blocking(&p);
            let r = p.finish().expect("producer");
            println!(
                "deterministic run {i}: {} ticks, digest messages {:016x} state {:016x}, queue full {} times, producer blocked {:.1} ms, {:.0} ms wall",
                got.ticks.len(), digest(&got.ticks), r.machine.state_hash(), r.full_events,
                r.blocked.as_secs_f64() * 1e3, t.elapsed().as_secs_f64() * 1e3
            );
            det.push((digest(&got.ticks), r.machine.state_hash()));
        }
        let ok = det[0] == det[1] && det[0] == (reference.digest, reference.state);
        println!(
            "deterministic streams equal each other and the single-thread reference: {}",
            if ok { "YES" } else { "NO" }
        );
        // derived queue (RT_RUNS times), then a 1-tick queue for contrast
        let runs = env("RT_RUNS", "3").parse::<usize>().unwrap();
        // consumer start offset after the producer start: 0 = cold (tick k due k + 1 ms after the spawn)
        let prefill_ms = env("PREFILL_MS", "0").parse::<u64>().unwrap();
        for (label, q) in std::iter::repeat_n(("derived", queue), runs).chain([("1 tick", 1)]) {
            let p = Producer::spawn(
                m.clone(),
                Config {
                    queue_ticks: q,
                    ..cfg.clone()
                },
            );
            let t0 = Instant::now() + Duration::from_millis(prefill_ms);
            let rep = consume_realtime(&p, t0, ticks);
            let r = p.finish().expect("producer");
            println!(
                "real-time run, queue {label} ({q}): taken {}, underruns {} ({:.2}%), max lateness {:.2} ms, ended {:?}, queue full {} times, producer blocked {:.1} ms",
                rep.taken, rep.underruns, 100.0 * rep.underruns as f64 / rep.taken.max(1) as f64,
                rep.max_late_ms, rep.end, r.full_events, r.blocked.as_secs_f64() * 1e3
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_of_no_ticks_is_the_digest_of_no_messages() {
        assert_eq!(digest(&[]), digest_messages(&[]));
    }
}
