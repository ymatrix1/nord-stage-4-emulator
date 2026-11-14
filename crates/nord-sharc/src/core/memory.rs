//! Data memory (SRAM, IOP window, device store) and the universal register file of [`Core`].
use super::{Core, IOP_WORDS, SRAM_BYTES};

/// Normal-word addresses backed by the flat SRAM image (blocks 1 to 3 of the ADSP-21489).
const SRAM_WORDS: std::ops::Range<u32> = 0x9_0000..0x10_0000;

/// Short-word (16-bit) data addresses backed by the flat SRAM image: a data access in this range
/// moves one 16-bit word (zero-extended on reads, low half on writes) at byte `2 * addr`.
/// VERIFIED: the SY1SY2/SMP boot kernel reads the received word `DM(0x92003)` as the shorts
/// `DM(0x124006)` and `DM(0x124007)` (`tools/adi_dis.py --isa`, ISA words `0x8C0ED`, `0x8C0F0`).
const SHORT_WORDS: std::ops::Range<u32> = 0x10_0000..0x20_0000;

/// Byte offset of normal-word address `addr` in the flat SRAM image, if it is backed by SRAM.
#[inline]
fn sram_offset(addr: u32) -> Option<usize> {
    let o = (addr as usize).wrapping_mul(4);
    (SRAM_WORDS.contains(&addr) && o + 4 <= SRAM_BYTES).then_some(o)
}

impl Core {
    /// Reads a 32-bit data word at a normal-word address.
    ///
    /// Internal SRAM and the IOP window are modelled; every other address reads from a sparse
    /// device store (ASSUMED: peripherals and the external bus behave like plain memory until
    /// they are modelled, `device_reads` counts these accesses).
    pub fn dm_read(&mut self, addr: u32) -> Result<u32, u32> {
        if self.rwatch_on
            && addr.wrapping_sub(self.rwatch) < self.rwatch_len
            && self.rwatch_hits.len() < self.watch_limit
        {
            self.rwatch_hits.push((self.pc, addr));
        }
        if self.periph.is_some() {
            if let Some(v) = self.periph_read(addr) {
                return Ok(v);
            }
        }
        if (addr as usize) < IOP_WORDS {
            return Ok(self.iop[addr as usize]);
        }
        if let Some(o) = sram_offset(addr) {
            let w = [
                self.sram[o],
                self.sram[o + 1],
                self.sram[o + 2],
                self.sram[o + 3],
            ];
            return Ok(u32::from_le_bytes(w));
        }
        if SHORT_WORDS.contains(&addr) && !self.preset.contains_key(&addr) {
            let o = addr as usize * 2;
            return Ok(u32::from(u16::from_le_bytes([
                self.sram[o],
                self.sram[o + 1],
            ])));
        }
        self.device_reads += 1;
        if let Some(v) = self.preset.get(&addr) {
            return Ok(*v);
        }
        Ok(self.device.get(&addr).copied().unwrap_or(0))
    }

    /// Writes a 32-bit data word at a normal-word address (see [`Core::dm_read`]).
    pub fn dm_write(&mut self, addr: u32, value: u32) -> Result<(), u32> {
        if addr.wrapping_sub(self.watch) < self.watch_len
            && self.watch_hits.len() < self.watch_limit
        {
            self.watch_hits.push((self.steps, self.pc, value));
            self.watch_addrs.push(addr);
        }
        if self.periph.is_some() && self.periph_write(addr, value) {
            return Ok(());
        }
        if (addr as usize) < IOP_WORDS {
            self.iop[addr as usize] = value;
            return Ok(());
        }
        if let Some(o) = sram_offset(addr) {
            self.sram[o..o + 4].copy_from_slice(&value.to_le_bytes());
            return Ok(());
        }
        if SHORT_WORDS.contains(&addr) {
            let o = addr as usize * 2;
            self.sram[o..o + 2].copy_from_slice(&(value as u16).to_le_bytes());
            return Ok(());
        }
        self.device_writes += 1;
        self.device.insert(addr, value);
        Ok(())
    }

    /// Scripts a device read: later reads of `addr` return `value` whatever was written there.
    pub fn preset_device(&mut self, addr: u32, value: u32) {
        self.preset.insert(addr, value);
    }

    /// Value of a 7-bit universal register code (Table 10-2, PEx view); groups 6 and 7 are the
    /// system registers of `sysregs.rs`.
    pub fn read_ureg(&self, code: u8) -> u32 {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => self.r[n],
            1 => self.i[n],
            2 => self.m[n],
            3 => self.l[n],
            4 => self.b[n],
            5 => self.s[n],
            7 if n < 2 => self.ustat[n], // USTAT1/2: hot in the firmware's polling loops
            6 | 7 => self.read_sys(code),
            _ => 0,
        }
    }

    /// Writes a universal register (system registers not listed are accepted and ignored).
    pub fn write_ureg(&mut self, code: u8, v: u32) {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => {
                self.r[n] = v;
                self.fx.clear(n);
            }
            1 => self.i[n] = v,
            2 => self.m[n] = v,
            3 => self.l[n] = v,
            // VERIFIED (census diff on the ADI simulator): loading Bx also loads Ix with the same
            // value; the SMP voice loop `0x1C07D8` relies on it (`B1 = DM(..)` then `R12 = DM(I1, M4)`).
            4 => {
                self.b[n] = v;
                self.i[n] = v;
            }
            5 => {
                self.s[n] = v;
                self.fx_y.clear(n);
            }
            7 if n < 2 => self.ustat[n] = v,
            6 | 7 => self.write_sys(code, v),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sram_iop_and_device_windows() {
        let mut c = Core::new(&[]);
        c.dm_write(0x9_0010, 42).unwrap();
        assert_eq!(c.dm_read(0x9_0010), Ok(42));
        c.dm_write(0x3_E018, 7).unwrap();
        assert_eq!(c.dm_read(0x3_E018), Ok(7));
        assert_eq!((c.device_reads, c.device_writes), (0, 0));
        // unmapped addresses go to the sparse device store
        assert_eq!(c.dm_read(0x70_0000), Ok(0));
        c.dm_write(0x3108_D004, 7).unwrap();
        assert_eq!(c.dm_read(0x3108_D004), Ok(7));
        assert_eq!((c.device_reads, c.device_writes), (2, 1));
        c.preset_device(0x3108_D008, 5);
        c.dm_write(0x3108_D008, 9).unwrap();
        assert_eq!(c.dm_read(0x3108_D008), Ok(5));
    }

    #[test]
    fn short_word_window_is_the_same_memory_as_normal_words() {
        let mut c = Core::new(&[]);
        c.dm_write(0x9_2003, 0x1234_5678).unwrap();
        assert_eq!(c.dm_read(0x12_4006), Ok(0x5678));
        assert_eq!(c.dm_read(0x12_4007), Ok(0x1234));
        c.dm_write(0x12_4006, 0xFFFF_ABCD).unwrap();
        assert_eq!(
            c.dm_read(0x9_2003),
            Ok(0x1234_ABCD),
            "only the low 16 bits are stored"
        );
        assert_eq!((c.device_reads, c.device_writes), (0, 0));
    }

    #[test]
    fn watch_logs_writes_to_one_address() {
        let mut c = Core::new(&[]);
        c.watch = 0x9_0010;
        c.pc = 0x42;
        c.dm_write(0x9_0010, 7).unwrap();
        c.dm_write(0x9_0014, 8).unwrap();
        assert_eq!(c.watch_hits, vec![(0, 0x42, 7)]);
        assert_eq!(c.watch_addrs, vec![0x9_0010]);
        // a range logs the address of each hit and honours the limit
        let mut c = Core::new(&[]);
        (c.watch, c.watch_len, c.watch_limit) = (0x9_0010, 4, 2);
        for a in [0x9_000F, 0x9_0011, 0x9_0013, 0x9_0014, 0x9_0012] {
            c.dm_write(a, 1).unwrap();
        }
        assert_eq!(c.watch_addrs, vec![0x9_0011, 0x9_0013]);
    }

    #[test]
    fn rwatch_logs_loads_only_while_enabled() {
        let mut c = Core::new(&[]);
        (c.rwatch, c.rwatch_len) = (0x9_0010, 2);
        c.pc = 0x77;
        c.dm_read(0x9_0010).unwrap();
        assert!(c.rwatch_hits.is_empty(), "host reads are not core loads");
        c.rwatch_on = true;
        for a in [0x9_0010, 0x9_0011, 0x9_0012] {
            c.dm_read(a).unwrap();
        }
        assert_eq!(c.rwatch_hits, vec![(0x77, 0x9_0010), (0x77, 0x9_0011)]);
    }

    /// A write to `Bn` also loads `In` (the SMP voice loop `B1 = DM(rec-26)` then reads through `I1`);
    /// a write to `In` leaves `Bn` alone.
    #[test]
    fn writing_a_b_register_also_loads_the_index_register() {
        let mut c = Core::new(&[]);
        c.write_ureg(0x41, 0x9B200); // B1
        assert_eq!((c.b[1], c.i[1]), (0x9B200, 0x9B200));
        c.write_ureg(0x11, 0x9B210); // I1
        assert_eq!((c.b[1], c.i[1]), (0x9B200, 0x9B210));
        c.write_ureg(0x4D, 0x4000); // B13 (PM side)
        assert_eq!(c.i[13], 0x4000);
    }

    #[test]
    fn universal_registers_round_trip() {
        let mut c = Core::new(&[]);
        for (code, field) in [(0x03u8, 0), (0x12, 1), (0x2A, 2), (0x35, 3), (0x41, 4)] {
            c.write_ureg(code, 100 + code as u32);
            assert_eq!(c.read_ureg(code), 100 + code as u32, "field {field}");
        }
        c.write_ureg(0x72, 0x4F0);
        assert_eq!((c.mode1, c.read_ureg(0x72)), (0x4F0, 0x4F0));
        c.write_ureg(0x7F, 9);
        assert_eq!(c.ustat[3], 9);
        c.write_ureg(0x53, 77);
        assert_eq!(
            (c.s[3], c.read_ureg(0x53)),
            (77, 77),
            "S registers are group 5"
        );
        c.write_ureg(0x7C, 1);
        assert_eq!(c.read_ureg(0x7C), 0, "unlisted system registers read as 0");
    }

    #[test]
    fn loading_a_b_register_also_loads_its_index_register() {
        // census diff: the probe loads I0, then B0 = 0, and the simulator dumps I0 = 0
        let mut c = Core::new(&[]);
        c.write_ureg(0x10, 0xB2200); // I0
        c.write_ureg(0x40, 0); // B0
        assert_eq!((c.i[0], c.b[0]), (0, 0));
        c.write_ureg(0x41, 0x1234);
        assert_eq!((c.i[1], c.b[1]), (0x1234, 0x1234));
    }
}
