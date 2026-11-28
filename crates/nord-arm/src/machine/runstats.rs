//! Work counters of the run loops (`ns4-lle2` tail follow-up). Plain `u64` increments at block
//! granularity (never per instruction), so they stay on in every run; they count work, not time,
//! and are therefore deterministic for a given program and budget.

/// Counters of [`super::Machine::run`] and the cached loop.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunStats {
    /// Calls of the cached loop (`run_cached`).
    pub cached_entries: u64,
    /// Straight-line stretches executed by the cached loop.
    pub blocks: u64,
    /// Block enders that were B / BL taken or not (handled inline).
    pub enders_branch: u64,
    /// Block enders that went through `exec_decoded` (loads into pc, BX, SLOW, UNCOND ...).
    pub enders_full: u64,
    /// Cached loop exits because a store made decoded code stale.
    pub stale_exits: u64,
    /// Jumps into another decoded page followed inside the loop.
    pub page_follows: u64,
    /// Instructions run through `Cpu::step` by `Machine::run` (Thumb, hooked page, misaligned,
    /// cache off).
    pub plain_steps: u64,
    /// Code hooks (stubs, null-call guard) executed.
    pub hook_runs: u64,
}

impl RunStats {
    /// `self - earlier`, field by field (per-tick deltas).
    pub fn since(&self, earlier: &RunStats) -> RunStats {
        RunStats {
            cached_entries: self.cached_entries - earlier.cached_entries,
            blocks: self.blocks - earlier.blocks,
            enders_branch: self.enders_branch - earlier.enders_branch,
            enders_full: self.enders_full - earlier.enders_full,
            stale_exits: self.stale_exits - earlier.stale_exits,
            page_follows: self.page_follows - earlier.page_follows,
            plain_steps: self.plain_steps - earlier.plain_steps,
            hook_runs: self.hook_runs - earlier.hook_runs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Machine;

    #[test]
    fn since_subtracts_every_field() {
        let a = RunStats {
            blocks: 5,
            plain_steps: 2,
            ..Default::default()
        };
        let b = RunStats {
            blocks: 9,
            plain_steps: 2,
            hook_runs: 1,
            ..Default::default()
        };
        let d = b.since(&a);
        assert_eq!((d.blocks, d.plain_steps, d.hook_runs), (4, 0, 1));
    }

    #[test]
    fn a_counting_loop_counts_blocks_and_branch_enders() {
        // mov r0,#0 ; add r0,r0,#1 ; add r0,r0,#1 ; b .-8 (back to the first add)
        let code = [0xE3A0_0000, 0xE280_0001, 0xE280_0001, 0xEAFF_FFFD];
        let mut m = Machine::new();
        for (i, w) in code.iter().enumerate() {
            m.bus.mem.write32(0xC000_0000 + 4 * i as u32, *w);
        }
        m.cpu.r[15] = 0xC000_0000;
        m.run(100, 0, None);
        assert_eq!(m.stats.plain_steps, 0);
        assert!(m.stats.blocks >= 20, "{:?}", m.stats);
        // the block cut by the budget has no ender
        assert!(m.stats.enders_branch + 1 >= m.stats.blocks, "{:?}", m.stats);
        assert!(m.stats.enders_branch <= m.stats.blocks);
        assert_eq!(m.stats.enders_full, 0);
        // the plain path counts steps instead
        let mut p = Machine::new();
        p.predecode = false;
        for (i, w) in code.iter().enumerate() {
            p.bus.mem.write32(0xC000_0000 + 4 * i as u32, *w);
        }
        p.cpu.r[15] = 0xC000_0000;
        p.run(100, 0, None);
        assert_eq!(p.stats.plain_steps, 100);
        assert_eq!(p.stats.blocks, 0);
    }
}
