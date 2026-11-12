//! The ADSP-2157x peripheral set behind the data-memory window: SEC0, SPI2, descriptor-list DMA,
//! TRU routing between channels, LP1 and L2 (see the module docs of `periph` and
//! `docs/research/sharc-chip-identity.md`).
use super::dma::{Channel, Events, Port};
use super::l2::L2;
use super::sec::Sec;
use super::spi2::Spi2;
use super::Bus;
use std::collections::{HashMap, VecDeque};

pub const SEC_VECTOR: u8 = 15;
const TRU_BASE: u32 = 0x3108_A000;
const LP1_BASE: u32 = 0x30FF_E100;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Spi2Tx,
    Spi2Rx,
    Lp1,
    /// SPORT channel: registers only, no data source modelled.
    Passive,
}

struct Slot {
    base: u32,
    kind: Kind,
    /// TRU master id this channel fires and slave id it waits on (`TRGM_*`, `TRGS_*` in the CCES header).
    master: u32,
    slave: u32,
    /// SEC source id of its done interrupt (`*_IRQn`).
    sec_id: u32,
    ch: Channel,
}

/// Word queues of the LP1 link port RX/TX DMA (DMA36).
#[derive(Default)]
pub struct Lp1 {
    pub rx: VecDeque<u32>,
    pub tx: Vec<u32>,
}

impl Port for Lp1 {
    fn rx_pop(&mut self) -> Option<u32> {
        self.rx.pop_front()
    }
    fn tx_push(&mut self, w: u32) {
        self.tx.push(w);
    }
}

struct L2Bus<'a> {
    l2: &'a mut L2,
    ext: &'a mut dyn Bus,
}

impl Bus for L2Bus<'_> {
    fn read32(&mut self, a: u32) -> u32 {
        if L2::contains(a) {
            self.l2.read32(a)
        } else {
            self.ext.read32(a)
        }
    }
    fn write32(&mut self, a: u32, v: u32) {
        if L2::contains(a) {
            self.l2.write32(a, v);
        } else {
            self.ext.write32(a, v);
        }
    }
}

/// The ADSP-2157x peripherals.
pub struct Sc57x {
    pub l2: L2,
    pub sec: Sec,
    pub spi2: Spi2,
    pub lp1: Lp1,
    tru: Vec<u32>,
    slots: Vec<Slot>,
    generic: HashMap<u32, u32>,
    /// 0 = Core1, 1 = Core2: the SEC core interface this core reads.
    pub core: usize,
    irqs: Vec<u8>,
}

impl Sc57x {
    /// A fresh peripheral set for core `core` (0 or 1).
    pub fn new(core: usize) -> Self {
        // DMA0..7 = SPORT0A, 0B, 1A, 1B, 2A, 2B, 3A, 3B (SEC ids and TRU ids from the CCES header)
        const SPORT_SEC: [u32; 8] = [60, 62, 64, 66, 90, 92, 94, 96];
        let mut slots: Vec<Slot> = (0..8u32)
            .map(|n| Slot {
                base: 0x3102_2000 + 0x80 * n,
                kind: Kind::Passive,
                master: 20 + n,
                slave: 16 + n,
                sec_id: SPORT_SEC[n as usize],
                ch: Channel::default(),
            })
            .collect();
        for (base, kind, master, slave, sec_id) in [
            (0x3104_6200, Kind::Spi2Tx, 32, 28, 68),
            (0x3104_6280, Kind::Spi2Rx, 33, 29, 69),
            (0x30FF_F080, Kind::Lp1, 40, 33, 79),
        ] {
            slots.push(Slot {
                base,
                kind,
                master,
                slave,
                sec_id,
                ch: Channel::default(),
            });
        }
        Sc57x {
            l2: L2::default(),
            sec: Sec::default(),
            spi2: Spi2::default(),
            lp1: Lp1::default(),
            tru: vec![0; 256],
            slots,
            generic: HashMap::new(),
            core: core.min(1),
            irqs: Vec::new(),
        }
    }

    fn slot_of(&self, addr: u32) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| addr.wrapping_sub(s.base) < 0x80)
    }

    fn claims(&self, addr: u32) -> bool {
        L2::contains(addr)
            || Sec::contains(addr)
            || Spi2::contains(addr)
            || addr.wrapping_sub(TRU_BASE) < 0x1000
            || addr.wrapping_sub(LP1_BASE) < 0x100
            || self.slot_of(addr).is_some()
    }

    /// Reads a register; `None` when `addr` is not modelled (the caller uses the device store).
    pub fn read(&mut self, addr: u32) -> Option<u32> {
        if !self.claims(addr) {
            return None;
        }
        Some(if L2::contains(addr) {
            self.l2.read32(addr)
        } else if Sec::contains(addr) {
            self.sec.read(addr)
        } else if Spi2::contains(addr) {
            self.spi2.read(addr)
        } else if addr.wrapping_sub(TRU_BASE) < 0x400 {
            self.tru[((addr - TRU_BASE) / 4) as usize]
        } else if let Some(i) = self.slot_of(addr) {
            self.slots[i].ch.read(addr - self.slots[i].base)
        } else {
            self.generic.get(&addr).copied().unwrap_or(0)
        })
    }

    /// Writes a register and runs the DMA engines; returns whether `addr` is modelled.
    pub fn write(&mut self, addr: u32, v: u32, ext: &mut dyn Bus) -> bool {
        if !self.claims(addr) {
            return false;
        }
        if L2::contains(addr) {
            self.l2.write32(addr, v);
        } else if Sec::contains(addr) {
            self.sec.write(addr, v);
            self.poll_sec();
        } else if Spi2::contains(addr) {
            self.spi2.write(addr, v);
        } else if addr.wrapping_sub(TRU_BASE) < 0x400 {
            self.tru[((addr - TRU_BASE) / 4) as usize] = v;
        } else if let Some(i) = self.slot_of(addr) {
            let off = addr - self.slots[i].base;
            self.slots[i].ch.write(off, v);
        } else {
            self.generic.insert(addr, v);
        }
        self.run_dma(ext);
        true
    }

    /// One line per enabled DMA channel plus the SPI2 state, for `sharc-run`.
    pub fn report(&self) -> String {
        let mut out = format!(
            "spi2 ctl 0x{:x} enabled {} rx queued {} tx sent {}
",
            self.spi2.read(0x3104_4004),
            self.spi2.enabled(),
            self.spi2.rx_in.len(),
            self.spi2.tx_out.len()
        );
        for s in self
            .slots
            .iter()
            .filter(|s| s.ch.enabled() || s.ch.cfg != 0)
        {
            out.push_str(&format!(
                "dma 0x{:08x} {:?} cfg 0x{:x} nxt 0x{:x} cur desc 0x{:x} xcnt {}/{} addr 0x{:x} stat 0x{:x}
",
                s.base,
                s.kind,
                s.ch.cfg,
                s.ch.nxt,
                s.ch.dscptr_cur,
                s.ch.xcnt_cur,
                s.ch.xcnt,
                s.ch.addr_cur,
                s.ch.stat
            ));
        }
        out
    }

    fn poll_sec(&mut self) {
        if self.sec.poll_irq(self.core) {
            self.irqs.push(SEC_VECTOR);
        }
    }

    /// Raises SEC source `id` towards this core (a peripheral event the harness injects).
    pub fn raise_source(&mut self, id: u32) {
        self.sec.raise(self.core, id);
        self.poll_sec();
    }

    /// Takes the core interrupt vectors raised since the last call.
    pub fn take_irqs(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.irqs)
    }

    /// Adds ARM-master words to the SPI2 receive side and moves what the DMA can take.
    pub fn feed_spi2(&mut self, words: &[u32], ext: &mut dyn Bus) {
        self.spi2.rx_in.extend(words.iter().copied());
        self.run_dma(ext);
    }

    /// Runs every enabled DMA channel until nothing moves (bounded), firing TRU triggers and SEC
    /// interrupts from completed work units.
    pub fn run_dma(&mut self, ext: &mut dyn Bus) {
        for _ in 0..8 {
            let mut events: Vec<(usize, Events)> = Vec::new();
            for i in 0..self.slots.len() {
                let Sc57x {
                    l2,
                    spi2,
                    lp1,
                    slots,
                    ..
                } = &mut *self;
                let slot = &mut slots[i];
                if !slot.ch.enabled() {
                    continue;
                }
                let mut bus = L2Bus { l2, ext: &mut *ext };
                let ev = match slot.kind {
                    Kind::Spi2Tx | Kind::Spi2Rx => slot.ch.pump(&mut bus, spi2),
                    Kind::Lp1 => slot.ch.pump(&mut bus, lp1),
                    Kind::Passive => Events::default(),
                };
                if ev != Events::default() {
                    events.push((i, ev));
                }
            }
            if events.is_empty() {
                return;
            }
            for (i, ev) in events {
                if ev.done {
                    let id = self.slots[i].sec_id;
                    self.sec.raise(self.core, id);
                    self.poll_sec();
                }
                if ev.trig {
                    let master = self.slots[i].master;
                    for k in 0..self.slots.len() {
                        if self.tru[self.slots[k].slave as usize] == master {
                            self.slots[k].ch.trigger();
                        }
                    }
                }
            }
        }
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
    fn claims_only_modelled_blocks() {
        let mut s = Sc57x::new(0);
        let mut ram = Ram::default();
        assert!(s.write(0x3104_4004, 1, &mut ram) && s.spi2.enabled());
        assert!(s.write(0x2000_4000, 7, &mut ram));
        assert_eq!(s.read(0x2000_4000), Some(7));
        assert!(s.write(0x30FF_E100, 3, &mut ram), "LP1 registers store");
        assert_eq!(s.read(0x30FF_E100), Some(3));
        // CGU0, DAI0, PORTA, SPU0 and RCU0 are left to the device store
        for a in [
            0x3108_D004,
            0x310C_9100,
            0x3100_4000,
            0x3108_B980,
            0x3108_C06C,
        ] {
            assert_eq!(s.read(a), None);
            assert!(!s.write(a, 1, &mut ram));
        }
    }

    #[test]
    fn tru_routes_a_done_trigger_to_the_waiting_channel_only_when_wired() {
        let mut s = Sc57x::new(0);
        let mut ram = Ram::default();
        ram.0.insert(0x1000, 0x1000); // TX descriptor {next, ADDRSTART} at 0x1000, 0x1004
        ram.0.insert(0x1004, 0x2000);
        ram.0.insert(0x2000, 0xAA);
        for (a, v) in [
            (0x3104_6200, 0x1000), // DMA26 SPI2 TX: list, NDSIZE 1, TWAIT, one word
            (0x3104_6208, 0x1C220),
            (0x3104_620C, 1),
            (0x3104_6208, 0x1C221),
            (0x3104_6284, 0x3000), // DMA27 SPI2 RX: one word, TRIG, no flow
            (0x3104_628C, 1),
            (0x3104_6288, 0x40_0223),
            (0x3104_4004, 1),
        ] {
            assert!(s.write(a, v, &mut ram));
        }
        s.feed_spi2(&[5], &mut ram);
        assert_eq!(ram.0[&0x3000], 5);
        assert!(
            s.spi2.tx_out.is_empty(),
            "TRU0_SSR28 not written: no trigger"
        );
        s.write(0x3108_A070, 33, &mut ram);
        s.write(0x3104_6288, 0x40_0222, &mut ram); // disarm RX, then re-arm
        s.write(0x3104_6288, 0x40_0223, &mut ram);
        s.feed_spi2(&[6], &mut ram);
        assert_eq!(s.spi2.tx_out, vec![0xAA]);
        assert!(s.report().contains("Spi2Tx"));
    }

    #[test]
    fn done_interrupt_goes_through_sec_to_p4i_once() {
        let mut s = Sc57x::new(1);
        s.raise_source(69);
        s.raise_source(70);
        assert_eq!(
            s.take_irqs(),
            vec![SEC_VECTOR],
            "one edge for two pending sources"
        );
        assert_eq!(s.read(0x3108_949C), Some(69), "core 2 reads CSID2");
        s.write(0x3108_900C, 69, &mut Ram::default());
        assert_eq!(s.take_irqs(), vec![SEC_VECTOR], "second source after END");
    }
}
