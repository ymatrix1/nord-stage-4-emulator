//! Shared pieces of the ARM0 benchmarks (`arm-tail-bench`, `arm-rt-bench`): boot of the user's
//! OS image with the full board, the six benchmark phases and their MIDI input script, one timed
//! run of all phases, thread CPU cycles.

use nord_arm::machine::idle::IdleStats;
use nord_arm::machine::metrics::{digest_messages, percentile};
use nord_arm::machine::uart::{note_off, note_on};
use nord_arm::machine::{Machine, RunStats, Stop};
use std::time::Instant;

/// CPU cycles consumed by the calling thread (immune to preemption by other processes, unlike wall
/// clock; still sensitive to SMT and cache contention). Wall clock on other platforms.
#[cfg(windows)]
pub fn thread_cycles() -> u64 {
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
    }
    let mut c = 0u64;
    // SAFETY: both calls are plain Win32 queries on the current thread with a valid out pointer.
    unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut c) };
    c
}

#[cfg(not(windows))]
pub fn thread_cycles() -> u64 {
    use std::sync::OnceLock;
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

pub const KEYS8: [u8; 8] = [60, 62, 64, 65, 67, 69, 71, 72];
/// Arena pages reserved before a timed run (64 MB) so on-demand mapping never reallocates.
pub const RESERVE_PAGES: usize = 16_384;

fn act_none(_: &mut Machine) {}
fn act_on1(m: &mut Machine) {
    m.send_midi(&note_on(KEYS8[0], 100, 0));
}
fn act_off1(m: &mut Machine) {
    m.send_midi(&note_off(KEYS8[0], 0));
}
fn act_on8(m: &mut Machine) {
    KEYS8.iter().for_each(|k| m.send_midi(&note_on(*k, 100, 0)));
}
fn act_off8(m: &mut Machine) {
    KEYS8.iter().for_each(|k| m.send_midi(&note_off(*k, 0)));
}

pub type Action = fn(&mut Machine);

/// Phase plan, same as `arm-lle-bench` (ticks scaled by `scale` percent).
pub fn plan(scale: usize) -> [(&'static str, usize, Action); 6] {
    let n = |x: usize| (x * scale / 100).max(1);
    [
        ("load", n(800), act_none),
        ("idle", n(300), act_none),
        ("key1 on", n(200), act_on1),
        ("key1 off", n(200), act_off1),
        ("key8 on", n(200), act_on8),
        ("key8 off", n(200), act_off8),
    ]
}

/// One run of all phases at one budget.
#[derive(Default)]
pub struct Run {
    pub ms: Vec<f64>,
    pub cyc: Vec<u64>,
    pub exec: Vec<u64>,
    pub stats: RunStats,
    pub digest: u64,
    pub state: u64,
    pub phase_ticks: Vec<usize>,
}

pub fn run_all(base: &Machine, budget: u64, scale: usize) -> Run {
    let mut m = base.clone();
    let (mut run, mut idle, mut buf) = (
        Run::default(),
        IdleStats::default(),
        Vec::with_capacity(4096),
    );
    let mut msgs = Vec::new();
    let s0 = m.stats;
    for (_, ticks, action) in plan(scale) {
        run.phase_ticks.push(ticks);
        for i in 0..ticks {
            let e0 = idle.executed;
            buf.clear();
            let (t, c0) = (Instant::now(), thread_cycles());
            if i == 0 {
                action(&mut m);
            }
            let s = m.run_tick_idle(budget, &mut idle);
            m.drain_messages_into(&mut buf);
            run.ms.push(t.elapsed().as_secs_f64() * 1e3);
            run.cyc.push(thread_cycles() - c0);
            assert!(matches!(s, Stop::Budget), "{s:?}");
            run.exec.push(idle.executed - e0);
            msgs.extend_from_slice(&buf);
        }
    }
    run.stats = m.stats.since(&s0);
    run.digest = digest_messages(&msgs);
    run.state = m.state_hash();
    run
}

/// `[min, median, max]` of the samples.
pub fn spread(v: &[f64]) -> [f64; 3] {
    [percentile(v, 0.0), percentile(v, 0.5), percentile(v, 1.0)]
}

/// The machine of `arm-lle-bench`, booted to the main loop with panel, I2C EEPROM, UART2, DSP
/// ports and the real timer; the arena is reserved so timed runs never reallocate.
pub fn boot(code: &[u8], sram: &[u8], eeprom: &[u8]) -> Machine {
    let mut m = Machine::new();
    m.prepare_system(code, sram).expect("init array");
    m.bus.i2c.as_mut().expect("i2c").load_image(eeprom);
    let (_, _, stop) = m.boot_system(1_500_000_000, &mut IdleStats::default());
    assert!(matches!(stop, Stop::StopPc), "boot stopped with {stop:?}");
    m.install_dsp_ports();
    m.install_dsp_return();
    m.enable_real_timer();
    m.bus.mem.reserve_pages(RESERVE_PAGES);
    m
}

/// The MIDI input of [`plan`] as `(tick, bytes)` for the producer thread: the same bytes at the
/// first tick of each phase as the phase actions send.
pub fn script(scale: usize) -> Vec<(u64, Vec<u8>)> {
    let mut out = Vec::new();
    let mut tick = 0u64;
    for (name, ticks, _) in plan(scale) {
        match name {
            "key1 on" => out.push((tick, note_on(KEYS8[0], 100, 0).to_vec())),
            "key1 off" => out.push((tick, note_off(KEYS8[0], 0).to_vec())),
            "key8 on" => out.extend(KEYS8.iter().map(|k| (tick, note_on(*k, 100, 0).to_vec()))),
            "key8 off" => out.extend(KEYS8.iter().map(|k| (tick, note_off(*k, 0).to_vec()))),
            _ => {}
        }
        tick += ticks as u64;
    }
    out
}

/// Total ticks of [`plan`].
pub fn total_ticks(scale: usize) -> u64 {
    plan(scale).iter().map(|p| p.1 as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_has_one_entry_per_midi_message_at_the_phase_starts() {
        let s = script(100);
        assert_eq!(s.len(), 1 + 1 + 8 + 8);
        assert_eq!(s[0].0, 1100);
        assert_eq!(s[1].0, 1300);
        assert_eq!(s[2].0, 1500);
        assert_eq!(s.last().unwrap().0, 1700);
        assert_eq!(total_ticks(100), 1900);
    }

    #[test]
    fn spread_is_min_median_max() {
        assert_eq!(spread(&[3.0, 1.0, 2.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn plan_keeps_every_phase_at_any_scale() {
        assert_eq!(plan(100).map(|p| p.1), [800, 300, 200, 200, 200, 200]);
        assert!(plan(0).iter().all(|p| p.1 == 1));
    }

    #[test]
    fn thread_cycles_never_decreases() {
        let a = thread_cycles();
        assert!(thread_cycles() >= a);
    }
}
