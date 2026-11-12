//! Core clock and the core timer (`TCOUNT`, `TPERIOD`, `MODE2.TIMEN`) of [`Core`].
//!
//! The clock counts cycles as executed instructions plus the cycles the core spent without
//! executing (interrupt entry latency, `IDLE`): ASSUMED one cycle per instruction, i.e. no cache
//! misses, memory conflicts or pipeline stalls. `EMUCLK` reads this clock.
//!
//! Timer (PRM chapter 5): while `TIMEN` is set `TCOUNT` decrements every cycle; when it
//! decrements to zero the timer latches `TMZHI` and `TMZLI`, and on the next cycle `TCOUNT` is
//! reloaded from `TPERIOD`, so the interrupts are `TPERIOD + 1` cycles apart. Enabling takes two
//! cycles and disabling one (Figure 5-2). The count is computed lazily from the cycle clock, so a
//! running timer costs nothing per instruction.
use super::Core;

/// MODE2 bit 5: core timer enable (PRM Table A-2; the firmware sets `0x20` after loading TCOUNT).
pub(super) const TIMEN: u32 = 1 << 5;
/// Cycles between setting `TIMEN` and the first decrement. ASSUMED from PRM Figure 5-2.
const START_LATENCY: u64 = 2;
/// Cycles between clearing `TIMEN` and the count stopping. ASSUMED from PRM Figure 5-2.
const STOP_LATENCY: u64 = 1;

/// Core timer registers and the lazily evaluated count.
#[derive(Default)]
pub(super) struct TimerState {
    /// TCOUNT as last written (or frozen when the timer stopped).
    tcount: u32,
    tperiod: u32,
    running: bool,
    /// Cycle at which `base_count` was valid and decrementing starts.
    base_cycle: u64,
    base_count: u32,
    /// Expiries already latched since `base_cycle`.
    seen: u64,
}

impl TimerState {
    /// Cycles after `base_cycle` of the first expiry: the count reaches zero, or, when it started
    /// at zero, one reload plus a full period later.
    fn first_expiry(&self) -> u64 {
        if self.base_count > 0 {
            u64::from(self.base_count)
        } else {
            u64::from(self.tperiod) + 1
        }
    }

    fn period(&self) -> u64 {
        u64::from(self.tperiod) + 1
    }

    /// TCOUNT `k` cycles after `base_cycle`.
    fn count_after(&self, k: u64) -> u32 {
        let c0 = u64::from(self.base_count);
        if k <= c0 {
            return (c0 - k) as u32;
        }
        let since_reload = k - (c0 + 1);
        (u64::from(self.tperiod) - since_reload % self.period()) as u32
    }

    /// Number of expiries within `k` cycles after `base_cycle`.
    fn expiries_by(&self, k: u64) -> u64 {
        let f = self.first_expiry();
        if k < f {
            0
        } else {
            1 + (k - f) / self.period()
        }
    }
}

impl Core {
    /// The core clock: instructions executed plus cycles spent waiting.
    pub fn cycles(&self) -> u64 {
        self.steps + self.irq.stall
    }

    /// Latches `TMZHI`/`TMZLI` when the timer expired up to cycle `now`.
    pub(super) fn timer_sync(&mut self, now: u64) {
        let t = &self.timer;
        if !t.running || now < t.base_cycle {
            return;
        }
        let e = t.expiries_by(now - t.base_cycle);
        if e > t.seen {
            self.timer.seen = e;
            self.raise_interrupt(4);
            self.raise_interrupt(32);
        }
    }

    /// Cycle of the next timer expiry, `u64::MAX` when the timer is stopped.
    pub(super) fn timer_next_event(&self) -> u64 {
        let t = &self.timer;
        if !t.running {
            return u64::MAX;
        }
        t.base_cycle + t.first_expiry() + t.seen * t.period()
    }

    /// TCOUNT as a program reads it.
    pub(super) fn read_tcount(&self) -> u32 {
        let t = &self.timer;
        let now = self.cycles();
        if t.running && now > t.base_cycle {
            t.count_after(now - t.base_cycle)
        } else if t.running {
            t.base_count
        } else {
            t.tcount
        }
    }

    pub(super) fn read_tperiod(&self) -> u32 {
        self.timer.tperiod
    }

    /// Restarts the count from `count` at the current cycle, keeping the running state.
    fn rebase_timer(&mut self, count: u32) {
        let now = self.cycles();
        self.timer_sync(now);
        let t = &mut self.timer;
        t.tcount = count;
        t.base_count = count;
        t.base_cycle = now;
        t.seen = 0;
        self.poke_irq();
    }

    /// An explicit TCOUNT write takes priority over decrement and reload (PRM 5-2).
    pub(super) fn write_tcount(&mut self, v: u32) {
        self.rebase_timer(v);
    }

    pub(super) fn write_tperiod(&mut self, v: u32) {
        if self.timer.running {
            let now = self.read_tcount();
            self.rebase_timer(now);
        }
        self.timer.tperiod = v;
        self.poke_irq();
    }

    /// Applies a change of `MODE2.TIMEN`.
    pub(super) fn set_timer_enable(&mut self, on: bool) {
        let now = self.cycles();
        self.timer_sync(now);
        if on && !self.timer.running {
            let t = &mut self.timer;
            t.running = true;
            t.base_count = t.tcount;
            t.base_cycle = now + START_LATENCY;
            t.seen = 0;
        } else if !on && self.timer.running {
            let stop = now + STOP_LATENCY;
            let t = &mut self.timer;
            t.tcount = if stop <= t.base_cycle {
                t.base_count
            } else {
                t.count_after(stop - t.base_cycle)
            };
            t.running = false;
        }
        self.poke_irq();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer_core(count: u32, period: u32) -> Core {
        let mut c = Core::new(&[]);
        c.write_ureg(0x6F, count);
        c.write_ureg(0x6E, period);
        c.write_ureg(0x74, TIMEN); // MODE2
        c
    }

    /// Runs `n` NOP-cycles (zero memory is the 48-bit NOP).
    fn tick(c: &mut Core, n: u64) {
        assert_eq!(c.run(n), None);
    }

    #[test]
    fn tcount_decrements_once_per_cycle_after_the_start_latency() {
        let mut c = timer_core(100, 1000);
        assert_eq!(c.read_ureg(0x6F), 100, "not running yet inside the latency");
        tick(&mut c, 2);
        assert_eq!(c.read_ureg(0x6F), 100);
        tick(&mut c, 10);
        assert_eq!(c.read_ureg(0x6F), 90);
    }

    #[test]
    fn expiry_latches_both_timer_interrupts_and_reloads_from_tperiod() {
        let mut c = timer_core(5, 9);
        tick(&mut c, 2 + 4);
        assert_eq!(
            c.read_ureg(0x7A) & (1 << 4 | 1 << 22),
            0,
            "count 1: not expired"
        );
        tick(&mut c, 1);
        assert_eq!(c.read_ureg(0x6F), 0);
        c.timer_sync(c.cycles()); // what the next instruction boundary does
        assert_eq!(c.read_ureg(0x7A) & (1 << 4 | 1 << 22), 1 << 4 | 1 << 22);
        tick(&mut c, 1);
        assert_eq!(
            c.read_ureg(0x6F),
            9,
            "reloaded from TPERIOD the cycle after zero"
        );
        // TPERIOD + 1 cycles between expiries
        c.write_irptl(0);
        tick(&mut c, 9);
        c.timer_sync(c.cycles());
        assert_eq!(c.read_ureg(0x7A) & (1 << 4), 1 << 4);
    }

    #[test]
    fn firmware_style_stopwatch_runs_with_the_maximum_period() {
        // FX start-up: TPERIOD = 0xFFFFFFFF, TCOUNT = 0x2400, TIMEN set
        let mut c = timer_core(0x2400, 0xFFFF_FFFF);
        tick(&mut c, 2 + 0x2400);
        assert_eq!(c.read_ureg(0x6F), 0);
        tick(&mut c, 1);
        assert_eq!(c.read_ureg(0x6F), 0xFFFF_FFFF);
        assert_eq!(c.timer_next_event(), 2 + 0x2400 + 0x1_0000_0000);
    }

    #[test]
    fn disabling_freezes_the_count_and_a_tcount_write_restarts_it() {
        let mut c = timer_core(1000, 5000);
        tick(&mut c, 12);
        c.write_ureg(0x74, 0); // TIMEN off
        let frozen = c.read_ureg(0x6F);
        tick(&mut c, 50);
        assert_eq!(c.read_ureg(0x6F), frozen);
        assert!(
            (988..=990).contains(&frozen),
            "stops one cycle late: {frozen}"
        );
        c.write_ureg(0x6F, 7);
        assert_eq!(c.read_ureg(0x6F), 7);
    }

    #[test]
    fn timer_expiry_wakes_idle_through_an_unmasked_interrupt() {
        // TMZHI (vector 4) unmasked: IDLE sleeps until the timer expires, then the ISR runs
        let mut c = timer_core(1000, 0xFFFF_FFFF);
        c.set_ivt_base(0x4000);
        c.mode1 |= 1 << 12;
        c.imask |= 1 << 4;
        let at = 0x4000 + 12 * 4;
        let jump = [0x0100u16, 0x0000, 0x063e]; // JUMP 0x100 in ISA parcel order
        for (k, p) in jump.iter().enumerate() {
            c.sram[(at as usize + k) * 2..][..2].copy_from_slice(&p.to_le_bytes());
        }
        c.sram[0x100 * 2..][..2].copy_from_slice(&0x0bbeu16.to_le_bytes());
        c.sram[0..2].copy_from_slice(&0x0091u16.to_le_bytes());
        assert_eq!(c.step(), Ok(())); // IDLE
        assert!(c.is_idle() && c.can_wake());
        assert_eq!(c.step(), Ok(())); // sleeps until the expiry
        assert_eq!(c.step(), Ok(())); // interrupt taken, vector JUMP executed
        assert!(!c.is_idle());
        assert_eq!(c.pc, 0x100);
        assert_eq!(c.servicing(), Some(4));
        assert!(
            c.cycles() >= 1000,
            "slept until the expiry at cycle {}",
            c.cycles()
        );
    }
}
