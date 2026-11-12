//! SPI-slave DMA of the ADSP-2148x family as used by the ADI boot kernel in the SY1SY2 and SMP
//! images (`docs/research/sharc-sy1sy2-smp.md` section 3).
//!
//! VERIFIED: IOP registers `IISPI 0x1080, IMSPI 0x1081, CSPI 0x1082, CPSPI 0x1083, SPIDMAC 0x1084`
//! (`.reference/sharc/ADSP-214xx_hwr_rev1.1.pdf` register table A-116 and the SPI-slave boot
//! table 24-10: reset `SPIDMAC = 7`, `IISPI = 0x92000`, `IMSPI = 1`, `CSPI = 0x180`; `SPIDMAC`
//! bit 15 = DMA in progress, bit 2 = interrupt on completion; the completion interrupt is SPIHI =
//! `P1I`, vector 12, IRPTL bit 12, which the kernel unmasks with `BIT SET IMASK 0x1000`).
//! The kernel itself (`tools/adi_dis.py --isa`) writes `IISPI`/`IMSPI`/`CSPI` at ISA words
//! `0x8C09C..0x8C0A5` and `0x8C19B..0x8C1A7` and polls `SPIDMAC` bit 15 at `0x8C0A8`.
//!
//! ASSUMED: the host stream is always ready, and a transfer started by a `CSPI` write completes
//! when the program waits for it: at the next read of `SPIDMAC` (the kernel polls bit 15) or when
//! the core enters `IDLE`. It must not complete earlier because the last block of the stream
//! overwrites the kernel's own code (VERIFIED: the 256-word interrupt table at the end of the
//! SY1SY2 and SMP images covers `0x8C000..0x8C0FF`, the whole kernel, and its word `0x8C08E` is
//! `jump (pc, 0)`), so the kernel only survives the transfer because it sleeps in `IDLE`.
//! A starved stream leaves the DMA in progress until the next [`SpiBoot::feed`] and wait.
use super::Bus;
use std::collections::VecDeque;

/// Register addresses (normal-word IOP window).
pub const IISPI: u32 = 0x1080;
pub const IMSPI: u32 = 0x1081;
pub const CSPI: u32 = 0x1082;
pub const CPSPI: u32 = 0x1083;
pub const SPIDMAC: u32 = 0x1084;
/// Interrupt vector number of SPIHI (`P1I`).
pub const SPIHI_VECTOR: u8 = 12;
const DEN: u32 = 1;
const RCV: u32 = 2;
const INTEN: u32 = 4;
const DMAS: u32 = 1 << 15;

/// The SPI receive DMA with the host's byte stream as words.
pub struct SpiBoot {
    ii: u32,
    im: u32,
    c: u32,
    cp: u32,
    dmac: u32,
    stream: VecDeque<u32>,
    /// Words delivered to memory so far.
    pub delivered: u64,
    irq: bool,
}

impl Default for SpiBoot {
    fn default() -> Self {
        SpiBoot {
            ii: 0x9_2000,
            im: 1,
            c: 0,
            cp: 0,
            dmac: DEN | RCV | INTEN,
            stream: VecDeque::new(),
            delivered: 0,
            irq: false,
        }
    }
}

impl SpiBoot {
    /// Whether `addr` is one of the modelled registers.
    pub fn contains(addr: u32) -> bool {
        (IISPI..=SPIDMAC).contains(&addr)
    }

    /// Words the host has not sent yet.
    pub fn queued(&self) -> usize {
        self.stream.len()
    }

    /// Register read; `SPIDMAC` bit 15 is set while a transfer waits for host data.
    pub fn read(&self, addr: u32) -> u32 {
        match addr {
            IISPI => self.ii,
            IMSPI => self.im,
            CSPI => self.c,
            CPSPI => self.cp,
            _ => self.dmac & !DMAS | if self.c > 0 { DMAS } else { 0 },
        }
    }

    /// Register write; writing `CSPI` starts the transfer.
    pub fn write(&mut self, addr: u32, v: u32) {
        match addr {
            IISPI => self.ii = v,
            IMSPI => self.im = v,
            CSPI => self.c = v & 0xFFFF,
            CPSPI => self.cp = v,
            _ => self.dmac = v,
        }
    }

    /// Appends host words.
    pub fn feed(&mut self, words: impl IntoIterator<Item = u32>) {
        self.stream.extend(words);
    }

    /// Whether a started transfer has not completed.
    pub fn busy(&self) -> bool {
        self.c > 0
    }

    /// Moves host words into memory until the count reaches zero or the stream is empty (the
    /// program waits for the DMA: a poll of `SPIDMAC` or `IDLE`).
    pub fn complete(&mut self, bus: &mut dyn Bus) {
        if self.dmac & (DEN | RCV) != DEN | RCV {
            return;
        }
        while self.c > 0 {
            let Some(w) = self.stream.pop_front() else {
                return;
            };
            bus.write32(self.ii, w);
            self.ii = self.ii.wrapping_add(self.im);
            self.c -= 1;
            self.delivered += 1;
            if self.c == 0 && self.dmac & INTEN != 0 {
                self.irq = true;
            }
        }
    }

    /// Takes the pending completion interrupt, if any.
    pub fn take_irq(&mut self) -> bool {
        std::mem::take(&mut self.irq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Ram(HashMap<u32, u32>);
    impl Bus for Ram {
        fn read32(&mut self, a: u32) -> u32 {
            self.0.get(&a).copied().unwrap_or(0)
        }
        fn write32(&mut self, a: u32, v: u32) {
            self.0.insert(a, v);
        }
    }

    #[test]
    fn reset_values_match_the_boot_table() {
        let s = SpiBoot::default();
        assert_eq!(
            (s.read(IISPI), s.read(IMSPI), s.read(CSPI), s.read(SPIDMAC)),
            (0x92000, 1, 0, 7)
        );
    }

    #[test]
    fn transfer_completes_and_interrupts() {
        let (mut s, mut ram) = (SpiBoot::default(), Ram::default());
        s.feed([10, 11, 12, 13]);
        s.write(IISPI, 0x92003);
        s.write(IMSPI, 1);
        s.write(CSPI, 3);
        assert!(
            s.busy() && ram.0.is_empty(),
            "nothing moves before the program waits"
        );
        s.complete(&mut ram);
        assert_eq!(
            [ram.0[&0x92003], ram.0[&0x92004], ram.0[&0x92005]],
            [10, 11, 12]
        );
        assert_eq!(s.read(SPIDMAC) & DMAS, 0);
        assert_eq!(s.read(IISPI), 0x92006);
        assert!(s.take_irq() && !s.take_irq());
        assert_eq!(s.queued(), 1);
    }

    #[test]
    fn starved_transfer_stays_busy_until_fed() {
        let (mut s, mut ram) = (SpiBoot::default(), Ram::default());
        s.write(IISPI, 0x100);
        s.write(CSPI, 2);
        s.complete(&mut ram);
        assert_eq!(s.read(SPIDMAC) & DMAS, DMAS);
        assert!(!s.take_irq());
        s.feed([1, 2]);
        s.complete(&mut ram);
        assert_eq!(s.read(SPIDMAC) & DMAS, 0);
        assert_eq!((ram.0[&0x100], ram.0[&0x101]), (1, 2));
        assert!(s.take_irq());
    }
}
