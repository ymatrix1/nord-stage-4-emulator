//! One processing pass of a Core2-style section: write the start trigger, run until the poll loop
//! is reached again and report which data words changed.
use nord_sharc::core::{Core, Fault};

/// Normal-word range backed by the flat SRAM image (blocks 1 to 3).
pub const SRAM_WORDS: std::ops::Range<u32> = 0x9_0000..0x10_0000;
/// Normal-word range of the memory-mapped IOP registers (peripheral set-up, DMA, serial ports).
pub const IOP_WORDS: std::ops::Range<u32> = 0..0x4_0000;
/// Both ranges, in the order [`snapshot`] stores them.
const WATCHED: [std::ops::Range<u32>; 2] = [SRAM_WORDS, IOP_WORDS];

/// How a pass is started and recognised as finished.
#[derive(Clone, Copy, Debug)]
pub struct PassConfig {
    /// Mailbox word the handler polls (`DM(0xB00E2)` in ORG Core2).
    pub trigger: u32,
    /// Short-word address of the poll loop the handler returns to.
    pub idle_pc: u32,
    /// Instruction limit per pass.
    pub max_steps: u64,
}

/// Why a pass stopped.
#[derive(Debug, PartialEq)]
pub enum PassEnd {
    /// Back in the poll loop with the mailbox cleared.
    Idle,
    /// Hit `max_steps` first.
    StepLimit,
    /// The oracle faulted (unmodelled instruction or bad address).
    Fault(String),
}

/// Result of one pass.
#[derive(Debug)]
pub struct PassReport {
    pub steps: u64,
    pub end: PassEnd,
    /// `(address, old, new)` for every SRAM or IOP data word that differs after the pass.
    pub changed: Vec<(u32, u32, u32)>,
    /// `(pc, address, value)` of every store in the watched window (see `Core::watch`).
    pub writes: Vec<(u32, u32, u32)>,
    /// Every program counter value the pass executed.
    pub visited: std::collections::BTreeSet<u32>,
    /// `(call target, return address)` of every call, in execution order (a call is seen when
    /// the call stack grows; a delayed call is seen after its delay slots).
    pub calls: Vec<(u32, u32)>,
}

/// Copy of all SRAM data words.
pub fn snapshot(core: &mut Core) -> Vec<u32> {
    WATCHED
        .iter()
        .flat_map(|r| r.clone())
        .map(|a| core.dm_read(a).unwrap_or(0))
        .collect()
}

/// Words that differ between `before` and the current memory.
pub fn diff(core: &mut Core, before: &[u32]) -> Vec<(u32, u32, u32)> {
    let mut out = Vec::new();
    for (k, a) in WATCHED.iter().flat_map(|r| r.clone()).enumerate() {
        let now = core.dm_read(a).unwrap_or(0);
        if now != before[k] {
            out.push((a, before[k], now));
        }
    }
    out
}

/// Runs the core for `steps` instructions or until a fault; used for the boot phase.
pub fn run_steps(core: &mut Core, steps: u64) -> Option<Fault> {
    core.run(steps)
}

/// One-line description of a fault.
pub fn fault_text(f: &Fault) -> String {
    match f {
        Fault::Unimplemented { pc, parcels } => {
            format!("unimplemented at 0x{pc:x} (parcels {parcels:04x?})")
        }
        Fault::BadAddress { pc, address } => format!("bad address 0x{address:x} at pc 0x{pc:x}"),
    }
}

/// Starts a pass by writing 1 to the mailbox and runs it to the next poll-loop visit.
pub fn run_pass(core: &mut Core, cfg: &PassConfig) -> PassReport {
    let before = snapshot(core);
    let _ = core.dm_write(cfg.trigger, 1);
    // the host's trigger write is not a core store
    core.watch_hits.clear();
    core.watch_addrs.clear();
    let start = core.steps;
    let mut end = PassEnd::StepLimit;
    let mut visited = std::collections::BTreeSet::new();
    let mut calls = Vec::new();
    while core.steps - start < cfg.max_steps {
        visited.insert(core.pc);
        let depth = core.return_addresses().len();
        if let Err(f) = core.step() {
            end = PassEnd::Fault(fault_text(&f));
            break;
        }
        if core.return_addresses().len() > depth {
            let ret = core.return_addresses().last().copied().unwrap_or(0);
            calls.push((core.pc, ret));
        }
        if core.pc == cfg.idle_pc && core.dm_read(cfg.trigger) == Ok(0) {
            end = PassEnd::Idle;
            break;
        }
    }
    PassReport {
        steps: core.steps - start,
        end,
        changed: diff(core, &before),
        visited,
        calls,
        writes: core
            .watch_hits
            .iter()
            .zip(&core.watch_addrs)
            .map(|(&(_, pc, v), &a)| (pc, a, v))
            .collect(),
    }
}

/// Contiguous ranges `(first, last)` of a set of program counters.
pub fn pc_ranges(visited: &std::collections::BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &pc in visited {
        match out.last_mut() {
            Some((_, last)) if *last + 1 == pc => *last = pc,
            _ => out.push((pc, pc)),
        }
    }
    out
}

/// Store sites: `(pc, stores, lowest address, highest address)`, ordered by pc.
pub fn store_sites(writes: &[(u32, u32, u32)]) -> Vec<(u32, u32, u32, u32)> {
    let mut sites: std::collections::BTreeMap<u32, (u32, u32, u32)> = Default::default();
    for &(pc, a, _) in writes {
        let e = sites.entry(pc).or_insert((0, a, a));
        *e = (e.0 + 1, e.1.min(a), e.2.max(a));
    }
    sites
        .into_iter()
        .map(|(pc, (n, lo, hi))| (pc, n, lo, hi))
        .collect()
}

/// Groups changed words into runs of consecutive addresses: `(first address, new words)`.
pub fn runs(changed: &[(u32, u32, u32)]) -> Vec<(u32, Vec<u32>)> {
    let mut out: Vec<(u32, Vec<u32>)> = Vec::new();
    for &(a, _, new) in changed {
        match out.last_mut() {
            Some((start, words)) if *start + words.len() as u32 == a => words.push(new),
            _ => out.push((a, vec![new])),
        }
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A core with `parcels` at short-word 0x120000 (byte 0x240000) and the pc there.
    pub fn core_with(parcels: &[u16]) -> Core {
        let mut image = vec![0u8; 0x24_0000 + 2 * parcels.len()];
        for (k, p) in parcels.iter().enumerate() {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&p.to_le_bytes());
        }
        let mut c = Core::new(&image);
        c.pc = 0x12_0000;
        c
    }

    /// Type 16b `DM(I0, M0) = 0` (clears the word I0 points at), then NOPs.
    pub fn clearing_core() -> Core {
        let mut c = core_with(&[0x9004, 0x0000, 1, 1, 1]);
        c.i[0] = 0xB_00E2;
        c
    }

    fn cfg(max_steps: u64) -> PassConfig {
        PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0002,
            max_steps,
        }
    }

    #[test]
    fn a_pass_ends_when_the_poll_loop_is_reached_with_the_mailbox_cleared() {
        let mut c = clearing_core();
        let r = run_pass(&mut c, &cfg(10));
        assert_eq!(r.end, PassEnd::Idle, "{r:?}");
        assert_eq!(r.steps, 1);
        // the handler cleared the trigger again, so the net diff is empty
        assert!(r.changed.is_empty(), "{:?}", r.changed);
    }

    #[test]
    fn a_pass_that_never_clears_the_mailbox_hits_the_step_limit() {
        let mut c = core_with(&[1, 1, 1, 1, 1]);
        let r = run_pass(&mut c, &cfg(3));
        assert_eq!((r.end, r.steps), (PassEnd::StepLimit, 3));
        assert_eq!(r.changed, vec![(0xB_00E2, 0, 1)]);
    }

    #[test]
    fn stores_in_the_watch_window_are_logged_per_site() {
        let mut c = clearing_core();
        (c.watch, c.watch_len) = (0xB_0000, 0x100);
        let r = run_pass(&mut c, &cfg(10));
        // the pass start (host write) is not a core store; the handler's clear is
        assert_eq!(r.writes, vec![(0x12_0000, 0xB_00E2, 0)]);
        assert_eq!(
            store_sites(&r.writes),
            vec![(0x12_0000, 1, 0xB_00E2, 0xB_00E2)]
        );
        let multi = store_sites(&[(4, 9, 0), (4, 7, 0), (2, 1, 0)]);
        assert_eq!(multi, vec![(2, 1, 1, 1), (4, 2, 7, 9)]);
    }

    #[test]
    fn visited_pcs_fold_into_ranges() {
        let mut c = clearing_core();
        let r = run_pass(&mut c, &cfg(10));
        // the Type 16b word is 2 parcels; only the instruction start is a pc value
        assert_eq!(pc_ranges(&r.visited), vec![(0x12_0000, 0x12_0000)]);
        assert!(r.calls.is_empty());
        let set: std::collections::BTreeSet<u32> = [1, 2, 3, 7, 9, 10].into();
        assert_eq!(pc_ranges(&set), vec![(1, 3), (7, 7), (9, 10)]);
    }

    #[test]
    fn diff_lists_changed_words_and_groups_runs() {
        let mut c = core_with(&[1]);
        let before = snapshot(&mut c);
        for (a, v) in [(0xB_0000, 1), (0xB_0001, 2), (0xB_0005, 3)] {
            c.dm_write(a, v).unwrap();
        }
        let d = diff(&mut c, &before);
        assert_eq!(d.len(), 3);
        assert_eq!(runs(&d), vec![(0xB_0000, vec![1, 2]), (0xB_0005, vec![3])]);
    }

    #[test]
    fn faults_are_reported_as_text() {
        // an unmodelled Type 10a word right at the start
        let mut c = core_with(&[0xd70a, 0x3c23, 0xd70a]);
        assert!(run_steps(&mut c, 5).is_some());
        let r = run_pass(&mut c, &cfg(5));
        assert!(matches!(r.end, PassEnd::Fault(_)), "{r:?}");
        let t = fault_text(&Fault::BadAddress { pc: 1, address: 2 });
        assert!(t.contains("0x2"), "{t}");
        let t = fault_text(&Fault::Unimplemented {
            pc: 0x10,
            parcels: [1, 2, 3],
        });
        assert!(t.contains("0x10"), "{t}");
    }
}
