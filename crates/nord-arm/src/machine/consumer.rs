//! Consumers of the ARM0 producer thread ([`super::producer`], D254): the deterministic one
//! (blocking `recv`, no clock), the real-time one (one tick per millisecond deadline, underruns
//! counted) and the queue sizing rule.

use super::metrics::tail;
use super::producer::{Collected, End, Item, Producer};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

/// Deterministic consumer: blocking `recv` until the producer ends, no clock.
pub fn collect_blocking(p: &Producer) -> Collected {
    let mut ticks = Vec::new();
    loop {
        let end = match p.rx.recv() {
            Ok(Item::Tick(t)) => {
                ticks.push(t);
                continue;
            }
            Ok(Item::Fault { tick, stop }) => End::Fault { tick, stop },
            Ok(Item::Done) => End::Complete,
            Err(_) => End::Disconnected,
        };
        return Collected { ticks, end };
    }
}

/// What the real-time consumer saw.
#[derive(Debug, Clone, PartialEq)]
pub struct RealtimeReport {
    /// Ticks taken, in order.
    pub taken: u64,
    /// Deadlines (tick `k` is due `k + 1` ms after `t0`) at which tick `k` was not ready.
    pub underruns: u64,
    /// Largest lateness of a tick behind its deadline, ms (0 when never late).
    pub max_late_ms: f64,
    pub end: End,
}

/// Real-time consumer: at deadline `t0 + (k + 1) ms` take exactly one tick (tick `k`); if it is
/// not ready that is an underrun, and the consumer waits for it and records the lateness. `t0` in
/// the past makes every deadline already due (tests). The producer's lead is therefore at most
/// the queue length: nothing is hoarded on the consumer side.
pub fn consume_realtime(p: &Producer, t0: Instant, ticks: u64) -> RealtimeReport {
    let mut rep = RealtimeReport {
        taken: 0,
        underruns: 0,
        max_late_ms: 0.0,
        end: End::Complete,
    };
    while rep.taken < ticks {
        let due = t0 + Duration::from_micros(1000 * (rep.taken + 1));
        while Instant::now() < due {
            std::hint::spin_loop();
        }
        let item = match p.rx.try_recv() {
            Ok(i) => i,
            Err(TryRecvError::Empty) => {
                rep.underruns += 1;
                let Ok(i) = p.rx.recv() else {
                    rep.end = End::Disconnected;
                    return rep;
                };
                rep.max_late_ms = rep.max_late_ms.max(due.elapsed().as_secs_f64() * 1e3);
                i
            }
            Err(TryRecvError::Disconnected) => {
                rep.end = End::Disconnected;
                return rep;
            }
        };
        match item {
            Item::Tick(t) => {
                debug_assert_eq!(t.tick, rep.taken);
                rep.taken += 1;
                p.recycle(t.msgs);
            }
            Item::Fault { tick, stop } => {
                rep.end = End::Fault { tick, stop };
                return rep;
            }
            Item::Done => return rep,
        }
    }
    rep
}

/// Queue size in ticks that absorbs the backlog of a series of per-tick costs in ms (consumed one
/// per ms, [`tail`]): `ceil(1.5 * worst backlog) + 2`.
///
/// ASSUMED: the 1.5 margin and the 2 ticks of slack (the tick the producer is blocked sending and
/// one of jitter). Evidence: a queue of `ceil(backlog) + 2` taken from ONE timing run underran in
/// 4 of 5 real-time runs at 150 k (host jitter); feed it the deterministic instruction series
/// divided by a conservative rate.
pub fn queue_ticks_for(cost_ms: &[f64]) -> usize {
    let (_, backlog) = tail(cost_ms);
    (backlog * 1.5).ceil() as usize + 2
}

#[cfg(test)]
mod tests {
    use super::super::producer::tests::{cfg, machine};
    use super::super::{Machine, Stop};
    use super::*;

    #[test]
    fn a_faulting_machine_is_reported_to_both_consumers() {
        let faulting = || {
            let mut m = Machine::new();
            m.bus.mem.write32(0xC000_0000, 0xE7F0_00F0); // undefined instruction
            m.cpu.r[15] = 0xC000_0000;
            m
        };
        let p = Producer::spawn(faulting(), cfg(4, 10));
        let got = collect_blocking(&p);
        assert!(
            matches!(
                got.end,
                End::Fault {
                    tick: 0,
                    stop: Stop::Fault(_)
                }
            ),
            "{got:?}"
        );
        assert_eq!(got.ticks.len(), 1, "the faulting tick's messages so far");
        let r = p.finish().unwrap();
        assert_eq!(r.produced, 0);
        assert!(matches!(r.stop, Stop::Fault(_)));
        let p = Producer::spawn(faulting(), cfg(4, 10));
        let rep = consume_realtime(&p, Instant::now(), 10);
        assert!(matches!(rep.end, End::Fault { tick: 0, .. }), "{rep:?}");
        assert_eq!(rep.taken, 1);
    }

    #[test]
    fn realtime_consumer_takes_every_tick_once_in_order_and_ends_complete() {
        // t0 far in the past: every deadline is already due, so the consumer is not paced
        let t0 = Instant::now().checked_sub(Duration::from_secs(5)).unwrap();
        let p = Producer::spawn(machine(), cfg(8, 50));
        let rep = consume_realtime(&p, t0, 50);
        let r = p.finish().unwrap();
        assert_eq!((rep.taken, rep.end.clone()), (50, End::Complete));
        assert_eq!(r.produced, 50);
    }

    #[test]
    fn a_producer_slower_than_the_deadlines_counts_underruns_and_lateness() {
        // 3 ms of wait per tick against a 1 ms deadline per tick: tick k is ready no earlier than
        // 3 (k + 1) ms, so (nearly) every deadline is missed and the first tick is >= 2 ms late
        let mut c = cfg(4, 10);
        c.tick_delay = Duration::from_millis(3);
        let p = Producer::spawn(machine(), c);
        let rep = consume_realtime(&p, Instant::now(), 10);
        p.finish().unwrap();
        assert_eq!(rep.taken, 10);
        assert!(rep.underruns >= 8, "{rep:?}");
        assert!(rep.max_late_ms >= 2.0, "{rep:?}");
    }

    #[test]
    fn queue_size_follows_the_backlog() {
        // a 5 ms tick behind 1 ms ticks: 4 ms of backlog, times 1.5, plus the slack
        let mut ms = vec![0.2; 10];
        assert_eq!(queue_ticks_for(&ms), 2);
        ms[3] = 5.0;
        assert_eq!(queue_ticks_for(&ms), 8);
    }
}
