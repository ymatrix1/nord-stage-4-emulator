//! System-level pass loop for `sharc-sys` (D119): ARM messages are delivered between passes and the
//! time of every component of one pass interval is measured separately.
//!
//! Components per pass (all wall clock of this host, interpreter only):
//!
//! | Component | What is timed |
//! |---|---|
//! | `msg` | delivering the ARM messages due before the pass ([`super::msg::deliver`]) |
//! | `stub` | the audio-input refresh stub ([`refresh_inputs`]): the pass-input words that the hardware DMA writes are re-stored with their current value, so the cost of "a DMA touches the buffers" is paid but no data changes (no DMA model, ASSUMED) |
//! | `sharc` | the interpreter steps from the trigger write to the poll loop |
//!
//! The pass period in audio time is `2 / frame_rate` seconds (ASSUMED, `docs/research/org-system-run.md`).
use super::msg::{deliver, Msg};
use super::pass::{PassConfig, SEC_CSID};
use crate::core::Core;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// Pass-input words the audio DMA fills (`DM(0xB0078)`, 20 words, and the LFO phases
/// `DM(0xB00C8)`, 12 words). ASSUMED: `docs/research/dsp-rx-protocol.md` section 6 lists the
/// two ranges as DMA-filled inputs; the oracle has no DMA, the stub only rewrites them.
pub const INPUT_RANGES: [(u32, u32); 2] = [(0xB_0078, 20), (0xB_00C8, 12)];

/// Audio frames per pass (ASSUMED, one LP24 path; see the research note).
pub const FRAMES_PER_PASS: f64 = 2.0;
/// Audio frame rate in Hz (ASSUMED: `docs/research/org-system-run.md`).
pub const FRAME_RATE: f64 = 43_402.343_75;

/// Audio time of one pass in seconds.
pub fn pass_period_s(frame_rate: f64) -> f64 {
    FRAMES_PER_PASS / frame_rate
}

/// Timing of one pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassTiming {
    pub steps: u64,
    pub msgs: u32,
    pub msg: Duration,
    pub stub: Duration,
    pub sharc: Duration,
    /// The pass reached the poll loop (not the step limit, not a fault).
    pub idle: bool,
}

impl PassTiming {
    /// Sum of the three components.
    pub fn total(&self) -> Duration {
        self.msg + self.stub + self.sharc
    }
}

/// Mean, median and worst of a series of seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
    pub worst: f64,
}

/// [`Stats`] of `v` (all zero for an empty series).
pub fn stats(v: &[f64]) -> Stats {
    if v.is_empty() {
        return Stats::default();
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    Stats {
        n: s.len(),
        mean: s.iter().sum::<f64>() / s.len() as f64,
        median: s[s.len() / 2],
        worst: s[s.len() - 1],
    }
}

/// Rewrites the DMA-filled input words with their current value (cost only, no data change).
pub fn refresh_inputs(core: &mut Core) {
    for (base, n) in INPUT_RANGES {
        for a in base..base + n {
            if let Ok(v) = core.dm_read(a) {
                let _ = core.dm_write(a, v);
            }
        }
    }
}

/// PNO core 1 ping-pong pointer `(word, ping, pong)`: the P0I ISR toggles it before every pass
/// (`docs/research/pno-full-pass.md`, VERIFIED by `sharc-pass`, which does the same).
pub const PNO_PING_PONG: (u32, u32, u32) = (0xB_0158, 0x28AC_01E0, 0x28AC_0000);

/// Toggles the ping-pong pointer for pass `p` (even passes ping).
pub fn set_ping_pong(core: &mut Core, pp: (u32, u32, u32), p: u32) {
    let _ = core.dm_write(pp.0, if p & 1 == 0 { pp.1 } else { pp.2 });
}

/// Starts a pass: the receive interrupt (Core1 sections) or the mailbox write.
fn start(core: &mut Core, cfg: &PassConfig) {
    match cfg.irq {
        // with the SEC model on (`--sec-id`) the source goes through its CSID/END handshake
        Some((_, status)) if core.sc57x().is_some() => core.sc57x_raise_source(status),
        Some((vector, status)) => {
            core.preset_device(SEC_CSID, status);
            core.raise_interrupt(vector);
        }
        None => {
            let _ = core.dm_write(cfg.trigger, 1);
        }
    }
}

/// Trigger write and interpreter steps to the poll loop; no snapshot, no coverage bookkeeping.
/// Returns `(steps, reached the idle loop)`; a fault ends the pass as not idle.
pub fn lean_pass(core: &mut Core, cfg: &PassConfig) -> (u64, bool) {
    start(core, cfg);
    let start = core.steps;
    while core.steps - start < cfg.max_steps {
        if core.step().is_err() {
            return (core.steps - start, false);
        }
        if core.pc == cfg.idle_pc && core.dm_read(cfg.trigger) == Ok(0) {
            return (core.steps - start, true);
        }
    }
    (core.steps - start, false)
}

/// [`lean_pass`] that also records every executed program counter (untimed coverage runs).
pub fn probe_pass(core: &mut Core, cfg: &PassConfig) -> (u64, bool, BTreeSet<u32>) {
    start(core, cfg);
    let (start, mut visited) = (core.steps, BTreeSet::new());
    while core.steps - start < cfg.max_steps {
        visited.insert(core.pc);
        if core.step().is_err() {
            return (core.steps - start, false, visited);
        }
        if core.pc == cfg.idle_pc && core.dm_read(cfg.trigger) == Ok(0) {
            return (core.steps - start, true, visited);
        }
    }
    (core.steps - start, false, visited)
}

/// Runs `warmup + passes` passes, delivering `msgs` before the pass they name. Timing rows are
/// returned for all passes (the first `warmup` are the warm-up, dropped by the caller's stats).
pub fn run_system(
    core: &mut Core,
    cfg: &PassConfig,
    msgs: &[Msg],
    msg_base: u32,
    total_passes: u32,
    ping_pong: Option<(u32, u32, u32)>,
) -> Result<Vec<PassTiming>, String> {
    let mut rows = Vec::with_capacity(total_passes as usize);
    // messages in pass order (stable: file order inside a pass); a cursor replaces a per-pass scan
    let mut due: Vec<&Msg> = msgs.iter().collect();
    due.sort_by_key(|m| m.pass);
    let mut next = 0;
    for p in 0..total_passes {
        let t0 = Instant::now();
        if let Some(pp) = ping_pong {
            set_ping_pong(core, pp, p);
        }
        let mut n = 0;
        while next < due.len() && due[next].pass <= p {
            deliver(core, msg_base, due[next])?;
            next += 1;
            n += u32::from(due[next - 1].pass == p);
        }
        let t1 = Instant::now();
        refresh_inputs(core);
        let t2 = Instant::now();
        let (steps, idle) = lean_pass(core, cfg);
        let t3 = Instant::now();
        rows.push(PassTiming {
            steps,
            msgs: n,
            msg: t1 - t0,
            stub: t2 - t1,
            sharc: t3 - t2,
            idle,
        });
        if !idle {
            break;
        }
    }
    Ok(rows)
}

/// Series of one component after dropping `warmup` rows, in seconds.
pub fn series(rows: &[PassTiming], warmup: usize, f: impl Fn(&PassTiming) -> Duration) -> Vec<f64> {
    rows.iter()
        .skip(warmup)
        .map(|r| f(r).as_secs_f64())
        .collect()
}

/// FNV-1a hash of the data words `lo..lo + len` (unreadable words hash as 0).
pub fn region_hash(core: &mut Core, lo: u32, len: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for a in lo..lo + len {
        let v = core.dm_read(a).unwrap_or(0);
        for b in v.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    }
    h
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
    fn pass_period_is_two_frames() {
        assert!((pass_period_s(FRAME_RATE) * 1e6 - 46.08).abs() < 0.01);
    }

    #[test]
    fn stats_sort_and_summarise() {
        assert_eq!(stats(&[]), Stats::default());
        let s = stats(&[3.0, 1.0, 2.0]);
        assert_eq!((s.n, s.mean, s.median, s.worst), (3, 2.0, 2.0, 3.0));
    }

    #[test]
    fn lean_pass_matches_run_pass_step_count() {
        let mut a = clearing_core();
        let mut b = clearing_core();
        let r = super::super::pass::run_pass(&mut a, &cfg());
        assert_eq!(lean_pass(&mut b, &cfg()), (r.steps, true));
    }

    #[test]
    fn irq_start_leaves_the_mailbox_alone_and_ping_pong_alternates() {
        let mut c = clearing_core();
        let irq = PassConfig {
            irq: Some((15, 0x41)),
            ..cfg()
        };
        let (_, idle) = lean_pass(&mut c, &irq);
        assert!(idle);
        assert_eq!(c.dm_read(SEC_CSID), Ok(0x41));
        set_ping_pong(&mut c, PNO_PING_PONG, 0);
        assert_eq!(c.dm_read(0xB_0158), Ok(0x28AC_01E0));
        set_ping_pong(&mut c, PNO_PING_PONG, 1);
        assert_eq!(c.dm_read(0xB_0158), Ok(0x28AC_0000));
    }

    #[test]
    fn probe_pass_records_the_executed_pcs() {
        let mut c = clearing_core();
        let (steps, idle, visited) = probe_pass(&mut c, &cfg());
        assert_eq!((steps, idle), (1, true));
        assert_eq!(visited.into_iter().collect::<Vec<_>>(), vec![0x12_0000]);
    }

    #[test]
    fn messages_are_delivered_before_their_pass_and_timed() {
        let mut c = clearing_core();
        let m = [Msg {
            pass: 0,
            word1: 0x10,
            word2: 7,
        }];
        let rows = run_system(&mut c, &cfg(), &m, 0xB_0000, 3, None).unwrap();
        // the tiny core idles once (its code is a single store); pass 1 hits the step limit and
        // ends the run
        assert_eq!(rows.len(), 2);
        assert!(!rows[1].idle);
        assert_eq!((rows[0].msgs, rows[0].steps, rows[0].idle), (1, 1, true));
        assert_eq!(c.dm_read(0xB_0010), Ok(7));
        assert_eq!(series(&rows, 0, PassTiming::total).len(), 2);
        assert_eq!(
            series(&rows, 1, PassTiming::total).len(),
            1,
            "warm-up rows are dropped"
        );
    }

    #[test]
    fn a_pass_that_never_idles_stops_the_run() {
        let mut c = clearing_core();
        c.pc = 0x12_0001; // skips the clearing store: the mailbox stays 1
        let rows = run_system(&mut c, &cfg(), &[], 0, 5, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].idle);
    }

    #[test]
    fn refresh_keeps_values_and_hash_tracks_changes() {
        let mut c = clearing_core();
        c.dm_write(0xB_0078, 0xDEAD).unwrap();
        let h0 = region_hash(&mut c, 0xB_0070, 0x20);
        refresh_inputs(&mut c);
        assert_eq!(region_hash(&mut c, 0xB_0070, 0x20), h0);
        c.dm_write(0xB_0079, 1).unwrap();
        assert_ne!(region_hash(&mut c, 0xB_0070, 0x20), h0);
    }
}
