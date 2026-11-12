//! L2 SRAM of the ADSP-2157x: 1 MiB of byte-addressed memory at `0x2000_0000`.
//!
//! VERIFIED: `SHARC/ldf/ADSP-SC573.ldf` (CCES 2.12.1) maps `0x20000000..0x200FFFFF` as L2 and the
//! ORG, PNO and FX loader streams place their DMA descriptor rings at `0x20004000` and their
//! L2 data above `0x20080000` (`python tools/ns4_dsp_ldr.py IMAGE --blocks`).
//! The backing store is allocated on first use so cores that never touch L2 pay nothing.

/// First byte address of L2.
pub const BASE: u32 = 0x2000_0000;
/// Size of L2 in bytes.
pub const BYTES: usize = 0x10_0000;

/// Byte-addressed L2 backing store (little endian words).
#[derive(Default)]
pub struct L2 {
    mem: Vec<u8>,
}

impl L2 {
    /// Whether byte address `addr` lies in L2.
    #[inline]
    pub fn contains(addr: u32) -> bool {
        addr.wrapping_sub(BASE) < BYTES as u32
    }

    fn offset(addr: u32) -> usize {
        (addr.wrapping_sub(BASE) as usize) % BYTES
    }

    /// The byte at `addr` (0 when never written).
    pub fn read8(&self, addr: u32) -> u8 {
        self.mem.get(Self::offset(addr)).copied().unwrap_or(0)
    }

    /// Stores one byte.
    pub fn write8(&mut self, addr: u32, value: u8) {
        if self.mem.is_empty() {
            self.mem = vec![0; BYTES];
        }
        self.mem[Self::offset(addr)] = value;
    }

    /// The 32-bit little-endian word at byte address `addr` (any alignment).
    pub fn read32(&self, addr: u32) -> u32 {
        (0..4).fold(0, |w, k| w | u32::from(self.read8(addr + k)) << (8 * k))
    }

    /// Stores a 32-bit little-endian word at byte address `addr`.
    pub fn write32(&mut self, addr: u32, value: u32) {
        for (k, b) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(addr + k as u32, b);
        }
    }

    /// Copies `bytes` to `addr` (a loader block).
    pub fn load(&mut self, addr: u32, bytes: &[u8]) {
        for (k, &b) in bytes.iter().enumerate() {
            self.write8(addr + k as u32, b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_and_words() {
        assert!(L2::contains(0x2000_0000) && L2::contains(0x200F_FFFF));
        assert!(!L2::contains(0x1FFF_FFFF) && !L2::contains(0x2010_0000));
        let mut l2 = L2::default();
        assert_eq!(l2.read32(0x2000_4000), 0, "never written reads zero");
        l2.write32(0x2000_4000, 0xCAFE_BABE);
        assert_eq!(l2.read32(0x2000_4000), 0xCAFE_BABE);
        assert_eq!(l2.read8(0x2000_4000), 0xBE);
        l2.write8(0x2000_4001, 0x11);
        assert_eq!(l2.read32(0x2000_4000), 0xCAFE_11BE);
    }

    #[test]
    fn loader_block() {
        let mut l2 = L2::default();
        l2.load(0x2000_4160, &[1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(l2.read32(0x2000_4164), 2);
    }
}
