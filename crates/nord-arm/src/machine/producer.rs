//! ARM0 on a decoupled producer thread (`ns4-lle2` tail follow-up, D254).
//!
//! The ARM core runs ahead of virtual time on its own thread and hands each tick's messages to the
//! consumer (the DSP side) through a bounded queue of `queue_ticks` ticks. A tick that costs more
//! than 1 ms of host time does not stall the consumer as long as the producer is at least that far
//! ahead, so the real-time criterion becomes "the backlog never exceeds the queue", not "every
//! tick under 1 ms" ([`super::consumer::queue_ticks_for`] sizes the queue from a per-tick cost series).
//!
//! * **Deterministic mode** ([`super::consumer::collect_blocking`]): blocking `recv`, no wall clock read anywhere.
//!   The stream is a function of the machine, the budget, the tick count and the input script
//!   only: it equals a plain single-thread `run_tick_idle` + drain loop whatever the scheduling or
//!   the queue size (tests below).
//! * **Real-time mode** ([`super::consumer::consume_realtime`]): the consumer takes exactly ONE tick per
//!   millisecond deadline and counts a deadline at which that tick was not ready (underrun) with
//!   its lateness. The producer is the same code: it runs until the queue is full (back-pressure:
//!   `send` blocks, counted in [`Report::full_events`] / [`Report::blocked`]).
//! * **Input**: a script `(tick, bytes)` is applied at the start of that tick (deterministic).
//!   Live input ([`Producer::input`]) is applied at the start of the producer's next tick and
//!   recorded in [`Report::applied`] (capped at [`APPLIED_CAP`]) so a session can be replayed.
//! * **Ending**: the producer sends [`Item::Done`] after the last tick, or the faulting tick's
//!   messages so far and [`Item::Fault`]; a panic closes the channel with neither, which the
//!   consumers report as [`End::Disconnected`] and [`Producer::finish`] as an `Err`.
//!
//! Not allocation-free: the producer allocates per tick (its own thread), the `mpsc` channels
//! allocate on `send`, and the consumer helpers push into a `Vec`. That is acceptable for the
//! offline harness; real-time DSP integration needs a preallocated SPSC ring of message buffers.

use super::grouper::Message;
use super::idle::IdleStats;
use super::{Machine, Stop};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Entries kept in [`Report::applied`]; later ones only count in `applied_dropped`.
pub const APPLIED_CAP: usize = 4096;

/// Messages one tick produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickMsgs {
    pub tick: u64,
    pub msgs: Vec<Message>,
}

/// What travels through the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Tick(TickMsgs),
    /// The machine stopped with `stop` (not a budget end) during `tick`; the messages it made so
    /// far came just before as a `Tick`.
    Fault {
        tick: u64,
        stop: Stop,
    },
    /// Every requested tick was produced.
    Done,
}

/// How a consumer saw the producer end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    Complete,
    Fault {
        tick: u64,
        stop: Stop,
    },
    /// The channel closed without `Done` or `Fault` (the producer thread panicked).
    Disconnected,
}

/// What a producer run needs.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Instructions of virtual time per tick (1 ms).
    pub budget: u64,
    /// Ticks the queue holds (at least 1).
    pub queue_ticks: usize,
    /// Ticks to produce.
    pub ticks: u64,
    /// `(tick, MIDI bytes)` applied at the start of `tick`; need not be sorted.
    pub script: Vec<(u64, Vec<u8>)>,
    /// Host time the producer waits after every tick (emulates a slow producer in tests; zero in
    /// normal use).
    pub tick_delay: Duration,
}

/// What the producer thread returns when it ends.
pub struct Report {
    pub machine: Machine,
    pub stop: Stop,
    pub produced: u64,
    /// Ticks that found the queue full (the producer had to wait for the consumer).
    pub full_events: u64,
    /// Wall clock spent blocked on a full queue.
    pub blocked: Duration,
    /// Inputs applied with the tick they landed on (script entries and live input), at most
    /// [`APPLIED_CAP`].
    pub applied: Vec<(u64, Vec<u8>)>,
    /// Inputs applied beyond [`APPLIED_CAP`].
    pub applied_dropped: u64,
    pub idle: IdleStats,
}

/// Handle of a running producer.
pub struct Producer {
    pub rx: Receiver<Item>,
    input: Sender<Vec<u8>>,
    recycle: Sender<Vec<Message>>,
    join: JoinHandle<Report>,
}

impl Producer {
    /// Start `machine` on its own thread.
    pub fn spawn(mut machine: Machine, mut cfg: Config) -> Producer {
        let (tx, rx) = sync_channel::<Item>(cfg.queue_ticks.max(1));
        let (input, input_rx) = channel::<Vec<u8>>();
        let (recycle, recycle_rx) = channel::<Vec<Message>>();
        cfg.script.sort_by_key(|s| s.0);
        let join = std::thread::spawn(move || {
            let mut idle = IdleStats::default();
            let (mut applied, mut dropped) = (Vec::new(), 0u64);
            let mut note = |tick: u64, bytes: &[u8]| {
                if applied.len() < APPLIED_CAP {
                    applied.push((tick, bytes.to_vec()));
                } else {
                    dropped += 1;
                }
            };
            let (mut script, mut produced) = (cfg.script.iter(), 0u64);
            let (mut full_events, mut blocked, mut stop) = (0u64, Duration::ZERO, Stop::Budget);
            let mut next = script.next();
            // blocking send that counts a full queue; false when the consumer is gone
            let mut send = |item: Item| match tx.try_send(item) {
                Ok(()) => true,
                Err(TrySendError::Full(item)) => {
                    full_events += 1;
                    let t0 = Instant::now();
                    let sent = tx.send(item).is_ok();
                    blocked += t0.elapsed();
                    sent
                }
                Err(TrySendError::Disconnected(_)) => false,
            };
            let mut completed = true;
            for tick in 0..cfg.ticks {
                while let Ok(bytes) = input_rx.try_recv() {
                    machine.send_midi(&bytes);
                    note(tick, &bytes);
                }
                while let Some(s) = next.filter(|s| s.0 <= tick) {
                    machine.send_midi(&s.1);
                    note(tick, &s.1);
                    next = script.next();
                }
                let s = machine.run_tick_idle(cfg.budget, &mut idle);
                let mut msgs = recycle_rx.try_recv().unwrap_or_default();
                msgs.clear();
                machine.drain_messages_into(&mut msgs);
                if !matches!(s, Stop::Budget) {
                    stop = s;
                    completed = false;
                    if send(Item::Tick(TickMsgs { tick, msgs })) {
                        send(Item::Fault { tick, stop: s });
                    }
                    break;
                }
                if !send(Item::Tick(TickMsgs { tick, msgs })) {
                    completed = false;
                    break;
                }
                produced += 1;
                if !cfg.tick_delay.is_zero() {
                    std::thread::sleep(cfg.tick_delay);
                }
            }
            if completed {
                send(Item::Done);
            }
            Report {
                machine,
                stop,
                produced,
                full_events,
                blocked,
                applied,
                applied_dropped: dropped,
                idle,
            }
        });
        Producer {
            rx,
            input,
            recycle,
            join,
        }
    }

    /// Live MIDI input: applied at the start of the producer's next tick.
    pub fn input(&self, bytes: Vec<u8>) {
        let _ = self.input.send(bytes);
    }

    /// Give a drained buffer back to the producer for reuse.
    pub fn recycle(&self, buf: Vec<Message>) {
        let _ = self.recycle.send(buf);
    }

    /// Wait for the producer thread (drops `rx` first so a producer blocked on a full queue
    /// ends). `Err` carries the panic message of a producer that panicked.
    pub fn finish(self) -> Result<Report, String> {
        drop(self.rx);
        self.join.join().map_err(|e| {
            e.downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "producer thread panicked".into())
        })
    }
}

/// Ticks received and how the producer ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collected {
    pub ticks: Vec<TickMsgs>,
    pub end: End,
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::machine::consumer::collect_blocking;
    use crate::machine::sysbus::PORT1_BASE;

    /// Synthetic ARM0: counts in r2 and stores the count as a DSP message every iteration.
    pub(in crate::machine) fn machine() -> Machine {
        let code = [
            0xE282_2001, // add r2,r2,#1
            0xE588_2030, // str r2,[r8,#0x30]  (first word of comm 3, path 1)
            0xE5C8_2031, // strb r2,[r8,#0x31] (second word completes the message)
            0xEAFF_FFFB, // b .-12 (back to the add)
        ];
        let mut m = Machine::new();
        for (i, w) in code.iter().enumerate() {
            m.bus.mem.write32(0xC000_0000 + 4 * i as u32, *w);
        }
        m.cpu.r[8] = PORT1_BASE;
        m.cpu.r[15] = 0xC000_0000;
        m
    }

    pub(in crate::machine) fn cfg(queue: usize, ticks: u64) -> Config {
        Config {
            budget: 400,
            queue_ticks: queue,
            ticks,
            script: vec![(5, vec![0x90, 60, 100]), (3, vec![0x80, 60, 0])],
            tick_delay: Duration::ZERO,
        }
    }

    fn run(queue: usize, ticks: u64, slow_consumer: bool) -> (Collected, Report) {
        let p = Producer::spawn(machine(), cfg(queue, ticks));
        let mut got = Collected {
            ticks: Vec::new(),
            end: End::Complete,
        };
        loop {
            match p.rx.recv() {
                Ok(Item::Tick(t)) => {
                    if slow_consumer {
                        std::thread::sleep(Duration::from_micros(300));
                    }
                    got.ticks.push(t);
                }
                Ok(Item::Done) => break,
                Ok(Item::Fault { tick, stop }) => {
                    got.end = End::Fault { tick, stop };
                    break;
                }
                Err(_) => {
                    got.end = End::Disconnected;
                    break;
                }
            }
        }
        (got, p.finish().unwrap())
    }

    /// The same machine and script on one thread: no queue, no clock, no helper of this module.
    fn single_thread(ticks: u64) -> (Vec<TickMsgs>, u64) {
        let (mut m, mut idle) = (machine(), IdleStats::default());
        let script = cfg(1, ticks).script;
        let mut out = Vec::new();
        for tick in 0..ticks {
            for s in script.iter().filter(|s| s.0 == tick) {
                m.send_midi(&s.1);
            }
            m.run_tick_idle(400, &mut idle);
            let mut msgs = Vec::new();
            m.drain_messages_into(&mut msgs);
            out.push(TickMsgs { tick, msgs });
        }
        (out, m.state_hash())
    }

    #[test]
    fn the_threaded_stream_equals_a_plain_single_thread_loop() {
        let (reference, hash) = single_thread(40);
        assert!(reference.iter().map(|t| t.msgs.len()).sum::<usize>() > 100);
        for queue in [1, 3, 64] {
            let (got, r) = run(queue, 40, false);
            assert_eq!(got.end, End::Complete);
            assert_eq!(got.ticks, reference, "queue {queue}");
            assert_eq!(r.machine.state_hash(), hash, "queue {queue}");
        }
    }

    #[test]
    fn a_slow_consumer_and_a_tiny_queue_change_nothing_but_the_wait() {
        let (a, ra) = run(64, 40, false);
        let (b, rb) = run(2, 40, true);
        assert_eq!(a, b);
        assert_eq!(ra.machine.state_hash(), rb.machine.state_hash());
        assert!(a.ticks.iter().enumerate().all(|(i, t)| t.tick == i as u64));
    }

    #[test]
    fn script_input_lands_on_its_tick_in_order() {
        let (_, r) = run(4, 10, false);
        assert_eq!(
            r.applied,
            vec![(3, vec![0x80, 60, 0]), (5, vec![0x90, 60, 100])]
        );
        assert_eq!(r.applied_dropped, 0);
    }

    #[test]
    fn applied_is_capped_and_the_overflow_is_counted() {
        let mut c = cfg(2, 2);
        c.script = (0..APPLIED_CAP as u64 + 10)
            .map(|i| (i % 2, vec![0x90, 60, 1]))
            .collect();
        let p = Producer::spawn(machine(), c);
        let got = collect_blocking(&p);
        let r = p.finish().unwrap();
        assert_eq!(got.end, End::Complete);
        assert_eq!(r.applied.len(), APPLIED_CAP);
        assert_eq!(r.applied_dropped, 10);
    }

    #[test]
    fn a_full_queue_blocks_the_producer_and_is_counted() {
        let (got, r) = run(1, 30, true);
        assert_eq!(got.ticks.len(), 30);
        assert!(
            r.full_events > 0,
            "a 1-tick queue behind a slow consumer fills"
        );
        assert!(r.blocked > Duration::ZERO);
        let (_, big) = run(64, 30, false);
        assert_eq!(big.full_events, 0, "a queue as long as the run never fills");
    }

    #[test]
    fn live_input_is_applied_at_a_tick_start_and_recorded_for_replay() {
        let p = Producer::spawn(machine(), cfg(2, 20));
        p.input(vec![0x90, 64, 90]);
        let got = collect_blocking(&p);
        let r = p.finish().unwrap();
        assert_eq!(got.ticks.len(), 20);
        let live: Vec<_> = r.applied.iter().filter(|a| a.1 == [0x90, 64, 90]).collect();
        assert_eq!(live.len(), 1, "{:?}", r.applied);
        assert!(live[0].0 < 20);
    }
}
