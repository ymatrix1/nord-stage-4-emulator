//! System registers of [`Core`] (universal register groups 6 and 7, Table 10-2), the PC, loop and
//! status stacks with their depth limits and the sticky status that feeds the arithmetic
//! exception interrupts.
//!
//! Provenance: register codes are PRM Table 10-2 (VERIFIED against the firmware: `0x76be 0x82bf`
//! is `R5 = EMUCLK` in ADI's listing, `BIT SET MODE2 0x20` after loading `TCOUNT` starts the
//! timer, `IMASK` is code `0x7B`). Stack depths are PRM 4-12, 4-37 and 6-86: PC stack 30, status
//! stack 15, loop stack 6.
use super::interrupts::{IRPTEN, NESTM};
use super::stacks::{LSEM, LSOV, PCEM, PCFL, PC_STACK_DEPTH, SSEM, SSOV, STATUS_STACK_DEPTH};
use super::timer::TIMEN;
use super::{Core, Flags};

/// Reset value of MMASK: only `PEYEN` is set (PRM A-44), so SIMD is switched off in the service
/// routine of an `IRQ2-0` or timer interrupt.
pub(super) const MMASK_RESET: u32 = 1 << 21;
/// ASTAT bits the flag set does not model: AS, AI, MN, MV, MU, MI, AF (4-10), SF (14), CACC (24-31).
const ASTAT_EXTRA: u32 = 0x1F << 6 | 1 << 14 | 0xFF00_0000;
/// STKY bits a program can write: AUS AVS AOS (0-2), AIS MOS MVS MUS MIS (5-9), CB7S CB15S IIRA
/// U64MA (17-20).
const STKY_WRITABLE: u32 = 0x3E7 | 0x1E_0000;
/// Sticky arithmetic bits and the interrupt they latch: fixed overflow, float overflow, float
/// underflow, float invalid (PRM Table 4-57, vectors 33-36).
const EXCEPTIONS: [(u32, u8); 4] = [
    (1 << 2 | 1 << 6, 33),
    (1 << 1 | 1 << 7, 34),
    (1 << 0 | 1 << 8, 35),
    (1 << 5 | 1 << 9, 36),
];

/// State of the system registers that has no other home.
pub(super) struct SysState {
    pub(super) stky: [u32; 2],
    pub(super) astat_extra: [u32; 2],
    pub(super) status: Vec<[u32; 3]>,
    pub(super) pc_overflow: bool,
    flags_reg: u32,
    /// Levels of the FLAG0-15 pins as driven from outside (see `set_flag_input`).
    flag_pins: u16,
    px: u64,
}

impl SysState {
    pub fn new() -> Self {
        SysState {
            stky: [0; 2],
            astat_extra: [0; 2],
            status: Vec::with_capacity(STATUS_STACK_DEPTH),
            pc_overflow: false,
            flags_reg: 0,
            flag_pins: 0,
            px: 0,
        }
    }
}

fn astat_pack(f: Flags, extra: u32) -> u32 {
    let b = |x: bool, n: u32| u32::from(x) << n;
    b(f.az, 0)
        | b(f.av, 1)
        | b(f.an, 2)
        | b(f.ac, 3)
        | b(f.as_, 4)
        | b(f.ai, 5)
        | b(f.sv, 11)
        | b(f.sz, 12)
        | b(f.ss, 13)
        | b(f.tf, 18)
        | extra & ASTAT_EXTRA
}

fn astat_unpack(v: u32, old: Flags) -> (Flags, u32) {
    let t = |n: u32| v >> n & 1 == 1;
    let f = Flags {
        az: t(0),
        av: t(1),
        an: t(2),
        ac: t(3),
        as_: t(4),
        ai: t(5),
        tf: t(18),
        sz: t(12),
        sv: t(11),
        ss: t(13),
        ..old
    };
    (f, v & ASTAT_EXTRA)
}

impl Core {
    pub(super) fn astat(&self, y: usize) -> u32 {
        let f = if y == 0 { self.flags } else { self.flags_y };
        astat_pack(f, self.sys.astat_extra[y])
    }

    pub(super) fn set_astat(&mut self, y: usize, v: u32) {
        let old = if y == 0 { self.flags } else { self.flags_y };
        let (f, extra) = astat_unpack(v, old);
        if y == 0 {
            self.flags = f;
        } else {
            self.flags_y = f;
        }
        self.sys.astat_extra[y] = extra;
    }

    /// Records arithmetic exception bits in STKYx (`y` = false) or STKYy and latches the matching
    /// `FIXI`/`FLTOI`/`FLTUI`/`FLTII` interrupt (every event latches again, PRM A-21).
    pub fn note_arith_exception(&mut self, y: bool, sticky: u32) {
        self.sys.stky[usize::from(y)] |= sticky & STKY_WRITABLE;
        for (mask, vector) in EXCEPTIONS {
            if sticky & mask != 0 {
                self.raise_interrupt(vector);
            }
        }
    }

    fn stky(&self, y: usize) -> u32 {
        let mut v = self.sys.stky[y] & (STKY_WRITABLE | SSOV | LSOV);
        if y == 1 {
            return v & 0x3E7;
        }
        let depth = self.pc_depth();
        v |= if depth == 0 { PCEM } else { 0 };
        v |= if depth >= PC_STACK_DEPTH { PCFL } else { 0 };
        v |= if self.sys.status.is_empty() { SSEM } else { 0 };
        v |= if self.loops.is_empty() { LSEM } else { 0 };
        v
    }

    /// Value of system register `code` (universal register groups 6 and 7).
    pub(super) fn read_sys(&self, code: u8) -> u32 {
        let n = (code & 15) as usize;
        if (code >> 4) & 7 == 6 {
            return match n {
                0 | 1 | 3 => self.pc, // FADDR/DADDR: ASSUMED equal to the pc (no pipeline model)
                4 => self.pcstk(),
                5 if self.sys.pc_overflow => 31,
                5 => self.pc_depth() as u32,
                6 => self.laddr(),
                7 | 8 => self.loops.last().map_or(0, |f| f.remaining),
                9 => self.cycles() as u32,
                10 => (self.cycles() >> 32) as u32,
                11 | 12 => self.sys.px as u32,
                13 => (self.sys.px >> 32) as u32,
                14 => self.read_tperiod(),
                15 => self.read_tcount(),
                _ => 0,
            };
        }
        match n {
            0 | 1 => self.ustat[n],
            2 => self.mode1,
            3 => self.mmask,
            4 => self.mode2,
            5 => self.flags_value(),
            6 => self.astat(0),
            7 => self.astat(1),
            8 | 9 => self.stky(n - 8),
            10 => self.irq.irptl,
            11 => self.imask,
            12 => self.imaskp_value(),
            13 => self.lirptl_value(),
            _ => self.ustat[n - 12],
        }
    }

    /// Writes system register `code`; read-only registers and unmodelled ones ignore the write.
    pub(super) fn write_sys(&mut self, code: u8, v: u32) {
        let n = (code & 15) as usize;
        if (code >> 4) & 7 == 6 {
            match n {
                4 => self.set_pcstk(v),
                5 if !self.sys.pc_overflow => self.call_stack.truncate(v as usize),
                6 => self.set_laddr(v),
                7 | 8 => self.set_curlcntr(v),
                11 => self.sys.px = self.sys.px & !0xFFFF_FFFF | u64::from(v),
                12 => self.sys.px = self.sys.px & !0xFFFF_FFFF | u64::from(v),
                13 => self.sys.px = self.sys.px & 0xFFFF_FFFF | u64::from(v) << 32,
                14 => self.write_tperiod(v),
                15 => self.write_tcount(v),
                _ => {}
            }
            return;
        }
        match n {
            0 | 1 => self.ustat[n] = v,
            2 => self.set_mode1(v),
            3 => self.mmask = v,
            4 => {
                let changed = self.mode2 ^ v;
                self.mode2 = v;
                if changed & TIMEN != 0 {
                    self.set_timer_enable(v & TIMEN != 0);
                }
            }
            5 => self.sys.flags_reg = v,
            6 | 7 => self.set_astat(n - 6, v),
            8 | 9 => {
                let keep = self.sys.stky[n - 8] & !STKY_WRITABLE;
                self.sys.stky[n - 8] = keep | v & STKY_WRITABLE;
            }
            10 => self.write_irptl(v),
            11 => {
                self.imask = v;
                self.poke_irq();
            }
            13 => self.write_lirptl(v),
            14 | 15 => self.ustat[n - 12] = v,
            _ => {} // IMASKP is controller state
        }
    }

    /// Drives the level of the FLAG`n` pin (0..16) from outside, as the other chips of the board
    /// do; it is what a program reads for a flag configured as an input.
    pub fn set_flag_input(&mut self, n: u8, high: bool) {
        let bit = 1u16 << (n & 15);
        self.sys.flag_pins = self.sys.flag_pins & !bit | if high { bit } else { 0 };
    }

    /// FLAGS as read: FLGx is bit `2x`, FLGxO bit `2x + 1` (PRM Table A-5); an input flag shows
    /// the pin level, an output flag the level the program wrote.
    pub(super) fn flags_value(&self) -> u32 {
        let written = self.sys.flags_reg;
        (0..16).fold(written, |acc, n| {
            let level = if written >> (2 * n + 1) & 1 == 1 {
                written >> (2 * n) & 1
            } else {
                u32::from(self.sys.flag_pins >> n & 1)
            };
            acc & !(1 << (2 * n)) | level << (2 * n)
        })
    }

    /// Whether interrupts are globally enabled and nesting is on (for tools).
    pub fn irq_modes(&self) -> (bool, bool) {
        (self.mode1 & IRPTEN != 0, self.mode1 & NESTM != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::stacks::PCSTK_EMPTY;
    use super::*;

    #[test]
    fn reset_state_matches_the_reference() {
        let c = Core::new(&[]);
        assert_eq!(
            c.read_ureg(0x78),
            PCEM | SSEM | LSEM,
            "STKYx reset (Figure A-4)"
        );
        assert_eq!(c.read_ureg(0x73), 1 << 21, "MMASK has PEYEN set");
        assert_eq!(c.read_ureg(0x64), PCSTK_EMPTY);
        assert_eq!(c.read_ureg(0x65), 0);
    }

    #[test]
    fn astat_round_trips_through_the_flag_set_and_extra_bits() {
        let mut c = Core::new(&[]);
        c.write_ureg(0x76, 0x00FF_FFFF);
        assert!(c.flags.az && c.flags.av && c.flags.an && c.flags.ac && c.flags.sv);
        assert!(c.flags.sz && c.flags.ss && c.flags.tf, "BTF is bit 18");
        c.write_ureg(0x76, 1 << 5 | 1 << 31);
        assert!(!c.flags.az);
        assert_eq!(c.read_ureg(0x76), 1 << 5 | 1 << 31, "AI and CACC kept");
        c.flags.an = true;
        assert_eq!(c.read_ureg(0x76) & 4, 4);
        // the firmware's `BIT TST ASTATx 0x20` (AI) sees bit 5
        c.write_ureg(0x76, 0);
        assert_eq!(c.read_ureg(0x76) & 0x20, 0);
    }

    #[test]
    fn arithmetic_exceptions_set_sticky_bits_and_latch_their_interrupts() {
        let mut c = Core::new(&[]);
        c.note_arith_exception(false, 1 << 5); // AIS
        assert_eq!(c.read_ureg(0x78) & (1 << 5), 1 << 5);
        assert_eq!(c.read_ureg(0x7A) >> 26 & 1, 1, "FLTII is IRPTL bit 26");
        c.note_arith_exception(true, 1 << 2); // AOS in STKYy
        assert_eq!(c.read_ureg(0x79) & 4, 4);
        assert_eq!(c.read_ureg(0x7A) >> 23 & 1, 1, "FIXI is IRPTL bit 23");
        c.write_ureg(0x78, 0);
        assert_eq!(
            c.read_ureg(0x78) & (1 << 5),
            0,
            "a program clears sticky bits"
        );
    }

    #[test]
    fn emuclk_px_and_misc_registers() {
        let mut c = Core::new(&[]);
        assert_eq!(c.run(7), None);
        assert_eq!(c.read_ureg(0x69), 7, "EMUCLK counts cycles");
        c.write_ureg(0x6C, 0x1111);
        c.write_ureg(0x6D, 0x2222);
        assert_eq!((c.read_ureg(0x6C), c.read_ureg(0x6D)), (0x1111, 0x2222));
        c.write_ureg(0x75, 0x8003); // FLG0 output high; FLG7 level bit (input) is ignored
        assert_eq!(c.read_ureg(0x75), 0x8003 & !(1 << 14));
        c.write_ureg(0x7C, 0xFF);
        assert_eq!(c.read_ureg(0x7C), 0, "IMASKP is controller state");
        assert_eq!(c.irq_modes(), (false, false));
    }

    #[test]
    fn flag_pins_feed_input_flags_and_outputs_read_back() {
        // the FX Core1 handshake: BIT SET FLAGS 0x1 (FLG0 high), then wait for FLG1 (bit 2) high
        let mut c = Core::new(&[]);
        c.write_ureg(0x75, 0x3); // FLG0 as output, high
        assert_eq!(c.read_ureg(0x75) & 0x7, 0x3);
        assert_eq!(
            c.read_ureg(0x75) & 0x4,
            0,
            "FLG1 is an input and its pin is low"
        );
        c.set_flag_input(1, true);
        assert_eq!(c.read_ureg(0x75) & 0x4, 0x4);
        c.set_flag_input(0, true); // driving an output pin does not change what was written
        c.write_ureg(0x75, 0x2);
        assert_eq!(c.read_ureg(0x75) & 0x3, 0x2);
    }

    #[test]
    fn lirptl_keeps_latch_and_mask_bits_and_derives_the_mask_pointers() {
        let mut c = Core::new(&[]);
        c.write_ureg(0x7D, 1 << 10 | 1 << 3); // P6I mask, P9I latch
        assert_eq!(c.read_ureg(0x7D), 1 << 10 | 1 << 3);
        c.mode1 |= IRPTEN;
        assert!(c.raise_interrupt(17)); // P6I
        c.mode1 |= 0;
        assert_eq!(c.pending_vector(), Some(17));
    }
}
