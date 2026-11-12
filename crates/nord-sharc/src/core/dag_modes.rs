//! DAG side effects outside plain address arithmetic: broadcast register loads (`MODE1`
//! `BDCST1`/`BDCST9`) and the circular buffer overflow interrupts `CB7I`/`CB15I` with their
//! sticky bits `CB7S`/`CB15S`.
//!
//! Broadcast (PRM 6-24, A-6): a data register load indexed with `I1` (`BDCST1`, MODE1 bit 23)
//! or `I9` (`BDCST9`, bit 22) writes the named register and its complement in the other
//! processing element with the same word, whatever `PEYEN` says. Loads into universal registers
//! that are not data registers and all stores are unaffected. The firmware sets both bits in
//! every section (`bit set mode1 0xe00000`).
//!
//! Overflow (PRM 6-30, 6-31, A-21): when a circular buffer operation on `I7` or `I15` wraps the
//! index past the end (or below the start) of its buffer, `STKYx.CB7S`/`CB15S` (bits 17/18) is
//! set and `CB7I`/`CB15I` (vectors 30/31) is latched. The wrap is noted while the address is
//! computed (`Core::cb_wrap`) and latched when the instruction ends.
//!
//! Provenance: MODE1 bit numbers and semantics are PRM text (Table A-1); not measured on
//! hardware (ASSUMED for the exact trigger on a wrap with `L = 0`, the PRM says the interrupt
//! then occurs at every change of `I7` after it crossed `B7`: not modelled).
use super::Core;

/// MODE1 bit 22 (`BDCST9`) and bit 23 (`BDCST1`).
const MODE1_BDCST9: u32 = 1 << 22;
const MODE1_BDCST1: u32 = 1 << 23;
/// STKYx bits `CB7S` and `CB15S`.
const CB7S: u32 = 1 << 17;
const CB15S: u32 = 1 << 18;
/// Interrupt vector numbers of `CB7I` and `CB15I` (Table B-2).
const CB7I: u8 = 30;
const CB15I: u8 = 31;

impl Core {
    /// Whether loads through index register `ii` are broadcast.
    #[inline]
    fn broadcasting(&self, ii: usize) -> bool {
        (ii == 1 && self.mode1 & MODE1_BDCST1 != 0) || (ii == 9 && self.mode1 & MODE1_BDCST9 != 0)
    }

    /// After a load of universal register `ureg` through index `ii`: with broadcast enabled for
    /// that index, copies the loaded data register to its complement.
    #[inline]
    pub(super) fn broadcast_load(&mut self, ii: usize, ureg: u8, write: bool) {
        if write || !self.broadcasting(ii) || !matches!(ureg >> 4, 0 | 5) {
            return;
        }
        let v = self.read_ureg(ureg);
        let n = ureg & 15;
        self.write_ureg(n, v);
        self.write_ureg(0x50 | n, v);
    }

    /// Records that a circular buffer operation on `ii` wrapped (`plain` is the unwrapped sum).
    #[inline]
    pub(super) fn note_cb_wrap(&self, ii: usize, plain: u32, wrapped: u32) {
        if wrapped != plain && (ii == 7 || ii == 15) {
            self.cb_wrap
                .set(self.cb_wrap.get() | if ii == 7 { 1 } else { 2 });
        }
    }

    /// Latches the overflow interrupts and sticky bits of the wraps seen by the last instruction.
    pub(super) fn latch_cb_overflow(&mut self) {
        let wraps = self.cb_wrap.replace(0);
        for (bit, sticky, vector) in [(1, CB7S, CB7I), (2, CB15S, CB15I)] {
            if wraps & bit != 0 {
                self.sys.stky[0] |= sticky;
                self.raise_interrupt(vector);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_at_zero;
    use super::*;

    /// `R0 = DM(Ii, M1)` / `PM(Ii, M9)`, Type 3a (`507e 8000 0000` is `DM(I0, M1) = R0` in the
    /// firmware, ADI): post-modify bit 44, index 43..41, modify 40..38, G bit 32, write bit 31.
    fn load_r0_through(ii: u16) -> [u16; 3] {
        [0x507E | (ii & 7) << 9 | ii >> 3, 0, 0]
    }

    #[test]
    fn bdcst1_load_through_i1_fills_both_register_files_even_in_sisd() {
        let mut c = core_at_zero(&load_r0_through(1));
        c.mode1 = MODE1_BDCST1;
        (c.i[1], c.m[1]) = (0x9_0100, 1);
        c.dm_write(0x9_0100, 0xCAFE).unwrap();
        c.dm_write(0x9_0101, 0xBEEF).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[0], c.s[0]), (0xCAFE, 0xCAFE), "PEy gets the same word");
        // without the bit only R0 is loaded in SISD mode
        let mut c = core_at_zero(&load_r0_through(1));
        (c.i[1], c.m[1]) = (0x9_0100, 1);
        c.dm_write(0x9_0100, 0xCAFE).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[0], c.s[0]), (0xCAFE, 0));
    }

    #[test]
    fn bdcst1_applies_to_i1_only_and_bdcst9_to_i9() {
        let mut c = core_at_zero(&load_r0_through(2));
        c.mode1 = MODE1_BDCST1 | MODE1_BDCST9;
        (c.i[2], c.m[1]) = (0x9_0100, 1);
        c.dm_write(0x9_0100, 5).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[0], c.s[0]), (5, 0), "I2 is not a broadcast index");
        let mut c = core_at_zero(&[0]);
        c.mode1 = MODE1_BDCST9;
        c.r[4] = 8;
        c.broadcast_load(9, 4, false);
        assert_eq!(c.s[4], 8);
        c.broadcast_load(9, 5, true);
        assert_eq!(c.s[5], 0, "stores are not broadcast");
        c.s[6] = 3;
        c.broadcast_load(9, 0x56, false);
        assert_eq!(c.r[6], 3, "an S register load goes to R as well");
        c.broadcast_load(1, 4, false);
        assert_eq!(c.s[4], 8);
    }

    #[test]
    fn circular_wrap_of_i7_sets_cb7s_and_latches_cb7i() {
        let mut c = core_at_zero(&[0x0001]);
        c.mode1 = 1 << 24; // CBUFEN
        (c.i[7], c.l[7], c.b[7]) = (0x103, 4, 0x100);
        assert_eq!(c.modified_by(7, 1), 0x100, "wraps to the base");
        c.latch_cb_overflow();
        assert_eq!(c.read_ureg(0x78) & CB7S, CB7S);
        assert_eq!(c.irq.irptl >> 20 & 1, 1, "CB7I is IRPTL bit 20");
        assert_eq!(c.cb_wrap.get(), 0, "the wrap record is consumed");
    }

    #[test]
    fn no_wrap_no_event_and_i15_has_its_own_bits() {
        let mut c = core_at_zero(&[0x0001]);
        c.mode1 = 1 << 24;
        (c.i[15], c.l[15], c.b[15]) = (0x200, 4, 0x200);
        assert_eq!(c.modified_by(15, 1), 0x201);
        c.latch_cb_overflow();
        assert_eq!(c.read_ureg(0x78) & (CB7S | CB15S), 0, "inside the buffer");
        assert_eq!(
            c.modified_by(15, 0xFFFF_FFFF),
            0x203,
            "below the base wraps"
        );
        c.latch_cb_overflow();
        assert_eq!(c.read_ureg(0x78) & (CB7S | CB15S), CB15S);
        assert_eq!(c.irq.irptl >> 21 & 1, 1, "CB15I is IRPTL bit 21");
        // other index registers do not report
        (c.i[6], c.l[6], c.b[6]) = (0x103, 4, 0x100);
        assert_eq!(c.modified_by(6, 1), 0x100);
        c.sys.stky[0] = 0;
        c.latch_cb_overflow();
        assert_eq!(c.read_ureg(0x78) & (CB7S | CB15S), 0);
    }

    #[test]
    fn linear_addressing_never_overflows() {
        let mut c = core_at_zero(&[0x0001]);
        (c.i[7], c.l[7], c.b[7]) = (0x103, 4, 0x100); // CBUFEN clear
        assert_eq!(c.modified_by(7, 1), 0x104);
        c.latch_cb_overflow();
        assert_eq!(c.read_ureg(0x78) & CB7S, 0);
    }
}
