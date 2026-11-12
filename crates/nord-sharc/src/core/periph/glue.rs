//! Wiring of the peripheral models into [`Core`]: the [`Bus`] onto its data memory and the
//! enable, feed and access hooks used by `memory.rs`, `interrupts.rs` and `sharc-run`.
use super::sc57x::Sc57x;
use super::spi_boot::{self, SpiBoot};
use super::Bus;
use crate::core::Core;

/// [`Bus`] onto the data memory of a core (SRAM, device store).
struct CoreBus<'a>(&'a mut Core);

impl Bus for CoreBus<'_> {
    fn read32(&mut self, a: u32) -> u32 {
        self.0.dm_read(a).unwrap_or(0)
    }
    fn write32(&mut self, a: u32, v: u32) {
        let _ = self.0.dm_write(a, v);
        self.0.isa_resync(a);
    }
}

impl Core {
    /// Switches on the ADSP-2157x peripheral models for core `core` (0 = Core1).
    pub fn enable_sc57x(&mut self, core: usize) {
        self.periph.get_or_insert_with(Default::default).sc57x = Some(Sc57x::new(core));
    }

    /// Switches on the 2148x SPI-slave boot DMA fed from `words`.
    pub fn enable_spi_boot(&mut self, words: Vec<u32>) {
        let mut p = self.periph.take().unwrap_or_default();
        let mut boot = SpiBoot::default();
        boot.feed(words);
        p.boot = Some(boot);
        self.periph = Some(p);
    }

    /// The ADSP-2157x models, if enabled.
    pub fn sc57x(&mut self) -> Option<&mut Sc57x> {
        self.periph.as_mut()?.sc57x.as_mut()
    }

    /// Raises SEC source `id` (no-op without [`Core::enable_sc57x`]).
    pub fn sc57x_raise_source(&mut self, id: u32) {
        if let Some(s) = self.sc57x() {
            s.raise_source(id);
            let irqs = s.take_irqs();
            for v in irqs {
                self.raise_interrupt(v);
            }
        }
    }

    /// Feeds ARM-master words to SPI2 (no-op without [`Core::enable_sc57x`]).
    pub fn sc57x_feed_spi2(&mut self, words: &[u32]) {
        if let Some(mut p) = self.periph.take() {
            if let Some(s) = p.sc57x.as_mut() {
                s.feed_spi2(words, &mut CoreBus(self));
                for v in s.take_irqs() {
                    self.raise_interrupt(v);
                }
            }
            self.periph = Some(p);
        }
    }

    /// Completes a started boot DMA (the program polls or sleeps) and raises SPIHI.
    pub(in crate::core) fn periph_boot_wait(&mut self) {
        let Some(mut p) = self.periph.take() else {
            return;
        };
        let mut irq = false;
        if let Some(b) = p.boot.as_mut().filter(|b| b.busy()) {
            b.complete(&mut CoreBus(self));
            irq = b.take_irq();
        }
        self.periph = Some(p);
        if irq {
            self.raise_interrupt(spi_boot::SPIHI_VECTOR);
        }
    }

    #[inline]
    fn periph_range(addr: u32) -> bool {
        (0x2000_0000..0x3200_0000).contains(&addr)
            || (spi_boot::IISPI..=spi_boot::SPIDMAC).contains(&addr)
    }

    /// Register read through the models (`None` = not modelled or scripted by `--dev`).
    pub(in crate::core) fn periph_read(&mut self, addr: u32) -> Option<u32> {
        if !Self::periph_range(addr) || self.preset.contains_key(&addr) {
            return None;
        }
        if addr == spi_boot::SPIDMAC && self.periph.as_ref()?.boot.is_some() {
            self.periph_boot_wait(); // a poll of SPIDMAC waits for the transfer
        }
        let p = self.periph.as_mut()?;
        let v = match (&mut p.sc57x, &p.boot) {
            (_, Some(b)) if SpiBoot::contains(addr) => Some(b.read(addr)),
            (Some(s), _) => s.read(addr),
            _ => None,
        };
        if v.is_some() {
            self.device_reads += 1;
        }
        v
    }

    /// Register write through the models; returns whether a model took it.
    pub(in crate::core) fn periph_write(&mut self, addr: u32, v: u32) -> bool {
        if !Self::periph_range(addr) || self.periph.is_none() {
            return false;
        }
        let Some(mut p) = self.periph.take() else {
            return false;
        };
        let mut irqs = Vec::new();
        let taken = if let Some(b) = p.boot.as_mut().filter(|_| SpiBoot::contains(addr)) {
            b.write(addr, v);
            true
        } else if let Some(s) = p.sc57x.as_mut() {
            let t = s.write(addr, v, &mut CoreBus(self));
            irqs.extend(s.take_irqs());
            t
        } else {
            false
        };
        self.periph = Some(p);
        if taken {
            self.device_writes += 1;
        }
        for v in irqs {
            self.raise_interrupt(v);
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the ORG Core1 SPI2 peek-port set-up of `docs/research/dsp-rx-protocol.md` section 2.
    fn org_setup(c: &mut Core) {
        let ring = [
            (0x2000_4160u32, 0x2000_416Cu32, 0x2000_418Cu32, 0x424223u32),
            (0x2000_416C, 0x2000_4178, 0x2000_418C, 0x424223),
            (0x2000_4178, 0x2000_4160, 0x2000_418C, 0x424223),
        ];
        for (a, n, d, cfg) in ring {
            for (k, w) in [n, d, cfg].into_iter().enumerate() {
                c.dm_write(a + 4 * k as u32, w).unwrap();
            }
        }
        // TX descriptor {next = itself, ADDRSTART = [0x2000418C] = 0x20004184}; reply words at 0x20004184
        c.dm_write(0x2000_4188, 0x2000_4188).unwrap();
        c.dm_write(0x2000_418C, 0x2000_4184).unwrap();
        c.dm_write(0x2000_4184, 0xCAFE_BABE).unwrap();
        for (a, v) in [
            (0x3108_A070, 0x21), // TRU0_SSR28 = SPI2 RX DMA master
            (0x3104_4004, 0x6511),
            (0x3104_6200, 0x2000_4188),
            (0x3104_6208, 0x1C220),
            (0x3104_620C, 3),
            (0x3104_6210, 4),
            (0x3104_6208, 0x1C221),
            (0x3104_6280, 0x2000_4160),
            (0x3104_6288, 0x424222),
            (0x3104_628C, 1),
            (0x3104_6288, 0x424223),
        ] {
            c.dm_write(a, v).unwrap();
        }
    }

    #[test]
    fn peek_port_replies_with_three_words() {
        let mut c = Core::new(&[]);
        c.enable_sc57x(0);
        org_setup(&mut c);
        c.sc57x_feed_spi2(&[0x2000_5000]);
        let s = c.sc57x().unwrap();
        // the third reply word is the descriptor's own ADDRSTART word, i.e. the address just received
        assert_eq!(s.spi2.tx_out, vec![0xCAFE_BABE, 0x2000_4188, 0x2000_5000]);
        // the descriptor was fetched before the request arrived, so the reply to the next
        // request comes from the first address (ASSUMED fetch timing)
        for (k, w) in [11u32, 22, 33].into_iter().enumerate() {
            s.l2.write32(0x2000_5000 + 4 * k as u32, w);
        }
        c.sc57x_feed_spi2(&[0x2000_6000]);
        assert_eq!(c.sc57x().unwrap().spi2.tx_out[3..], [11, 22, 33]);
        assert_eq!(c.dm_read(0x2000_418C), Ok(0x2000_6000));
    }

    #[test]
    fn dma_done_reaches_the_core_through_the_sec() {
        let mut c = Core::new(&[]);
        c.enable_sc57x(0);
        // SPORT2 A RX (DMA4) 2 words into L2 with INT set
        for (a, v) in [
            (0x3102_2204, 0x2000_6000),
            (0x3102_220C, 2),
            (0x3102_2210, 4),
            (0x3102_2208, 1 << 20 | 0x200 | 2 | 1),
        ] {
            c.dm_write(a, v).unwrap();
        }
        assert_eq!(c.irq.irptl & 1 << 15, 0);
        c.sc57x().unwrap().lp1.rx.clear();
        // SPORT channels have no data source: inject the done event as the harness does
        c.sc57x_raise_source(90);
        assert_ne!(c.irq.irptl & 1 << 15, 0, "P4I latched");
        assert_eq!(c.dm_read(0x3108_945C), Ok(90));
        c.dm_write(0x3108_900C, 90).unwrap();
        assert_eq!(c.dm_read(0x3108_945C), Ok(0));
        // DMA4 STAT is a normal register
        c.dm_write(0x3102_2230, 1).unwrap();
        assert_eq!(c.dm_read(0x3102_2230).unwrap() & 1, 0);
    }

    #[test]
    fn rx_done_interrupt_from_lp1_sets_sec_source_79() {
        let mut c = Core::new(&[]);
        c.enable_sc57x(0);
        c.sc57x().unwrap().lp1.rx.extend([5, 6]);
        for (a, v) in [
            (0x30FF_F084, 0x2000_7000),
            (0x30FF_F08C, 2),
            (0x30FF_F090, 4),
            (0x30FF_F088, 1 << 20 | 0x200 | 2 | 1),
        ] {
            c.dm_write(a, v).unwrap();
        }
        assert_eq!(c.dm_read(0x2000_7004), Ok(6));
        assert_eq!(c.dm_read(0x3108_945C), Ok(79));
    }

    #[test]
    fn scripted_presets_and_unmodelled_registers_keep_the_device_store() {
        let mut c = Core::new(&[]);
        c.enable_sc57x(0);
        c.preset_device(0x3108_945C, 0x41);
        assert_eq!(c.dm_read(0x3108_945C), Ok(0x41));
        c.dm_write(0x3108_D004, 7).unwrap(); // CGU0: not modelled
        assert_eq!(c.dm_read(0x3108_D004), Ok(7));
        // models are off by default
        let mut plain = Core::new(&[]);
        plain.dm_write(0x3108_900C, 5).unwrap();
        assert_eq!(plain.dm_read(0x3108_900C), Ok(5));
    }

    #[test]
    fn spi_boot_dma_fills_memory_and_wakes_through_spihi() {
        let mut c = Core::new(&[]);
        c.enable_spi_boot(vec![7, 8, 9]);
        c.dm_write(spi_boot::IISPI, 0x9_2003).unwrap();
        c.dm_write(spi_boot::IMSPI, 1).unwrap();
        c.dm_write(spi_boot::CSPI, 3).unwrap();
        assert_eq!(
            c.dm_read(0x9_2005),
            Ok(0),
            "not delivered before the program waits"
        );
        assert_eq!(
            c.dm_read(spi_boot::SPIDMAC).unwrap() & 1 << 15,
            0,
            "the poll waits"
        );
        assert_eq!(c.dm_read(0x9_2005), Ok(9));
        assert_ne!(c.irq.irptl & 1 << 12, 0, "SPIHI = P1I latched");
        // no boot model: the IOP window is plain storage
        let mut plain = Core::new(&[]);
        plain.dm_write(spi_boot::CSPI, 3).unwrap();
        assert_eq!(plain.dm_read(spi_boot::CSPI), Ok(3));
    }
}
