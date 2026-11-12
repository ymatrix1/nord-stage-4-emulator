//! Interrupt controller of [`Core`]: the IRPTL/LIRPTL latches, IMASK/IMASKP masking, priority
//! arbitration, nesting, vector-table dispatch, `RTI` and `IDLE`.
//!
//! Model (SHARC Processor Programming Reference rev 2.4, chapter 4 "Interrupt Processing" and
//! appendix B): an interrupt is raised into a latch bit; it is taken at an instruction boundary
//! when it is unmasked, `MODE1.IRPTEN` is set, no delayed branch is in flight and, while another
//! interrupt is being serviced, `MODE1.NESTM` is set and it has a higher priority (lower vector
//! number). Taking it pushes the return address, clears the latch, pushes the status stack for
//! `IRQ2-0` and the timer, marks the interrupt as in service and jumps to
//! `ivt_base + 12 * vector` (four 48-bit ISA words per vector). The vector slot is executed in
//! ISA parcel order (least significant parcel first) through the fetch window below.
//!
//! Not modelled: the one-instruction delay of a higher-priority interrupt that arrives right
//! after a lower one (PRM 4-43), the cycle-exact pipeline stalls that postpone interrupts
//! (PRM 4-39) and the `JUMP (CI)` self-nesting form.
use super::vectors::{slot, Slot, VECTOR_COUNT};
use super::Core;

/// MODE1 bit 11: nesting of interrupts enabled.
pub(super) const NESTM: u32 = 1 << 11;
/// MODE1 bit 12: global interrupt enable.
pub(super) const IRPTEN: u32 = 1 << 12;
/// Short words between two vectors: four 48-bit instructions of three parcels each.
/// VERIFIED: the images hold the reset jump of vector 1 in its second word, `0x120000 + 12 + 3`
/// (`docs/research/dsp-image-hunt.md` 5.20), and `tools`' scan of the six images shows 32 vectors
/// of 12 parcels each up to `0x12017F`.
pub(super) const VECTOR_PARCELS: u32 = 12;
/// Short-word address of the internal vector table. VERIFIED: in all six firmware images the
/// reset vector (vector 1) at `0x12000C` jumps to the section entry value.
pub const IVT_BASE: u32 = 0x12_0000;
/// Cycles from recognising an interrupt to fetching its first vector instruction.
/// ASSUMED from PRM 4-30 ("four instruction cycles" to branch, five from IDLE).
const ENTRY_STALL: u64 = 4;

/// Interrupts that push the status stack: `TMZHI`, `IRQ2I`, `IRQ1I`, `IRQ0I`, `TMZLI` (PRM 4-14).
const PUSHES_STATUS: [u8; 5] = [4, 8, 9, 10, 32];

/// An interrupt whose service routine is running.
#[derive(Clone, Copy, Debug)]
struct Serviced {
    vector: u8,
    status_pushed: bool,
}

/// State of the interrupt controller.
pub(super) struct IrqState {
    pub irptl: u32,
    /// LIRPTL latch bits 0-9 and mask bits 10-19 (the mask pointers 20-29 are derived).
    pub lirptl: u32,
    serviced: Vec<Serviced>,
    pub idle: bool,
    pub ivt_base: u32,
    /// Short-word window `[window_lo, window_lo + window_len)` fetched in ISA parcel order.
    window_lo: u32,
    pub window_len: u32,
    /// External interrupts scheduled as `(cycle, vector)`, ascending.
    scheduled: Vec<(u64, u8)>,
    /// Cycles spent without executing an instruction (entry latency, idle fast-forward).
    pub stall: u64,
    /// Instruction count at which `step` must look at the controller again (0 = at the next
    /// step): the next event cycle minus the stall cycles, so the hot path compares one counter.
    pub due: u64,
    wake_at: u64,
    pub taken: u64,
}

impl IrqState {
    pub fn new() -> Self {
        IrqState {
            irptl: 0,
            lirptl: 0,
            serviced: Vec::with_capacity(VECTOR_COUNT),
            idle: false,
            ivt_base: IVT_BASE,
            window_lo: 0,
            window_len: 0,
            scheduled: Vec::new(),
            stall: 0,
            due: 0,
            wake_at: u64::MAX,
            taken: 0,
        }
    }
}

impl Core {
    /// Makes the controller look at its inputs at the next instruction boundary.
    #[inline]
    pub(super) fn poke_irq(&mut self) {
        self.irq.due = 0;
    }

    fn latched(&self, v: u8) -> bool {
        match slot(v) {
            Slot::Irptl(b) => self.irq.irptl >> b & 1 == 1,
            Slot::Lirptl(j) => self.irq.lirptl >> j & 1 == 1,
        }
    }

    fn unmasked(&self, v: u8) -> bool {
        match slot(v) {
            Slot::Irptl(b) => v <= 1 || self.imask >> b & 1 == 1,
            Slot::Lirptl(j) => self.irq.lirptl >> (10 + j) & 1 == 1,
        }
    }

    fn set_latch(&mut self, v: u8, on: bool) {
        let (reg, bit) = match slot(v) {
            Slot::Irptl(b) => (&mut self.irq.irptl, b),
            Slot::Lirptl(j) => (&mut self.irq.lirptl, j),
        };
        *reg = *reg & !(1 << bit) | u32::from(on) << bit;
    }

    /// Raises interrupt `vector` (0..[`VECTOR_COUNT`]) into its latch. Returns false when the
    /// vector is invalid or is in service (the sequencer clears the latch of the running ISR
    /// every cycle, PRM 4-35).
    pub fn raise_interrupt(&mut self, vector: u8) -> bool {
        if vector as usize >= VECTOR_COUNT || self.irq.serviced.iter().any(|s| s.vector == vector) {
            return false;
        }
        self.set_latch(vector, true);
        self.poke_irq();
        true
    }

    /// Schedules interrupt `vector` to be raised when the core clock reaches `cycle`.
    pub fn schedule_interrupt(&mut self, cycle: u64, vector: u8) {
        let at = self.irq.scheduled.partition_point(|s| s.0 <= cycle);
        self.irq.scheduled.insert(at, (cycle, vector));
        self.poke_irq();
    }

    /// Vector of the interrupt in service with the highest priority, if any.
    pub fn servicing(&self) -> Option<u8> {
        self.irq.serviced.iter().map(|s| s.vector).min()
    }

    /// Whether the core sleeps in `IDLE`.
    pub fn is_idle(&self) -> bool {
        self.irq.idle
    }

    /// Whether something can still end an `IDLE`: a scheduled interrupt, a running timer or a
    /// latched interrupt that is not blocked.
    pub fn can_wake(&self) -> bool {
        self.irq.wake_at != u64::MAX
            || !self.irq.scheduled.is_empty()
            || self.pending_vector().is_some()
    }

    /// Number of interrupts dispatched so far.
    pub fn interrupts_taken(&self) -> u64 {
        self.irq.taken
    }

    /// Moves the interrupt vector table (SYSCTL.IIVT places it in internal RAM at [`IVT_BASE`]).
    pub fn set_ivt_base(&mut self, short_word: u32) {
        self.irq.ivt_base = short_word;
    }

    /// Highest-priority interrupt that may be taken now, by the masking and nesting rules.
    pub(super) fn pending_vector(&self) -> Option<u8> {
        let enabled = self.mode1 & IRPTEN != 0;
        let active = self.servicing();
        (0..VECTOR_COUNT as u8).find(|&v| {
            if !self.latched(v) {
                return false;
            }
            if v <= 1 {
                return true; // EMUI and RSTI cannot be masked
            }
            enabled && self.unmasked(v) && active.is_none_or(|a| self.mode1 & NESTM != 0 && v < a)
        })
    }

    /// Takes interrupt `v`: pushes the return address, latches the in-service state and jumps to
    /// its vector.
    fn take_interrupt(&mut self, v: u8) {
        self.set_latch(v, false);
        self.push_return(self.pc);
        let status_pushed = PUSHES_STATUS.contains(&v);
        if status_pushed {
            self.push_status();
        }
        self.irq.serviced.push(Serviced {
            vector: v,
            status_pushed,
        });
        let vector_pc = if self.isa_mode() {
            // ISA mode: four 48-bit words per vector from the table at the start of block 0
            super::isa::ISA_BASE + 4 * v as u32
        } else {
            let pc = self.irq.ivt_base + VECTOR_PARCELS * v as u32;
            self.irq.window_lo = pc;
            self.irq.window_len = VECTOR_PARCELS;
            pc
        };
        self.pc = vector_pc;
        self.irq.stall += ENTRY_STALL + u64::from(self.irq.idle);
        self.irq.idle = false;
        self.irq.taken += 1;
    }

    /// `RTI` bookkeeping, after the return address was popped: ends the service of the innermost
    /// interrupt and restores the status stack if it was pushed on entry.
    pub(super) fn return_from_interrupt(&mut self) {
        if let Some(s) = self.irq.serviced.pop() {
            if s.status_pushed {
                self.pop_status();
            }
            self.poke_irq();
        }
    }

    /// `IDLE`: the core sleeps until an interrupt is taken.
    pub(super) fn enter_idle(&mut self) {
        if self.periph.is_some() {
            self.periph_boot_wait(); // a sleeping core lets the host DMA finish
        }
        self.irq.idle = true;
        self.poke_irq();
    }

    /// The parcels of an instruction fetched in the vector window, in VISA (most significant
    /// first) order, or `None` outside it.
    #[inline]
    pub(super) fn isa_window_parcels(&self, pc: u32) -> Option<[u16; 3]> {
        if self.irq.window_len == 0 || pc.wrapping_sub(self.irq.window_lo) >= self.irq.window_len {
            return None;
        }
        Some([self.parcel(pc + 2), self.parcel(pc + 1), self.parcel(pc)])
    }

    /// Closes the vector window once the pc has left it.
    #[inline]
    pub(super) fn close_window_if_left(&mut self) {
        if self.pc.wrapping_sub(self.irq.window_lo) >= self.irq.window_len {
            self.irq.window_len = 0;
        }
    }

    /// Value of IMASKP: the in-service interrupts in IRPTL bit positions.
    pub(super) fn imaskp_value(&self) -> u32 {
        self.irq
            .serviced
            .iter()
            .fold(0, |acc, s| match slot(s.vector) {
                Slot::Irptl(b) => acc | 1 << b,
                Slot::Lirptl(_) => acc,
            })
    }

    /// Value of LIRPTL: latch and mask bits plus the in-service mask pointers (bits 20-29).
    pub(super) fn lirptl_value(&self) -> u32 {
        self.irq
            .serviced
            .iter()
            .fold(self.irq.lirptl & 0xF_FFFF, |acc, s| match slot(s.vector) {
                Slot::Lirptl(j) => acc | 1 << (20 + j),
                Slot::Irptl(_) => acc,
            })
    }

    /// Software write of IRPTL: EMUI/RSTI are read-only and the latch of an interrupt in service
    /// stays clear.
    pub(super) fn write_irptl(&mut self, v: u32) {
        let held = self.imaskp_value();
        self.irq.irptl = v & !0b11 & !held;
        self.poke_irq();
    }

    /// Software write of LIRPTL (latch and mask bits only).
    pub(super) fn write_lirptl(&mut self, v: u32) {
        let held = self.lirptl_value() >> 20 & 0x3FF;
        self.irq.lirptl = v & 0xF_FFFF & !held;
        self.poke_irq();
    }

    /// Looks at the controller at an instruction boundary: timer expiry, scheduled interrupts and
    /// dispatch. Returns true when the step is consumed by `IDLE` (no instruction to execute).
    pub(super) fn sequencer_event(&mut self) -> bool {
        let now = self.cycles();
        self.timer_sync(now);
        while let Some(&(at, v)) = self.irq.scheduled.first() {
            if at > now {
                break;
            }
            self.irq.scheduled.remove(0);
            self.raise_interrupt(v);
        }
        let mut retry = false;
        if let Some(v) = self.pending_vector() {
            if self.pending.is_none() {
                self.take_interrupt(v);
                retry = true;
            } else {
                retry = true; // inside the delay slots of a branch: try again next step
                              // an IDLE in a delay slot ends as soon as an interrupt is pending (the boot
                              // kernel's `JUMP (DB)` + `IDLE`); the interrupt is taken after the branch
                self.irq.idle = false;
            }
        }
        let wake = self
            .timer_next_event()
            .min(self.irq.scheduled.first().map_or(u64::MAX, |s| s.0));
        self.irq.wake_at = wake;
        let at_cycle = if retry || self.irq.idle { 0 } else { wake };
        self.irq.due = at_cycle.saturating_sub(self.irq.stall);
        if !self.irq.idle {
            return false;
        }
        let here = self.cycles();
        if wake != u64::MAX && wake > here + 1 {
            self.irq.stall += wake - here - 1;
        }
        self.steps += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_latches_and_scheduling_orders_events() {
        let mut c = Core::new(&[]);
        assert!(c.raise_interrupt(11));
        assert!(!c.raise_interrupt(42), "no such vector");
        assert_eq!(c.irq.irptl, 1 << 11);
        c.schedule_interrupt(50, 3);
        c.schedule_interrupt(20, 4);
        assert_eq!(c.irq.scheduled, vec![(20, 4), (50, 3)]);
        assert!(c.can_wake());
    }
}
