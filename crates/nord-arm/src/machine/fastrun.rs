//! Predecode cache and the cached run loop (`ns4-lle2` follow-up 1).
//!
//! [`CodeCache`] keeps one decoded page ([`ArmPage`]) per 4 KiB page of guest code that the
//! ARM-state loop has executed. A page is decoded as a whole when it is first run, together with
//! the length of the straight-line run at every word (`Entry::run`). The page is flagged `HOOK_C`
//! in [`crate::mem::Mem`]; a store to it (or a code hook added on it) queues the page in
//! `Mem::stale_code` and the loop drops the decode before it fetches again, so self-modifying code
//! and late hooks stay exact. A page that has a decode is never hooked (hooking drops it).
//!
//! The loop runs each straight-line stretch without looking at `jumped`, faults or the page, then
//! runs the one ender instruction (branch, load into `pc`, anything generic) with the full
//! protocol. Thumb state, hooked pages (stubs, the null-call guard), misaligned `pc` and pages
//! without a decode yet go back to [`Machine::run`], which keeps the plain [`Cpu::step`] path.

use super::Machine;
use crate::cpu::predecode::{build_page, op, ArmPage, ENTRIES, MASK_ALWAYS};
use crate::cpu::Fault;

/// Decoded pages by guest page number.
#[derive(Clone)]
pub struct CodeCache {
    pages: Vec<Option<Box<ArmPage>>>,
    /// Pages decoded (including re-decodes after a change).
    pub decoded_pages: u64,
    /// Pages dropped because their code changed or got a hook.
    pub invalidated: u64,
}

impl Default for CodeCache {
    fn default() -> Self {
        Self {
            pages: vec![None; 1 << 20],
            decoded_pages: 0,
            invalidated: 0,
        }
    }
}

impl CodeCache {
    /// Pages currently holding decoded code.
    pub fn live_pages(&self) -> usize {
        self.pages.iter().filter(|p| p.is_some()).count()
    }

    fn drop_page(&mut self, page: u32) {
        if self.pages[page as usize].take().is_some() {
            self.invalidated += 1;
        }
    }

    fn has(&self, page: u32) -> bool {
        self.pages[page as usize].is_some()
    }
}

impl Machine {
    /// Drop the decode of every page whose code was written or hooked since the last call.
    pub(super) fn drop_stale_code(&mut self) {
        while let Some(p) = self.bus.mem.stale_code.pop() {
            self.code.drop_page(p);
        }
    }

    /// Decode the page of `pc` into the cache (mapping it first: an unmapped fetch maps a zero
    /// page, as on the plain path) and flag it for invalidation.
    fn decode_page(&mut self, page_no: u32) {
        let mut words = [0u32; ENTRIES];
        for (i, w) in words.iter_mut().enumerate() {
            *w = self.bus.mem.read32((page_no << 12) + 4 * i as u32);
        }
        self.bus.mem.mark_code(page_no << 12);
        self.code.pages[page_no as usize] = Some(build_page(&words));
        self.code.decoded_pages += 1;
    }

    /// Execute one ARM instruction at `pc` from the cache (decoding its page if needed); the
    /// fault, if any. Same effect as [`Cpu::step`] (the probe uses it).
    pub(super) fn step_cached(&mut self, pc: u32) -> Option<Fault> {
        self.drop_stale_code();
        if !self.code.has(pc >> 12) {
            self.decode_page(pc >> 12);
        }
        let table = self.code.pages[(pc >> 12) as usize].as_deref().unwrap();
        let e = table[(pc >> 2) as usize & (ENTRIES - 1)];
        self.cpu.insns += 1;
        if self.cpu.exec_decoded(&mut self.bus, e, pc) {
            return self.cpu.fault.take();
        }
        None
    }

    /// Run instructions from the predecode cache while `pc` stays in decoded, unhooked ARM code.
    /// Runs at least one instruction (the caller checked the budget, `until`/`stop`, the hook
    /// flag, Thumb state and `pc` alignment). Stops when the budget (`limit`, a value of
    /// `cpu.insns`) is used up, `pc` reaches `until`/`stop`, leaves decoded code, the CPU
    /// switches to Thumb, a store made decoded code stale, or an instruction faults.
    pub(super) fn run_cached(&mut self, limit: u64, until: u32, stop: u32) -> Option<Fault> {
        self.drop_stale_code();
        let mut page_no = self.cpu.r[15] >> 12;
        if !self.code.has(page_no) {
            self.decode_page(page_no);
        }
        let start = self.cpu.insns;
        let mut left = limit - start;
        let mut table = self.code.pages[page_no as usize].as_deref().unwrap();
        let mut guard = until >> 12 == page_no || stop >> 12 == page_no;
        let (cpu, bus, stats) = (&mut self.cpu, &mut self.bus, &mut self.stats);
        stats.cached_entries += 1;
        let fault = loop {
            stats.blocks += 1;
            let pc = cpu.r[15];
            let i = (pc >> 2) as usize & (ENTRIES - 1);
            // straight-line stretch: nothing in it jumps, faults or leaves the page
            let mut n = (table[i].run as u64).min(left);
            if guard {
                for g in [until, stop] {
                    if g > pc && g >> 12 == page_no {
                        n = n.min(((g - pc + 3) >> 2) as u64);
                    }
                }
            }
            let mut p = pc;
            let mut done = 0;
            while done < n {
                let e = table[i + done as usize];
                bus.pc = p;
                cpu.exec_pure(bus, e, p);
                p = p.wrapping_add(4);
                done += 1;
                if !bus.mem.stale_code.is_empty() {
                    break;
                }
            }
            cpu.r[15] = p;
            left -= done;
            // a store that made decoded code stale ends the run even when it was the last
            // instruction before the ender: the ender must come from the fresh page, not `table`
            if done < n || left == 0 || !bus.mem.stale_code.is_empty() {
                stats.stale_exits += u64::from(!bus.mem.stale_code.is_empty());
                break None;
            }
            if done == table[i].run as u64 {
                // the ender: full protocol (unless it is where the caller wants to stop)
                if guard && (p == until || p == stop) {
                    break None;
                }
                let e = table[(p >> 2) as usize & (ENTRIES - 1)];
                bus.pc = p;
                left -= 1;
                if matches!(e.op, op::B | op::BL) {
                    stats.enders_branch += 1;
                    // the common enders, inline: the target is a constant of the word
                    if e.mask == MASK_ALWAYS || e.mask >> cpu.nzcv() & 1 != 0 {
                        if e.op == op::BL {
                            cpu.r[14] = p.wrapping_add(4);
                        }
                        let off = (((e.insn & 0x00FF_FFFF) << 8) as i32 >> 6) as u32;
                        cpu.r[15] = p.wrapping_add(8).wrapping_add(off) & !3;
                    } else {
                        cpu.r[15] = p.wrapping_add(4);
                    }
                } else {
                    stats.enders_full += 1;
                    if cpu.exec_decoded(bus, e, p) {
                        break cpu.fault.take();
                    }
                }
            }
            let next = cpu.r[15];
            if left == 0 || next & 3 != 0 || cpu.thumb || !bus.mem.stale_code.is_empty() {
                break None;
            }
            if next >> 12 != page_no {
                // follow a jump into another decoded page without leaving the loop
                let target = next >> 12;
                match self.code.pages[target as usize].as_deref() {
                    Some(t) => {
                        stats.page_follows += 1;
                        table = t;
                        page_no = target;
                        guard = until >> 12 == page_no || stop >> 12 == page_no;
                    }
                    None => break None,
                }
            }
            if guard && (next == until || next == stop) {
                break None;
            }
        };
        cpu.insns = limit - left;
        fault
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Stop;

    const CODE: u32 = 0xC000_0000;
    const WORDS: usize = 1100; // crosses a page boundary (1024 words per page)

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 16) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            self.next() % n
        }
    }

    /// One random word from a mix that stays inside the test region: data processing on r0..r7,
    /// loads and stores through r8 (data) and r10 (the code itself: self-modifying code), block
    /// transfers on r9, short branches, a few generic and garbage words.
    fn word(rng: &mut Rng, at: usize) -> u32 {
        let cond = if rng.below(4) == 0 { rng.below(14) } else { 14 } << 28;
        let rd = if rng.below(12) == 0 { 15 } else { rng.below(8) }; // sometimes writes pc
        let (rn, rm) = (rng.below(8), rng.below(8));
        cond | match rng.below(28) {
            20 => 0x050F_0004 | rd << 12 | rng.below(2) << 20, // str/ldr rd,[pc,#-4]: the next word
            21 => {
                0x01C0_00B0
                    | 10 << 16
                    | rd << 12
                    | rng.below(2) << 20
                    | rng.below(2) << 8
                    | rng.below(16)
            } // strh/ldrh [r10,#imm]
            22 => 0x038F_C001,                                 // orr r12,pc,#1 (Thumb target)
            23 => 0x012F_FF1C,                                 // bx r12 (enter Thumb state)
            24 => {
                0x0890_800F | rng.below(2) << 20 | rng.below(2) << 24 | rng.below(2) << 23 | 9 << 16
            } // ldm/stm with pc
            25..=27 => rng.next() & 0x0FFF_FFFF,
            0..=8 => {
                let (op, s) = (rng.below(16), rng.below(2));
                let s = if (8..=11).contains(&op) { 1 } else { s };
                let operand = match rng.below(3) {
                    0 => 1 << 25 | rng.below(0x1000),
                    1 => rm | rng.below(0x100) << 4 & !0x10,
                    _ => rm | 1 << 4 | rng.below(8) << 8 | rng.below(4) << 5,
                };
                op << 21 | s << 20 | rn << 16 | rd << 12 | operand
            }
            9..=11 => {
                let (l, b, base) = (
                    rng.below(2),
                    rng.below(2),
                    if rng.below(5) == 0 { 10 } else { 8 },
                );
                0x0580_0000 | b << 22 | l << 20 | base << 16 | rd << 12 | (rng.below(0x40) * 4)
            }
            12 => {
                0x0890_000F
                    | (rng.below(2) << 20)
                    | (rng.below(2) << 24)
                    | (rng.below(2) << 23)
                    | 9 << 16
            }
            13..=14 => {
                let target = rng.below(WORDS as u32) as i64;
                let off = (target - at as i64 - 2) as u32 & 0x00FF_FFFF;
                0x0A00_0000 | rng.below(2) << 24 | off
            }
            15 => 0x0000_0090 | rd << 16 | rm << 8 | rn, // mul
            16 => 0x012F_FF10 | 14,                      // bx lr
            17 => 0x0100_0090 | rm | rd << 12 | rn << 16, // swp
            _ => rng.next() & 0x0FFF_FFFF,
        }
    }

    fn machine(rng: &mut Rng, predecode: bool, words: &[u32]) -> Machine {
        let mut m = Machine::new();
        m.predecode = predecode;
        for (i, w) in words.iter().enumerate() {
            m.bus.mem.write32(CODE + 4 * i as u32, *w);
        }
        for reg in m.cpu.r.iter_mut().take(8) {
            *reg = rng.next();
        }
        (m.cpu.r[8], m.cpu.r[9], m.cpu.r[10], m.cpu.r[13]) =
            (0x9000_0000, 0x9000_1000, CODE + 0x100, 0x9000_2000);
        m.cpu.r[14] = CODE;
        m.cpu.r[15] = CODE;
        m
    }

    #[test]
    fn the_cached_run_equals_the_plain_run_on_random_programs() {
        // `NS4_FUZZ_CASES` / `NS4_FUZZ_SEED` widen the search (a large run is a manual check)
        let env = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        let mut rng = Rng(0xD1B5_4A32_D192_ED03 ^ env("NS4_FUZZ_SEED", 0));
        for case in 0..env("NS4_FUZZ_CASES", 60) {
            let words: Vec<u32> = (0..WORDS).map(|i| word(&mut rng, i)).collect();
            let seed = rng.next();
            let (mut a, mut b) = (
                machine(&mut Rng(seed as u64 | 1), false, &words),
                machine(&mut Rng(seed as u64 | 1), true, &words),
            );
            let stop = (case % 3 == 0).then(|| CODE + 4 * rng.below(WORDS as u32));
            for slice in 0..40 {
                let budget = 1 + rng.below(if slice % 4 == 0 { 7 } else { 400 }) as u64;
                let (ra, rb) = (a.run(budget, 0, stop), b.run(budget, 0, stop));
                let what = format!("case {case} slice {slice} budget {budget} stop {stop:x?}");
                assert_eq!(ra, rb, "{what}");
                assert_eq!(
                    (a.cpu.r, a.cpu.cpsr(), a.cpu.insns),
                    (b.cpu.r, b.cpu.cpsr(), b.cpu.insns),
                    "{what}"
                );
                if matches!(ra.1, Stop::Fault(_) | Stop::StopPc) {
                    break;
                }
            }
            assert_eq!(
                a.state_hash(),
                b.state_hash(),
                "case {case}: memory differs"
            );
        }
    }

    #[test]
    fn a_store_into_decoded_code_takes_effect_on_the_next_fetch() {
        // 0: add r0,r0,#1 ; 4: str r1,[r2] (r2 -> word at 12) ; 8: add r0,r0,#1 ; c: add r0,r0,#16
        // r1 = mov r0,#99 ; the store replaces word c, then the loop at 10 jumps back to 0
        let code = [
            0xE280_0001,
            0xE582_1000,
            0xE280_0001,
            0xE280_0010,
            0xEAFF_FFFB,
        ];
        let mut m = Machine::new();
        for (i, w) in code.iter().enumerate() {
            m.bus.mem.write32(CODE + 4 * i as u32, *w);
        }
        m.cpu.r[1] = 0xE3A0_0063; // mov r0,#99
        m.cpu.r[2] = CODE + 12;
        m.cpu.r[15] = CODE;
        m.run(3, 0, None); // runs 0, 4, 8: the decoded page is live and word c is rewritten
        assert!(
            m.code.invalidated >= 1,
            "the store made the page stale and it was dropped"
        );
        m.run(1, 0, None);
        assert_eq!(m.cpu.r[0], 99, "the new word ran, not the stale decode");
        assert!(m.code.decoded_pages >= 2, "and decoded the page again");
    }

    #[test]
    fn a_store_into_the_ender_right_after_it_runs_the_new_ender() {
        // 0: str r1,[r2] (r2 -> word 4) ; 4: b . ; the store is the last non-ender before the ender
        let mut results = [(0, 0); 2];
        for (k, predecode) in [false, true].into_iter().enumerate() {
            let mut m = Machine::new();
            m.predecode = predecode;
            m.bus.mem.write32(CODE, 0xE582_1000);
            m.bus.mem.write32(CODE + 4, 0xEAFF_FFFE);
            m.cpu.r[1] = 0xE3A0_0063; // mov r0,#99
            m.cpu.r[2] = CODE + 4;
            m.cpu.r[15] = CODE;
            m.run(2, 0, None);
            results[k] = (m.cpu.r[0], m.cpu.r[15]);
        }
        assert_eq!(
            results[0],
            (99, CODE + 8),
            "the plain path runs the rewritten word"
        );
        assert_eq!(results[1], results[0], "the cached path must match it");
    }

    #[test]
    fn a_hook_added_on_a_decoded_page_drops_it() {
        let mut m = Machine::new();
        m.bus.mem.write32(CODE, 0xE280_0001);
        m.cpu.r[15] = CODE;
        m.run(1, 0, None);
        assert_eq!(m.code.live_pages(), 1);
        m.add_hook(CODE + 4, crate::machine::Hook::Return(7));
        assert_eq!(m.bus.mem.stale_code, vec![CODE >> 12]);
        m.cpu.r[15] = CODE + 4;
        m.cpu.r[14] = 0x1234;
        m.run(1, 0, None);
        assert_eq!(
            m.code.invalidated, 1,
            "the next run dropped the decode of the hooked page"
        );
        assert_eq!(m.cpu.r[0], 7, "the hook ran instead of the cached word");
    }

    #[test]
    fn step_cached_equals_step_and_run_stops_at_until_inside_a_straight_line() {
        let code = [0xE280_0001u32; 8];
        let mut m = Machine::new();
        for (i, w) in code.iter().enumerate() {
            m.bus.mem.write32(CODE + 4 * i as u32, *w);
        }
        m.cpu.r[15] = CODE;
        assert_eq!(m.run(100, CODE + 12, None), (3, Stop::Until));
        assert_eq!(m.cpu.r[0], 3);
        assert_eq!(m.step_cached(CODE + 12), None);
        assert_eq!((m.cpu.r[0], m.cpu.r[15], m.cpu.insns), (4, CODE + 16, 4));
    }
}
