//! Shifter bit FIFO of [`Core`] (ADSP-214xx only): `BITDEP`, `BITEXT` and `BFFWRP`, the 64-bit
//! register with its write pointer, and the `SF` status flag (programming reference rev 2.4,
//! section 3 "Bit Stream Manipulation Instructions", section 11 pages 11-86 to 11-91).
//!
//! The FIFO is MSB aligned: bit 63 is the oldest bit, the write pointer `wrp` counts the valid
//! bits (0 = empty, 64 = full). `BITDEP` ORs a right justified field in below the valid bits,
//! `BITEXT` takes the top bits and shifts the register left. Each processing element has its own
//! FIFO (it lives in [`super::Flags`], which the PEx/PEy swap already exchanges).
//!
//! Encodings, VERIFIED against ADI's `elfdump` (`docs/research/sharc-bitfifo.md`): the register
//! forms are shifter opcodes `0x50` (`Rn = BITEXT Ry`), `0x58` (with `(NU)`), `0x70`
//! (`Rn = BFFWRP`), `0x74` (`BITDEP Rx BY Ry`) and `0x7C` (`BFFWRP = Ry`); the shift-immediate
//! forms (opcode `0x02`, `SHIFTOP` 20, 22, 29, 31) pass `DATA12` where the register forms read
//! `Ry`. The length is `Ry[5:0]` (`DATA12[5:0]`) and the `BFFWRP` value `Ry[6:0]`.
//!
//! Cases the manual leaves undefined (a length above 32, more bits than the FIFO holds or
//! has room for) set `SV` and are modelled as ASSUMED: see each method.
use super::Core;

/// Shifter opcode of `Rn = BITEXT Ry` (VERIFIED, `docs/research/sharc-bitfifo.md`).
pub(super) const BITEXT: u64 = 0x50;
/// Shifter opcode of `Rn = BITEXT Ry (NU)`.
pub(super) const BITEXT_NU: u64 = 0x58;
/// Shifter opcode of `Rn = BFFWRP`.
pub(super) const BFFWRP_READ: u64 = 0x70;
/// Shifter opcode of `BITDEP Rx BY Ry`.
pub(super) const BITDEP: u64 = 0x74;
/// Shifter opcode of `BFFWRP = Ry`.
pub(super) const BFFWRP_WRITE: u64 = 0x7C;

/// Largest bit count the write pointer may hold (a full FIFO).
const FULL: u32 = 64;
/// Largest field one `BITDEP` or `BITEXT` handles.
const MAX_LEN: u32 = 32;

/// The 64-bit bit FIFO register and its write pointer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BitFifo {
    /// FIFO contents, oldest bit at bit 63. ASSUMED: all zero at reset (not stated by the manual).
    pub bff: u64,
    /// Number of valid bits (`BFFWRP`), 0..=64.
    pub wrp: u32,
}

impl BitFifo {
    /// The `SF` flag: set while the write pointer is 32 or more (A-6, rev 2.4).
    #[inline]
    pub fn sf(&self) -> bool {
        self.wrp >= MAX_LEN
    }

    /// `BITDEP`: appends the low `len` bits of `x`; returns `SV`. VERIFIED semantics: the manual
    /// equation `BFF |= FDEP x BY (64 - (wrp + len)) : len`, `wrp += len`. ASSUMED for the
    /// undefined cases: `len > 32` or a FIFO without room for `len` more bits sets `SV` and
    /// leaves the FIFO unchanged.
    pub fn deposit(&mut self, x: u32, len: u32) -> bool {
        if len > MAX_LEN || self.wrp + len > FULL {
            return true;
        }
        if len > 0 {
            let field = u64::from(x) & ((1u64 << len) - 1);
            self.bff |= field << (FULL - self.wrp - len);
            self.wrp += len;
        }
        false
    }

    /// `BITEXT`: returns the top `len` bits right justified and `SV`. With `update` the register
    /// shifts left by `len` and the pointer drops by `len`; without it (`NU`) nothing changes.
    /// ASSUMED for the undefined cases: `len > 32` gives 0 with `SV` and no change; asking for
    /// more bits than are valid gives the top bits as they are (zero below the pointer when the
    /// FIFO was filled by `BITDEP`), sets `SV`, and leaves the pointer at 0.
    pub fn extract(&mut self, len: u32, update: bool) -> (u32, bool) {
        if len > MAX_LEN {
            return (0, true);
        }
        let value = if len == 0 {
            0
        } else {
            ((self.bff >> 32) as u32) >> (MAX_LEN - len)
        };
        let underflow = len > self.wrp;
        if update {
            self.bff <<= len;
            self.wrp = self.wrp.saturating_sub(len);
        }
        (value, underflow)
    }

    /// `BFFWRP = value` (7 bits used): returns `SV`, set when the value exceeds 64. ASSUMED: an
    /// over-range value is stored as 64; the register contents are left alone (the manual's
    /// restore sequence, listing 3-3, sets the pointer and then deposits).
    pub fn set_wrp(&mut self, value: u32) -> bool {
        let value = value & 0x7F;
        self.wrp = value.min(FULL);
        value > FULL
    }
}

impl Core {
    /// Executes a bit FIFO shifter operation (`Rn = BITEXT`, `BITDEP`, `BFFWRP`); `rx`/`ry` are the
    /// operand values (the immediate forms pass `DATA12` as `ry`). Returns false when `op` is not
    /// a bit FIFO opcode. Flags per the manual: `SZ` only for `BITEXT`, `SV` as described on the
    /// methods of [`BitFifo`], `SS` always cleared; `SF` follows the pointer.
    pub(super) fn bitfifo_op(&mut self, op: u64, n: usize, rx: u32, ry: u32) -> bool {
        let len = ry & 63;
        let (result, sz, sv) = match op {
            BITEXT | BITEXT_NU => {
                let (v, sv) = self.flags.fifo.extract(len, op == BITEXT);
                (Some(v), v == 0, sv)
            }
            BITDEP => (None, false, self.flags.fifo.deposit(rx, len)),
            BFFWRP_READ => (Some(self.flags.fifo.wrp), false, false),
            BFFWRP_WRITE => (None, false, self.flags.fifo.set_wrp(ry)),
            _ => return false,
        };
        if let Some(v) = result {
            self.r[n] = v;
        }
        self.set_shifter_flags(sz, sv, false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::{core_at_zero, core_with};
    use super::*;

    /// Compute word of a shifter operation `Rn = op Rx BY Ry` (compute unit 2).
    fn word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (2 << 20) | (op << 12) | (n << 8) | (x << 4) | y
    }

    fn exec(c: &mut Core, op: u64, n: u64, x: u64, y: u64) {
        assert!(c.compute(word(op, n, x, y)), "op {op:#x}");
    }

    #[test]
    fn deposit_appends_below_the_valid_bits_like_the_manual_figure() {
        // manual figure 11-86: three bits `lmn` in the FIFO, then a 12-bit field `abcdefghijkl`
        let mut f = BitFifo {
            bff: 0b101 << 61,
            wrp: 3,
        };
        assert!(!f.deposit(0xFFFF_F000 | 0b1100_0011_0101, 12));
        assert_eq!(f.wrp, 15);
        assert_eq!(f.bff >> 49, (0b101 << 12) | 0b1100_0011_0101);
        assert_eq!(f.bff << 15, 0, "nothing below the valid bits");
        assert!(!f.sf());
    }

    #[test]
    fn extract_takes_the_top_bits_and_shifts_left() {
        // manual figure 11-90: 12 bits `abcdefghijkl` out of `abcdefghijklmn`
        let mut f = BitFifo {
            bff: 0b10_1011_0011_1101 << 50,
            wrp: 14,
        };
        let (v, sv) = f.extract(12, true);
        assert_eq!((v, sv), (0b1010_1100_1111, false));
        assert_eq!((f.bff, f.wrp), (0b01 << 62, 2));
        let mut f = BitFifo {
            bff: 0b11 << 62,
            wrp: 2,
        };
        assert_eq!(f.extract(0, true), (0, false));
        assert_eq!(f.extract(2, true), (0b11, false));
        assert_eq!((f.bff, f.wrp), (0, 0));
    }

    #[test]
    fn nu_extract_changes_nothing_but_the_result_registers() {
        let mut c = Core::new(&[]);
        c.flags.fifo = BitFifo {
            bff: 0xABCD_0000 << 32,
            wrp: 16,
        };
        c.r[2] = 8;
        exec(&mut c, BITEXT_NU, 3, 0, 2);
        assert_eq!(c.r[3], 0xAB);
        assert_eq!(
            (c.flags.fifo.bff, c.flags.fifo.wrp),
            (0xABCD_0000 << 32, 16)
        );
        exec(&mut c, BITEXT, 3, 0, 2);
        assert_eq!((c.r[3], c.flags.fifo.wrp), (0xAB, 8));
        assert_eq!(c.flags.fifo.bff, 0xCD00_0000 << 32);
    }

    #[test]
    fn header_extraction_loop_of_listing_3_1() {
        // refill with 32 bits whenever SF is clear, take 6 then 3 bits
        let mut c = Core::new(&[]);
        exec(&mut c, BFFWRP_WRITE, 0, 0, 0); // BFFWRP = R0 = 0
        (c.r[10], c.r[2]) = (0xDEAD_BEEF, 32);
        exec(&mut c, BITDEP, 0, 10, 2);
        assert!(c.flags.fifo.sf(), "32 bits make SF true");
        (c.r[2], c.r[4]) = (6, 3);
        exec(&mut c, BITEXT, 6, 0, 2);
        assert_eq!(c.r[6], 0xDEAD_BEEF >> 26);
        assert!(!c.flags.fifo.sf(), "26 bits left");
        exec(&mut c, BITEXT, 6, 0, 4);
        assert_eq!(c.r[6], (0xDEAD_BEEF >> 23) & 7);
        assert_eq!(c.flags.fifo.wrp, 23);
    }

    #[test]
    fn deposit_and_extract_round_trip_arbitrary_widths() {
        let mut c = Core::new(&[]);
        let widths = [5u32, 12, 1, 32, 7, 7];
        let values = [0x15u32, 0xABC, 1, 0x8000_0001, 0x55, 0x2A];
        for (w, v) in widths.iter().zip(values) {
            (c.r[1], c.r[2]) = (v, *w);
            exec(&mut c, BITDEP, 0, 1, 2);
        }
        assert_eq!(c.flags.fifo.wrp, 64);
        for (w, v) in widths.iter().zip(values) {
            c.r[2] = *w;
            exec(&mut c, BITEXT, 3, 0, 2);
            let mask = if *w == 32 { u32::MAX } else { (1 << w) - 1 };
            assert_eq!(c.r[3], v & mask, "width {w}");
        }
        assert_eq!((c.flags.fifo.bff, c.flags.fifo.wrp), (0, 0));
    }

    #[test]
    fn save_and_restore_sequence_of_listing_3_3() {
        let mut c = Core::new(&[]);
        (c.r[1], c.r[2]) = (0xCAFE_F00D, 32);
        exec(&mut c, BITDEP, 0, 1, 2);
        (c.r[1], c.r[2]) = (0x1234_5678, 20);
        exec(&mut c, BITDEP, 0, 1, 2);
        let saved = c.flags.fifo;
        assert_eq!(saved.wrp, 52);
        // R5 = BFFWRP; BFFWRP = 64; R6 = BITEXT 32; R7 = BITEXT 32
        exec(&mut c, BFFWRP_READ, 5, 0, 0);
        assert_eq!(c.r[5], 52);
        (c.r[0], c.r[3]) = (64, 32);
        exec(&mut c, BFFWRP_WRITE, 0, 0, 0);
        exec(&mut c, BITEXT, 6, 0, 3);
        exec(&mut c, BITEXT, 7, 0, 3);
        assert_eq!((c.r[6], c.r[7]), (0xCAFE_F00D, 0x4567_8000));
        assert_eq!(c.flags.fifo, BitFifo::default());
        // restore: BFFWRP = 0; BITDEP R6 BY 32; BITDEP R7 BY 32; BFFWRP = R5
        c.r[0] = 0;
        exec(&mut c, BFFWRP_WRITE, 0, 0, 0);
        exec(&mut c, BITDEP, 0, 6, 3);
        exec(&mut c, BITDEP, 0, 7, 3);
        exec(&mut c, BFFWRP_WRITE, 0, 0, 5);
        assert_eq!(c.flags.fifo, saved);
    }

    #[test]
    fn pointer_register_flags() {
        let mut c = Core::new(&[]);
        c.flags.sz = true;
        c.flags.ss = true;
        c.r[2] = 40;
        exec(&mut c, BFFWRP_WRITE, 0, 0, 2);
        assert!(c.flags.fifo.sf() && !c.flags.sv && !c.flags.sz && !c.flags.ss);
        exec(&mut c, BFFWRP_READ, 4, 0, 0);
        assert_eq!(c.r[4], 40);
        c.r[2] = 65;
        exec(&mut c, BFFWRP_WRITE, 0, 0, 2);
        assert!(c.flags.sv, "above 64 sets SV");
        assert_eq!(c.flags.fifo.wrp, 64);
        c.r[2] = 0x180 | 32; // only the low 7 bits are written
        exec(&mut c, BFFWRP_WRITE, 0, 0, 2);
        assert_eq!((c.flags.fifo.wrp, c.flags.sv), (32, false));
        exec(&mut c, BFFWRP_READ, 4, 0, 0);
        assert!(!c.flags.sv, "a pointer read clears SV");
    }

    #[test]
    fn extract_flags_zero_and_underflow() {
        let mut c = Core::new(&[]);
        c.r[2] = 4;
        exec(&mut c, BITEXT, 3, 0, 2); // empty FIFO: 4 bits asked, none valid
        assert_eq!(c.r[3], 0);
        assert!(c.flags.sz && c.flags.sv && !c.flags.ss);
        assert_eq!(c.flags.fifo.wrp, 0);
        (c.r[1], c.r[2]) = (0b1000, 4);
        exec(&mut c, BITDEP, 0, 1, 2);
        assert!(!c.flags.sz && !c.flags.sv, "BITDEP clears SZ and SS");
        exec(&mut c, BITEXT, 3, 0, 2);
        assert_eq!(c.r[3], 0b1000);
        assert!(!c.flags.sz && !c.flags.sv);
        c.r[2] = 33;
        exec(&mut c, BITEXT, 3, 0, 2);
        assert!(c.flags.sv && c.flags.sz, "length above 32 is prohibited");
    }

    #[test]
    fn deposit_flags_overflow_and_oversized_length() {
        let mut c = Core::new(&[]);
        (c.r[1], c.r[2]) = (u32::MAX, 33);
        exec(&mut c, BITDEP, 0, 1, 2);
        assert!(c.flags.sv);
        assert_eq!(c.flags.fifo, BitFifo::default(), "ASSUMED: no change");
        let mut f = BitFifo { bff: 0, wrp: 60 };
        assert!(f.deposit(0xFF, 8), "no room for 8 more bits");
        assert_eq!(f.wrp, 60);
        assert!(!f.deposit(0xF, 4));
        assert_eq!((f.wrp, f.bff), (64, 0xF));
        assert!(f.sf());
    }

    #[test]
    fn unrelated_opcodes_are_not_taken() {
        let mut c = Core::new(&[]);
        assert!(!c.bitfifo_op(0x00, 0, 0, 0));
        assert!(!c.bitfifo_op(0x78, 0, 0, 0));
    }

    /// ADI `elfdump` ground truth for the register forms: `013e 0025 0312` is `r3=bitext r2`,
    /// `013e 0027 4312` is `bitdep r1 by r2`, `013e 0027 c312` is `bffwrp=r2`, `013e 0027 0312`
    /// is `r3=bffwrp` (Rx = bits 7..4, Ry = bits 3..0, Rn = bits 11..8).
    #[test]
    fn register_words_match_the_adi_disassembly() {
        let mut c = core_at_zero(&[0x013e, 0x0027, 0xc312, 0x013e, 0x0027, 0x4312]);
        c.r[2] = 12;
        assert_eq!(c.step(), Ok(())); // bffwrp=r2
        assert_eq!(c.flags.fifo.wrp, 12);
        c.r[1] = 0xFFFF_FFFF;
        c.r[2] = 4;
        assert_eq!(c.step(), Ok(())); // bitdep r1 by r2 (uses Ry = 4 now, not the pointer)
        assert_eq!(c.flags.fifo.wrp, 16);
        assert_eq!(c.flags.fifo.bff, 0xF << 48);
        let mut c = core_at_zero(&[0x013e, 0x0025, 0x0312, 0x013e, 0x0025, 0x8312]);
        c.flags.fifo = BitFifo {
            bff: 0xA500_0000 << 32,
            wrp: 8,
        };
        c.r[2] = 4;
        assert_eq!(c.step(), Ok(())); // r3=bitext r2
        assert_eq!((c.r[3], c.flags.fifo.wrp), (0xA, 4));
        assert_eq!(c.step(), Ok(())); // r3=bitext r2 (nu)
        assert_eq!((c.r[3], c.flags.fifo.wrp), (0x5, 4));
        let mut c = core_at_zero(&[0x013e, 0x0027, 0x0312]);
        c.flags.fifo.wrp = 9;
        assert_eq!(c.step(), Ok(())); // r3=bffwrp
        assert_eq!(c.r[3], 9);
    }

    /// ADI `elfdump` ground truth for the shift-immediate forms: `023e 0014 0535` is
    /// `r3=bitext 0x5`, `023e 0016 0535` the `(nu)` form, `023e 001d 0535` is `bitdep r5 by 0x5`
    /// and `023e 001f 7f35` is `bffwrp=0x7f` (the length keeps `DATA12[5:0]`, the pointer
    /// `DATA12[6:0]`, `DATA12` high bits ride in bits 30..27).
    #[test]
    fn immediate_words_match_the_adi_disassembly() {
        let mut c = core_at_zero(&[0x023e, 0x001f, 0x2035, 0x023e, 0x001d, 0x0535]);
        assert_eq!(c.step(), Ok(())); // bffwrp=0x20
        assert_eq!(c.flags.fifo.wrp, 32);
        c.r[5] = 0b10110;
        assert_eq!(c.step(), Ok(())); // bitdep r5 by 0x5
        assert_eq!(c.flags.fifo.wrp, 37);
        assert_eq!(c.flags.fifo.bff, 0b10110 << (64 - 37));
        let mut c = core_at_zero(&[0x023e, 0x0014, 0x0535, 0x023e, 0x0016, 0x0535]);
        c.flags.fifo = BitFifo {
            bff: 0b10110 << 59,
            wrp: 5,
        };
        assert_eq!(c.step(), Ok(())); // r3=bitext 0x5
        assert_eq!((c.r[3], c.flags.fifo.wrp), (0b10110, 0));
        c.flags.fifo.wrp = 5;
        assert_eq!(c.step(), Ok(())); // r3=bitext 0x5 (nu) of the now empty register
        assert_eq!((c.r[3], c.flags.sz), (0, true));
        // DATA12 = 0x80 keeps only DATA12[5:0] = 0 for the length, [6:0] = 0 for the pointer
        let mut c = core_at_zero(&[0x023e, 0x001f, 0x8035]);
        c.flags.fifo.wrp = 7;
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.flags.fifo.wrp, 0);
    }

    #[test]
    fn sf_and_not_sf_conditions_use_code_14_and_30() {
        // `if bm r3=lshift r1 by r2` (011c) is ADSP-214xx `IF SF`; `013c` is `IF NOT SF`
        for (parcel, wrp, runs) in [
            (0x011cu16, 32, true),
            (0x011c, 31, false),
            (0x013c, 31, true),
            (0x013c, 32, false),
        ] {
            let mut c = core_with(&[parcel, 0x0020, 0x0312]);
            (c.r[1], c.r[2]) = (1, 4);
            c.flags.fifo.wrp = wrp;
            assert_eq!(c.step(), Ok(()));
            assert_eq!(c.r[3], if runs { 16 } else { 0 }, "{parcel:#x} wrp {wrp}");
        }
    }

    #[test]
    fn each_processing_element_has_its_own_fifo() {
        let mut c = core_at_zero(&[0x013e, 0x0027, 0x4312]); // bitdep r1 by r2
        c.mode1 = 1 << 21;
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (0xF, 4, 0x3, 2);
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.flags.fifo.wrp, c.flags_y.fifo.wrp), (4, 2));
        assert_eq!(c.flags_y.fifo.bff, 0b11 << 62);
    }
}
