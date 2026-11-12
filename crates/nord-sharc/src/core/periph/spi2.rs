//! SPI2 of the ADSP-2157x (`0x3104_4000`), the link to the ARM host in slave mode.
//!
//! Offsets VERIFIED against `ADI_SPI_TypeDef` (CCES 2.12.1 `ADSP_21571_device.h`): `CTL +0x04, RXCTL
//! +0x08, TXCTL +0x0C, CLK +0x10, DLY +0x14, SLVSEL +0x18, RWC +0x1C, RWCR +0x20, TWC +0x24, TWCR
//! +0x28, IMSK +0x30, STAT +0x40, RFIFO +0x50, TFIFO +0x58`. The ORG/FX Core1 init writes
//! `CTL = 0x6510` (slave, 32-bit, MISO on, flow control on TX), `RXCTL = 0x10`, `TXCTL = 0x110`,
//! `RWC = RWCR = 1`, `TWC = TWCR = 3` and `TFIFO = 0xCAFEBABE` twice.
//!
//! ASSUMED: the ARM master side is a word queue, words are delivered to RX DMA only while
//! `CTL.EN` is set, `TFIFO` writes go out before any DMA transmit words, `STAT` reports only
//! the receive-FIFO-empty flag. None of this is measured.
use super::dma::Port;
use std::collections::VecDeque;

/// Base address of SPI2.
pub const BASE: u32 = 0x3104_4000;
/// Size of the register window.
pub const SIZE: u32 = 0x100;
const CTL: u32 = 0x04;
const STAT: u32 = 0x40;
const TFIFO: u32 = 0x58;
/// `STAT.RFE`: receive FIFO empty.
const RFE: u32 = 1 << 22;

/// SPI2 state.
pub struct Spi2 {
    regs: [u32; 64],
    /// Words the ARM master clocked in, not yet taken by the RX DMA.
    pub rx_in: VecDeque<u32>,
    /// Words clocked out to the master, in order.
    pub tx_out: Vec<u32>,
}

impl Default for Spi2 {
    fn default() -> Self {
        Spi2 {
            regs: [0; 64],
            rx_in: VecDeque::new(),
            tx_out: Vec::new(),
        }
    }
}

impl Spi2 {
    /// Whether `addr` is an SPI2 register.
    pub fn contains(addr: u32) -> bool {
        addr.wrapping_sub(BASE) < SIZE
    }

    /// Whether `CTL.EN` is set.
    pub fn enabled(&self) -> bool {
        self.regs[(CTL / 4) as usize] & 1 != 0
    }

    /// Register read.
    pub fn read(&self, addr: u32) -> u32 {
        let off = addr - BASE;
        let v = self.regs[(off / 4) as usize % 64];
        if off == STAT {
            return v & !RFE | if self.rx_in.is_empty() { RFE } else { 0 };
        }
        v
    }

    /// Register write.
    pub fn write(&mut self, addr: u32, value: u32) {
        let off = addr - BASE;
        if off == TFIFO {
            self.tx_out.push(value);
        } else if off == STAT {
            self.regs[(STAT / 4) as usize] &= !value; // write 1 to clear
        } else {
            self.regs[(off / 4) as usize % 64] = value;
        }
    }
}

impl Port for Spi2 {
    fn rx_pop(&mut self) -> Option<u32> {
        if self.enabled() {
            self.rx_in.pop_front()
        } else {
            None
        }
    }

    fn tx_push(&mut self, word: u32) {
        self.tx_out.push(word);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_store_and_tfifo_queues() {
        let mut s = Spi2::default();
        s.write(0x3104_4004, 0x6510);
        s.write(0x3104_4024, 3);
        assert_eq!(s.read(0x3104_4004), 0x6510);
        assert_eq!(s.read(0x3104_4024), 3);
        assert!(!s.enabled());
        s.write(0x3104_4004, 0x6511);
        assert!(s.enabled());
        s.write(0x3104_4058, 0xCAFE_BABE);
        assert_eq!(s.tx_out, vec![0xCAFE_BABE]);
        assert!(Spi2::contains(0x3104_4058) && !Spi2::contains(0x3104_4100));
    }

    #[test]
    fn rx_needs_enable_and_stat_reports_empty() {
        let mut s = Spi2::default();
        s.rx_in.push_back(9);
        assert_eq!(s.rx_pop(), None, "disabled port takes nothing");
        assert_eq!(s.read(0x3104_4040) & RFE, 0);
        s.write(0x3104_4004, 1);
        assert_eq!(s.rx_pop(), Some(9));
        assert_eq!(s.read(0x3104_4040) & RFE, RFE);
    }
}
