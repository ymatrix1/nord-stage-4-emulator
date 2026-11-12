//! Descriptor-list DMA channel of the ADSP-2157x (`ADI_DMA_TypeDef`).
//!
//! Register offsets VERIFIED against CCES 2.12.1 `ADSP_21571_device.h`: `DSCPTR_NXT +0x00,
//! ADDRSTART +0x04, CFG +0x08, XCNT +0x0C, XMOD +0x10, YCNT +0x14, YMOD +0x18, DSCPTR_CUR +0x24,
//! DSCPTR_PRV +0x28, ADDR_CUR +0x2C, STAT +0x30, XCNT_CUR +0x34, YCNT_CUR +0x38`; the ORG image
//! writes `+0x00/08/0C/10/14/18` of `0x31046200/0x31046280` and clears `DM(0x31022230)` = DMA4 `STAT`.
//! CFG bits VERIFIED from `BITP_DMA_CFG_*`: EN 0, WNR 1, PSIZE 4, MSIZE 8, FLOW 12 (1 auto, 4 list,
//! 5 array), TWAIT 15, NDSIZE 16, INT 20, TRIG 22, TWOD 26.
//!
//! Descriptor layout ASSUMED, consistent with every ring in the ORG loader data (`sharc-chip-identity.md`
//! section 5): in list mode a descriptor is `[next, ADDRSTART, CFG, XCNT, XMOD, YCNT, YMOD]`
//! truncated to `1 + NDSIZE` words, in array mode the `NDSIZE` words without `next`. At the end of
//! a row of a 2D unit the address moves by YMOD from the last element instead of by XMOD.
use super::Bus;

const EN: u32 = 1;
const WNR: u32 = 2;
const TWAIT: u32 = 1 << 15;
const TWOD: u32 = 1 << 26;
/// STAT bit 0: work unit done (write 1 to clear).
pub const IRQDONE: u32 = 1;
/// Most elements moved by one [`Channel::pump`] call (stops autobuffer loops).
const BUDGET: u32 = 1 << 16;

/// Peripheral side of a channel: the word source for RX, the word sink for TX.
pub trait Port {
    /// Next word for a peripheral-to-memory channel, `None` when the peripheral has none.
    fn rx_pop(&mut self) -> Option<u32>;
    /// A word read from memory by a memory-to-peripheral channel.
    fn tx_push(&mut self, word: u32);
}

/// What a pump call completed.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Events {
    /// A work unit finished with `CFG.INT != 0`.
    pub done: bool,
    /// A work unit finished with `CFG.TRIG != 0` (fires the channel's TRU master).
    pub trig: bool,
}

/// One DMA channel.
#[derive(Default, Clone, Debug)]
pub struct Channel {
    pub nxt: u32,
    pub addrstart: u32,
    pub cfg: u32,
    pub xcnt: u32,
    pub xmod: i32,
    pub ycnt: u32,
    pub ymod: i32,
    pub dscptr_cur: u32,
    pub dscptr_prv: u32,
    pub addr_cur: u32,
    pub stat: u32,
    pub xcnt_cur: u32,
    pub ycnt_cur: u32,
    arm: bool,
    loaded: bool,
    waiting: bool,
    trig_latched: bool,
}

impl Channel {
    /// Register read at byte offset `off` of the channel block.
    pub fn read(&self, off: u32) -> u32 {
        match off {
            0x00 => self.nxt,
            0x04 => self.addrstart,
            0x08 => self.cfg,
            0x0C => self.xcnt,
            0x10 => self.xmod as u32,
            0x14 => self.ycnt,
            0x18 => self.ymod as u32,
            0x24 => self.dscptr_cur,
            0x28 => self.dscptr_prv,
            0x2C => self.addr_cur,
            0x30 => self.stat | u32::from(self.cfg & EN != 0 && self.loaded) << 8,
            0x34 => self.xcnt_cur,
            0x38 => self.ycnt_cur,
            _ => 0,
        }
    }

    /// Register write; setting `CFG.EN` arms the channel.
    pub fn write(&mut self, off: u32, v: u32) {
        match off {
            0x00 => self.nxt = v,
            0x04 => self.addrstart = v,
            0x08 => {
                if v & EN != 0 && self.cfg & EN == 0 {
                    self.arm = true;
                    self.loaded = false;
                }
                self.cfg = v;
            }
            0x0C => self.xcnt = v,
            0x10 => self.xmod = v as i32,
            0x14 => self.ycnt = v,
            0x18 => self.ymod = v as i32,
            0x30 => self.stat &= !(v & (IRQDONE | 2 | 4)), // W1C
            _ => {}
        }
    }

    /// A TRU trigger arrived: releases a unit waiting on `TWAIT`, otherwise latches it.
    pub fn trigger(&mut self) {
        if self.waiting {
            self.waiting = false;
        } else {
            self.trig_latched = true;
        }
    }

    /// Whether the channel is enabled.
    pub fn enabled(&self) -> bool {
        self.cfg & EN != 0
    }

    fn fetch(&mut self, bus: &mut dyn Bus) {
        let flow = self.cfg >> 12 & 7;
        let n = (self.cfg >> 16 & 7) as usize;
        let mut ptr = self.nxt;
        self.dscptr_prv = self.dscptr_cur;
        self.dscptr_cur = ptr;
        if flow == 4 {
            self.nxt = bus.read32(ptr);
            ptr = ptr.wrapping_add(4);
        }
        let mut words = [0u32; 6];
        for (k, w) in words.iter_mut().enumerate().take(n.min(6)) {
            *w = bus.read32(ptr.wrapping_add(4 * k as u32));
        }
        if flow == 5 {
            self.nxt = ptr.wrapping_add(4 * n as u32);
        }
        let regs: [&mut u32; 6] = [
            &mut self.addrstart,
            &mut self.cfg,
            &mut self.xcnt,
            &mut 0,
            &mut self.ycnt,
            &mut 0,
        ];
        for (k, r) in regs.into_iter().enumerate().take(n.min(6)) {
            *r = words[k];
        }
        if n > 3 {
            self.xmod = words[3] as i32;
        }
        if n > 5 {
            self.ymod = words[5] as i32;
        }
        self.cfg |= EN;
    }

    fn load_unit(&mut self, bus: &mut dyn Bus) {
        let flow = self.cfg >> 12 & 7;
        if (flow == 4 || flow == 5) && self.cfg >> 16 & 7 != 0 {
            self.fetch(bus);
        }
        self.addr_cur = self.addrstart;
        self.xcnt_cur = self.xcnt;
        self.ycnt_cur = if self.cfg & TWOD != 0 { self.ycnt } else { 1 };
        self.loaded = true;
        self.waiting = self.cfg & TWAIT != 0 && !std::mem::take(&mut self.trig_latched);
    }

    /// Moves words until the peripheral has none, a unit waits for a trigger, the channel stops
    /// or the budget is spent.
    pub fn pump(&mut self, bus: &mut dyn Bus, port: &mut dyn Port) -> Events {
        let mut ev = Events::default();
        for _ in 0..BUDGET {
            if self.cfg & EN == 0 {
                break;
            }
            if self.arm || !self.loaded {
                self.arm = false;
                self.load_unit(bus);
            }
            if self.waiting {
                break;
            }
            let size = 1u32 << (self.cfg >> 8 & 7).min(2);
            if self.cfg & WNR != 0 {
                let Some(w) = port.rx_pop() else { break };
                write_n(bus, self.addr_cur, size, w);
            } else {
                port.tx_push(read_n(bus, self.addr_cur, size));
            }
            self.advance(&mut ev);
        }
        ev
    }

    fn advance(&mut self, ev: &mut Events) {
        self.xcnt_cur -= 1;
        if self.xcnt_cur > 0 {
            self.addr_cur = self.addr_cur.wrapping_add(self.xmod as u32);
            return;
        }
        self.ycnt_cur -= 1;
        if self.ycnt_cur > 0 {
            self.xcnt_cur = self.xcnt;
            self.addr_cur = self.addr_cur.wrapping_add(self.ymod as u32);
            return;
        }
        self.loaded = false;
        if self.cfg >> 20 & 3 != 0 {
            self.stat |= IRQDONE;
            ev.done = true;
        }
        ev.trig |= self.cfg >> 22 & 3 != 0;
        let flow = self.cfg >> 12 & 7;
        let listed = (flow == 4 || flow == 5) && self.cfg >> 16 & 7 != 0;
        if flow == 0 || (flow > 1 && !listed) {
            self.cfg &= !EN;
        }
    }
}

fn read_n(bus: &mut dyn Bus, addr: u32, size: u32) -> u32 {
    let w = bus.read32(addr & !3);
    match size {
        4 => w,
        2 => w >> (8 * (addr & 2)) & 0xFFFF,
        _ => w >> (8 * (addr & 3)) & 0xFF,
    }
}

fn write_n(bus: &mut dyn Bus, addr: u32, size: u32, v: u32) {
    if size == 4 {
        return bus.write32(addr, v);
    }
    let shift = 8 * (addr & 3);
    let mask = if size == 2 { 0xFFFF } else { 0xFF } << shift;
    let old = bus.read32(addr & !3);
    bus.write32(addr & !3, old & !mask | (v << shift) & mask);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    #[derive(Default)]
    pub struct Ram(pub HashMap<u32, u32>);
    impl Bus for Ram {
        fn read32(&mut self, a: u32) -> u32 {
            self.0.get(&a).copied().unwrap_or(0)
        }
        fn write32(&mut self, a: u32, v: u32) {
            self.0.insert(a, v);
        }
    }
    #[derive(Default)]
    struct Pipe(VecDeque<u32>, Vec<u32>);
    impl Port for Pipe {
        fn rx_pop(&mut self) -> Option<u32> {
            self.0.pop_front()
        }
        fn tx_push(&mut self, w: u32) {
            self.1.push(w);
        }
    }

    /// The ORG SPI2 RX ring: three 1-word units, all into one word (list flow, NDSIZE 2).
    #[test]
    fn rx_descriptor_ring_cycles_and_flags_done() {
        let mut ram = Ram::default();
        for (a, d) in [
            (0x2000_4160u32, [0x2000_416Cu32, 0x2000_418C, 0x424223]),
            (0x2000_416C, [0x2000_4178, 0x2000_418C, 0x424223]),
            (0x2000_4178, [0x2000_4160, 0x2000_418C, 0x424223]),
        ] {
            for (k, w) in d.into_iter().enumerate() {
                ram.0.insert(a + 4 * k as u32, w);
            }
        }
        let mut ch = Channel::default();
        ch.write(0x00, 0x2000_4160);
        ch.write(0x08, 0x424222);
        ch.write(0x0C, 1);
        ch.write(0x08, 0x424223);
        let mut port = Pipe::default();
        assert_eq!(
            ch.pump(&mut ram, &mut port),
            Events::default(),
            "no data yet"
        );
        port.0.extend([0x2000_41A4, 0x55]);
        let ev = ch.pump(&mut ram, &mut port);
        // CFG 0x424223 has TRIG (bit 22) but INT (bits 21:20) = 0: a trigger, no interrupt
        assert!(ev.trig && !ev.done);
        assert_eq!(ram.0[&0x2000_418C], 0x55, "second word overwrote the first");
        assert_eq!(
            ch.dscptr_cur, 0x2000_4178,
            "third descriptor fetched, waiting for data"
        );
        assert!(ch.enabled());
        // with INT set a unit raises IRQDONE, cleared by writing 1 to STAT
        ch.cfg |= 1 << 20;
        port.0.push_back(7);
        assert!(ch.pump(&mut ram, &mut port).done);
        assert_eq!(ch.read(0x30) & 1, 1);
        ch.write(0x30, 1);
        assert_eq!(ch.read(0x30) & 1, 0);
    }

    /// The ORG SPI2 TX channel: waits for a trigger, then sends 3 words from the received address.
    #[test]
    fn tx_waits_for_trigger_then_reads_memory() {
        let mut ram = Ram::default();
        ram.0.insert(0x2000_4188, 0x2000_4188); // next = itself
                                                // reply words at 0x20004184; the third one is the descriptor's ADDRSTART field itself
        for (k, w) in [0xCAFE_BABEu32, 0x2000_4188, 0x2000_4184]
            .into_iter()
            .enumerate()
        {
            ram.0.insert(0x2000_4184 + 4 * k as u32, w);
        }
        let mut ch = Channel::default();
        ch.write(0x00, 0x2000_4188);
        ch.write(0x08, 0x1C220);
        ch.write(0x0C, 3);
        ch.write(0x10, 4);
        ch.write(0x08, 0x1C221);
        let mut port = Pipe::default();
        ch.pump(&mut ram, &mut port);
        assert!(port.1.is_empty(), "TWAIT holds the unit");
        ch.trigger();
        ch.pump(&mut ram, &mut port);
        assert_eq!(port.1, vec![0xCAFE_BABE, 0x2000_4188, 0x2000_4184]);
    }

    /// 2D unit from the ORG audio lists: X 2 words stride 0xA0, Y 3 rows, YMOD -0x9C.
    #[test]
    fn two_dimensional_addressing() {
        let mut ram = Ram::default();
        let mut ch = Channel::default();
        ch.write(0x04, 0x1000);
        ch.write(0x0C, 2);
        ch.write(0x10, 0xA0);
        ch.write(0x14, 3);
        ch.write(0x18, (-0x9Ci32) as u32);
        ch.write(0x08, TWOD | WNR | 0x200 | EN | 1 << 20);
        let mut port = Pipe((1..=6).collect(), vec![]);
        let ev = ch.pump(&mut ram, &mut port);
        assert!(ev.done);
        let at: Vec<u32> = [0x1000, 0x10A0, 0x1004, 0x10A4, 0x1008, 0x10A8]
            .iter()
            .map(|a| ram.0[a])
            .collect();
        assert_eq!(at, vec![1, 2, 3, 4, 5, 6]);
        assert!(!ch.enabled(), "flow STOP ends the channel");
    }

    #[test]
    fn narrow_memory_words_merge() {
        let mut ram = Ram::default();
        write_n(&mut ram, 0x101, 1, 0xAB);
        write_n(&mut ram, 0x102, 2, 0xCDEF);
        assert_eq!(ram.0[&0x100], 0xCDEF_AB00);
        assert_eq!(read_n(&mut ram, 0x102, 2), 0xCDEF);
        assert_eq!(read_n(&mut ram, 0x101, 1), 0xAB);
    }
}
