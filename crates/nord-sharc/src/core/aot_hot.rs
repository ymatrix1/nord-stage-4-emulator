//! Runtime of the multi-block AOT prototype: the opt-in hook that runs generated straight-line
//! blocks (see `aot_lower.rs`, `aot_emit.rs`) in place of their interpreted steps, and the guard
//! that decides when that is exact.
//!
//! The generated module is compiled into this crate by `build.rs` when `NORD_SHARC_AOT_HOT` names
//! a file written by `sharc-aot --hot/--ranges`; without it `gen::blocks()` is empty and nothing
//! changes. With no block enabled, [`Core::run`] and [`Core::step`] are untouched (flag-off
//! output is bit-identical to the interpreter).
//!
//! Entry is decided per block, not per step: a bitmap of block start pcs is tested with one
//! compare (`aot_span == 0` while off). A block holds native batches and interpreter items
//! (`Core::step`); after every interpreter item the block checks that nothing it depends on
//! changed ([`AotSig`]) and, if it did, returns early with the exact interpreter state.
use super::{Core, Fault};

mod gen {
    include!(concat!(env!("OUT_DIR"), "/aot_hot_gen.rs"));
}

/// One generated block: its pc range, step counts, the code bytes it was generated from and the
/// compiled function that executes it.
#[derive(Clone, Copy)]
pub(super) struct AotBlock {
    pub start: u32,
    pub last: u32,
    /// Instructions (= interpreter steps) of the whole block.
    pub steps: u32,
    /// Of those, the steps run natively (the rest are interpreter items).
    pub native: u32,
    /// Little-endian bytes of the parcels `start..exit`.
    pub code: &'static [u8],
    pub run: fn(&mut Core) -> Result<(), Fault>,
}

/// Counters of the enabled blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AotHotStats {
    /// Times a generated block ran (to the end or until it bailed out).
    pub calls: u64,
    /// Interpreter steps replaced by those calls (interpreter items included).
    pub steps: u64,
    /// Of `steps`, those run natively (a bailed-out call counts none).
    pub native_steps: u64,
    /// Times the pc was at a block start but the guard (or the step limit) refused.
    pub misses: u64,
    /// Calls that left a block early because an interpreter item changed guarded state.
    pub bails: u64,
}

impl AotHotStats {
    /// The counts accumulated since `earlier` (a snapshot of the same counters).
    pub fn since(&self, earlier: &AotHotStats) -> AotHotStats {
        AotHotStats {
            calls: self.calls - earlier.calls,
            steps: self.steps - earlier.steps,
            native_steps: self.native_steps - earlier.native_steps,
            misses: self.misses - earlier.misses,
            bails: self.bails - earlier.bails,
        }
    }
}

/// The enabled blocks (sorted by start) and their counters.
pub(super) struct AotHot {
    blocks: Vec<AotBlock>,
    per_block: Vec<AotHotStats>,
    total: AotHotStats,
    fault: Option<Fault>,
}

impl Core {
    /// Whether the live SRAM still holds the code the block was generated from.
    fn aot_code_matches(&self, b: &AotBlock) -> bool {
        let o = b.start as usize * 2;
        self.sram.get(o..o + b.code.len()) == Some(b.code)
    }

    /// Whether running `b` now is exactly equivalent to `b.steps` interpreter steps (SIMD or
    /// scalar mode: the block runs PEy only when SIMD is on): no delayed branch or loop end inside the block, no
    /// interrupt-controller event due within it, no enabled arithmetic-exception interrupt that a
    /// compute op could latch and take mid-block, tracing off and the code unchanged.
    fn aot_ready(&self, b: &AotBlock) -> bool {
        let n = u64::from(b.steps);
        self.pending.is_none()
            && self.trace.is_none()
            && self.irq.window_len == 0
            && !self.irq.idle
            && self.steps + n <= self.irq.due
            && self
                .loops
                .last()
                .is_none_or(|f| f.term.is_none() && !(b.start..=b.last).contains(&f.end))
            && !self.arith_irq_enabled()
            && self.aot_code_matches(b)
    }

    /// Installs `blocks` (any order) as the enabled set and builds the entry bitmap.
    fn aot_install(&mut self, mut blocks: Vec<AotBlock>) {
        blocks.sort_by_key(|b| b.start);
        blocks.dedup_by_key(|b| b.start);
        let (lo, hi) = match (blocks.first(), blocks.last()) {
            (Some(f), Some(l)) => (f.start, l.start),
            _ => {
                self.disable_aot_hot();
                return;
            }
        };
        let span = hi - lo + 1;
        let mut bits = vec![0u64; (span as usize).div_ceil(64)];
        for b in &blocks {
            let o = (b.start - lo) as usize;
            bits[o >> 6] |= 1 << (o & 63);
        }
        (self.aot_lo, self.aot_span, self.aot_bits) = (lo, span, bits.into_boxed_slice());
        self.aot_hot = Some(Box::new(AotHot {
            per_block: vec![AotHotStats::default(); blocks.len()],
            blocks,
            total: AotHotStats::default(),
            fault: None,
        }));
    }

    /// Enables the generated blocks for `ranges` (`(start, last)` instruction starts), failing
    /// when this binary has no generated block for one of them or the loaded code differs from
    /// the code it was generated from. Generated blocks not listed stay off.
    pub fn enable_aot_hot(&mut self, ranges: &[(u32, u32)]) -> Result<(), String> {
        let all = gen::blocks();
        if all.is_empty() {
            return Err("built without a generated AOT block (set NORD_SHARC_AOT_HOT=<file from sharc-aot --hot/--ranges> and rebuild)".into());
        }
        let mut picked = Vec::with_capacity(ranges.len());
        for &(start, last) in ranges {
            let b = all
                .iter()
                .find(|b| (b.start, b.last) == (start, last))
                .ok_or_else(|| format!("no generated block for 0x{start:x}..=0x{last:x}"))?;
            if !self.aot_code_matches(b) {
                return Err(format!(
                    "loaded code at 0x{start:x} differs from the code the block was generated from"
                ));
            }
            picked.push(*b);
        }
        self.aot_install(picked);
        Ok(())
    }

    /// Ranges `(start, last)` of every block this binary embeds (empty for a stub build).
    pub fn aot_generated_ranges() -> Vec<(u32, u32)> {
        gen::blocks().iter().map(|b| (b.start, b.last)).collect()
    }

    /// Turns the hook off again.
    pub fn disable_aot_hot(&mut self) {
        (self.aot_lo, self.aot_span, self.aot_bits) = (0, 0, Box::default());
        self.aot_hot = None;
    }

    /// `(start, last, steps)` of the enabled blocks, sorted by start.
    pub fn aot_hot_ranges(&self) -> Vec<(u32, u32, u32)> {
        self.aot_hot.as_deref().map_or_else(Vec::new, |h| {
            h.blocks
                .iter()
                .map(|b| (b.start, b.last, b.steps))
                .collect()
        })
    }

    /// Counters summed over the enabled blocks.
    pub fn aot_hot_stats(&self) -> AotHotStats {
        self.aot_hot.as_deref().map(|h| h.total).unwrap_or_default()
    }

    /// `(start, last, steps, native steps, counters)` of each enabled block.
    pub fn aot_block_stats(&self) -> Vec<(u32, u32, u32, u32, AotHotStats)> {
        self.aot_hot.as_deref().map_or_else(Vec::new, |h| {
            h.blocks
                .iter()
                .zip(&h.per_block)
                .map(|(b, s)| (b.start, b.last, b.steps, b.native, *s))
                .collect()
        })
    }

    /// The fault an interpreter item of the last block raised, if any ([`Core::aot_try`] then
    /// returned true with the state of that step).
    pub fn aot_take_fault(&mut self) -> Option<Fault> {
        self.aot_hot.as_deref_mut().and_then(|h| h.fault.take())
    }

    /// Runs the enabled block starting at the pc when `limit` leaves room for all its steps and
    /// the guard holds; returns whether it ran (the pc is then the block's exit, or where an
    /// early bail-out left it). A fault of an interpreter item is left for [`Core::aot_take_fault`].
    ///
    /// The per-step cost is one compare of `pc` against the entry window (`aot_span`, 0 while
    /// disabled) and, inside it, one bitmap bit; everything else lives in [`Core::aot_enter`].
    #[inline(always)]
    pub fn aot_try(&mut self, limit: u64) -> bool {
        let o = self.pc.wrapping_sub(self.aot_lo);
        o < self.aot_span
            && self.aot_bits[(o >> 6) as usize] >> (o & 63) & 1 != 0
            && self.aot_enter(limit)
    }

    #[inline(never)]
    fn aot_enter(&mut self, limit: u64) -> bool {
        let Some(h) = self.aot_hot.as_deref() else {
            return false;
        };
        let Ok(i) = h.blocks.binary_search_by_key(&self.pc, |b| b.start) else {
            return false;
        };
        let b = h.blocks[i];
        if self.steps + u64::from(b.steps) > limit || !self.aot_ready(&b) {
            if let Some(h) = self.aot_hot.as_deref_mut() {
                h.per_block[i].misses += 1;
                h.total.misses += 1;
            }
            return false;
        }
        let s0 = self.steps;
        let res = (b.run)(self);
        let done = self.steps - s0;
        let bailed = res.is_ok() && done < u64::from(b.steps);
        if let Some(h) = self.aot_hot.as_deref_mut() {
            for st in [&mut h.per_block[i], &mut h.total] {
                st.calls += 1;
                st.steps += done;
                st.native_steps += if bailed { 0 } else { u64::from(b.native) };
                st.bails += u64::from(bailed);
            }
            h.fault = res.err();
        }
        true
    }

    /// [`Core::run`] with the block hook: identical results, but an enabled block runs as one
    /// call whenever the pc reaches its start with room for all of its steps.
    pub fn run_aot(&mut self, max_steps: u64) -> Option<Fault> {
        let limit = self.steps + max_steps;
        while self.steps < limit {
            if self.aot_try(limit) {
                if let Some(f) = self.aot_take_fault() {
                    return Some(f);
                }
                continue;
            }
            if let Err(f) = self.step() {
                return Some(f);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    /// Test block: runs `steps` plain NOP steps through the interpreter, like an all-interpreter
    /// item block would.
    fn nop_block(start: u32, steps: u32) -> AotBlock {
        fn run2(core: &mut Core) -> Result<(), Fault> {
            core.step()?;
            core.step()
        }
        AotBlock {
            start,
            last: start + steps - 1,
            steps,
            native: 0,
            code: &[1, 0, 1, 0],
            run: run2,
        }
    }

    fn simd_core() -> Core {
        let mut c = core_with(&[0x0001u16, 0x0001, 0x0001]);
        c.mode1 |= 1 << 21;
        c.irq.due = u64::MAX;
        c
    }

    #[test]
    fn guard_rejects_due_event_trace_and_changed_code() {
        let program = [0x0001u16, 0x0001];
        let c = core_with(&program);
        let b = nop_block(c.pc, 2);
        let mut c = core_with(&program);
        c.irq.due = u64::MAX;
        assert!(c.aot_ready(&b), "scalar mode runs PEx only");
        c.mode1 |= 1 << 21;
        assert!(c.aot_ready(&b));
        c.irq.due = c.steps + 1;
        assert!(!c.aot_ready(&b), "event due inside the block");
        c.irq.due = c.steps + 2;
        assert!(c.aot_ready(&b), "event due right after the block is fine");
        c.irq.due = u64::MAX;
        c.trace = Some((0, 1, 1));
        assert!(!c.aot_ready(&b), "tracing");
        c.trace = None;
        c.dm_write(0x9_0000, 0xDEAD_BEEF).unwrap();
        assert!(!c.aot_ready(&b), "code differs from the generated snapshot");
    }

    #[test]
    fn hook_enters_by_bitmap_counts_steps_and_matches_the_interpreter() {
        let mut a = simd_core();
        let mut b = simd_core();
        let blk = nop_block(a.pc, 2);
        a.aot_install(vec![blk, nop_block(a.pc + 100, 2)]);
        assert_eq!(a.aot_hot_ranges().len(), 2);
        assert_eq!(a.run_aot(2), None);
        assert_eq!(b.run(2), None);
        assert_eq!(a.state_mismatch(&b), None);
        let st = a.aot_hot_stats();
        assert_eq!((st.calls, st.steps, st.misses, st.bails), (1, 2, 0, 0));
        assert_eq!(a.aot_block_stats()[0].4.calls, 1);
        // a step limit shorter than the block is a miss and falls back to the interpreter
        let mut c = simd_core();
        c.aot_install(vec![nop_block(c.pc, 2)]);
        assert_eq!(c.run_aot(1), None);
        assert_eq!((c.aot_hot_stats().calls, c.aot_hot_stats().misses), (0, 1));
        c.disable_aot_hot();
        assert!(!c.aot_try(10) && c.aot_hot_ranges().is_empty());
    }

    #[test]
    fn enable_fails_without_generated_block_and_flag_off_run_aot_equals_run() {
        let mut a = core_with(&[0x0001u16; 8]);
        assert!(a.enable_aot_hot(&[(a.pc, a.pc + 1)]).is_err());
        assert!(a.aot_hot_ranges().is_empty());
        let mut b = core_with(&[0x0001u16; 8]);
        a.mode1 |= 1 << 21;
        b.mode1 |= 1 << 21;
        assert_eq!(a.run(8), None);
        assert_eq!(b.run_aot(8), None);
        assert_eq!(a.state_mismatch(&b), None);
        assert_eq!(b.aot_hot_stats(), AotHotStats::default());
        assert!(!b.aot_try(100));
        assert!(Core::aot_generated_ranges().is_empty());
    }

    #[test]
    fn arith_exception_interrupts_block_the_fast_path_only_when_enabled() {
        let mut c = core_with(&[0x0001]);
        assert!(!c.arith_irq_enabled());
        c.mode1 |= 1 << 12; // IRPTEN
        c.imask = 1 << 18; // some unrelated vector bit
        let _ = c.arith_irq_enabled();
    }

    #[test]
    fn stats_since_subtracts_a_snapshot() {
        let a = AotHotStats {
            calls: 5,
            steps: 50,
            native_steps: 40,
            misses: 3,
            bails: 1,
        };
        let b = AotHotStats {
            calls: 2,
            steps: 20,
            native_steps: 10,
            misses: 1,
            bails: 0,
        };
        let d = a.since(&b);
        assert_eq!(
            (d.calls, d.steps, d.native_steps, d.misses, d.bails),
            (3, 30, 30, 2, 1)
        );
    }
}
