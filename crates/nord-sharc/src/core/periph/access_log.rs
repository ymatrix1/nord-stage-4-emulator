//! Census of the data accesses outside the SRAM windows: every register or device word a core
//! reads or writes, with counts and the last value. Used by `sharc-sys --census` and
//! `sharc-pass --census` to list the peripheral registers of one pass
//! (`docs/research/dsp-peripherals-audio.md` section 2). Pure bookkeeping, no model behaviour.
use std::collections::BTreeMap;

/// Reads, writes and last values of one address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Access {
    pub reads: u64,
    pub writes: u64,
    /// Last value written (`None` until the first write).
    pub last_write: Option<u32>,
    /// Last value a read returned.
    pub last_read: Option<u32>,
    /// Program counter of the last access.
    pub last_pc: u32,
}

/// Per-address access counts, optionally per pass window (see [`AccessLog::mark`]).
#[derive(Default, Debug)]
pub struct AccessLog {
    map: BTreeMap<u32, Access>,
    /// Counts at the last [`AccessLog::mark`], to report the accesses of one pass.
    marked: BTreeMap<u32, (u64, u64)>,
}

impl AccessLog {
    /// Records a read of `addr` that returned `value`.
    pub fn read(&mut self, addr: u32, value: u32, pc: u32) {
        let a = self.map.entry(addr).or_default();
        a.reads += 1;
        a.last_read = Some(value);
        a.last_pc = pc;
    }

    /// Records a write of `value` to `addr`.
    pub fn write(&mut self, addr: u32, value: u32, pc: u32) {
        let a = self.map.entry(addr).or_default();
        a.writes += 1;
        a.last_write = Some(value);
        a.last_pc = pc;
    }

    /// Remembers the current counts; [`AccessLog::since_mark`] then reports the difference.
    pub fn mark(&mut self) {
        self.marked = self
            .map
            .iter()
            .map(|(k, a)| (*k, (a.reads, a.writes)))
            .collect();
    }

    /// `(address, reads, writes)` since the last [`AccessLog::mark`], ascending, non-zero only.
    pub fn since_mark(&self) -> Vec<(u32, u64, u64)> {
        self.map
            .iter()
            .filter_map(|(k, a)| {
                let (r0, w0) = self.marked.get(k).copied().unwrap_or((0, 0));
                let (r, w) = (a.reads - r0, a.writes - w0);
                (r + w > 0).then_some((*k, r, w))
            })
            .collect()
    }

    /// All recorded addresses with their totals, ascending.
    pub fn entries(&self) -> impl Iterator<Item = (u32, &Access)> {
        self.map.iter().map(|(k, a)| (*k, a))
    }

    /// Text table of every address: counts, last values and last pc.
    pub fn table(&self) -> String {
        let mut out =
            String::from("address      reads    writes  last write  last read   last pc\n");
        for (addr, a) in self.entries() {
            let hex = |v: Option<u32>| v.map_or("-".to_string(), |v| format!("0x{v:08x}"));
            out.push_str(&format!(
                "0x{addr:08x} {:8} {:9}  {:>10}  {:>10}  0x{:06x}\n",
                a.reads,
                a.writes,
                hex(a.last_write),
                hex(a.last_read),
                a.last_pc
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_last_values_and_marks_per_pass() {
        let mut l = AccessLog::default();
        l.read(0x3E021, 1, 0x12_717C);
        l.write(0x3E002, 8, 0x12_714B);
        l.write(0x3E002, 8, 0x12_714B);
        let a = l.entries().find(|(k, _)| *k == 0x3E002).unwrap().1;
        assert_eq!((a.reads, a.writes, a.last_write), (0, 2, Some(8)));
        assert_eq!(l.since_mark(), vec![(0x3E002, 0, 2), (0x3E021, 1, 0)]);
        l.mark();
        assert!(l.since_mark().is_empty());
        l.read(0x3E021, 1, 0);
        assert_eq!(l.since_mark(), vec![(0x3E021, 1, 0)]);
        let t = l.table();
        assert!(t.contains("0x0003e021") && t.contains("0x00000008"), "{t}");
    }
}
