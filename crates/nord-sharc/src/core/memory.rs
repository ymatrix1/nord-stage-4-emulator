//! Data memory (SRAM, IOP window, device store) and the universal register file of [`Core`].
use super::{Core, IOP_WORDS, SRAM_BYTES};

/// Normal-word addresses backed by the flat SRAM image (blocks 1 to 3 of the ADSP-21489).
const SRAM_WORDS: std::ops::Range<u32> = 0x9_0000..0x10_0000;

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
        self.device_reads += 1;
        if let Some(v) = self.preset.get(&addr) {
            return Ok(*v);
        }
        Ok(self.device.get(&addr).copied().unwrap_or(0))
    }

    /// Writes a 32-bit data word at a normal-word address (see [`Core::dm_read`]).
    pub fn dm_write(&mut self, addr: u32, value: u32) -> Result<(), u32> {
        if (addr as usize) < IOP_WORDS {
            self.iop[addr as usize] = value;
            return Ok(());
        }
        if let Some(o) = sram_offset(addr) {
            self.sram[o..o + 4].copy_from_slice(&value.to_le_bytes());
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

    /// Value of a 7-bit universal register code (Table 10-2, PEx view).
    pub fn read_ureg(&self, code: u8) -> u32 {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => self.r[n],
            1 => self.i[n],
            2 => self.m[n],
            3 => self.l[n],
            4 => self.b[n],
            6 => match n {
                3 => self.pc,
                7 | 8 => self.loops.last().map_or(0, |f| f.remaining),
                _ => 0,
            },
            7 => match n {
                0 | 1 => self.ustat[n],
                2 => self.mode1,
                3 => self.mmask,
                4 => self.mode2,
                11 => self.imask,
                14 | 15 => self.ustat[n - 12],
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Writes a universal register (system registers not listed are accepted and ignored).
    pub fn write_ureg(&mut self, code: u8, v: u32) {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => self.r[n] = v,
            1 => self.i[n] = v,
            2 => self.m[n] = v,
            3 => self.l[n] = v,
            4 => self.b[n] = v,
            7 => match n {
                0 | 1 => self.ustat[n] = v,
                2 => self.mode1 = v,
                3 => self.mmask = v,
                4 => self.mode2 = v,
                11 => self.imask = v,
                14 | 15 => self.ustat[n - 12] = v,
                _ => {}
            },
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
        c.write_ureg(0x75, 1);
        assert_eq!(c.read_ureg(0x75), 0, "unlisted system registers read as 0");
    }
}
