//! Output lanes and reports of `sharc-pass`: the 12 words of an output pointer set, level
//! statistics and the aggregated store-site list of `--watch`.
use nord_sharc::core::Core;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The 12 words a pass writes through the pointer stored at `ptr_word`: three pairs of
/// `[L frame 0, R frame 0, L frame 1, R frame 1]` (SIMD register pairs R/S of `DM(0, I)` and
/// `DM(2, I)`; the same order as the `(out_x, out_y)` frames of `sharc-organ`).
pub fn lane_words(core: &mut Core, ptr_word: u32) -> [u32; 12] {
    let base = core.dm_read(ptr_word).unwrap_or(0);
    let mut w = [0u32; 12];
    for (k, slot) in w.iter_mut().enumerate() {
        *slot = core.dm_read(base.wrapping_add(k as u32)).unwrap_or(0);
    }
    w
}

/// Peak and rms of the lanes written to the WAV.
#[derive(Default)]
pub struct Stats {
    peak: f32,
    sum_sq: f64,
    count: usize,
}

impl Stats {
    pub fn add(&mut self, samples: &[f32]) {
        for &s in samples.iter().filter(|s| s.is_finite()) {
            self.peak = self.peak.max(s.abs());
            self.sum_sq += f64::from(s) * f64::from(s);
            self.count += 1;
        }
    }

    pub fn text(&self, frames: usize) -> String {
        let rms = if self.count == 0 {
            0.0
        } else {
            (self.sum_sq / self.count as f64).sqrt()
        };
        format!("lane: {frames} frames, peak {:.6}, rms {rms:.6}", self.peak)
    }
}

/// Store sites over all passes: `pc -> (stores, first pass, last pass, lowest, highest address)`.
#[derive(Default)]
pub struct Writers {
    sites: BTreeMap<u32, (u32, u32, u32, u32, u32)>,
}

impl Writers {
    /// Adds the `(pc, stores, lo, hi)` sites of pass `pass`.
    pub fn add(&mut self, pass: u32, sites: &[(u32, u32, u32, u32)]) {
        for &(pc, n, lo, hi) in sites {
            let e = self.sites.entry(pc).or_insert((0, pass, pass, lo, hi));
            *e = (e.0 + n, e.1, pass, e.3.min(lo), e.4.max(hi));
        }
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for (pc, (n, first, last, lo, hi)) in &self.sites {
            let _ = writeln!(
                s,
                "writer pc 0x{pc:x}: {n} stores, passes {first}..{last}, DM(0x{lo:x})..DM(0x{hi:x})"
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_words_follow_the_pointer() {
        let mut core = Core::new(&vec![0u8; 0x24_0000 + 32]);
        core.dm_write(0xB_0159, 0xB_1000).unwrap();
        for k in 0..12 {
            core.dm_write(0xB_1000 + k, 100 + k).unwrap();
        }
        let w = lane_words(&mut core, 0xB_0159);
        assert_eq!((w[0], w[3], w[11]), (100, 103, 111));
    }

    #[test]
    fn stats_ignore_non_finite_samples() {
        let mut s = Stats::default();
        s.add(&[0.5, -1.0, f32::NAN, f32::INFINITY]);
        let t = s.text(4);
        assert!(
            t.contains("peak 1.000000") && t.contains("rms 0.790569"),
            "{t}"
        );
        assert!(Stats::default().text(0).contains("rms 0.000000"));
    }

    #[test]
    fn writers_merge_sites_over_passes() {
        let mut w = Writers::default();
        w.add(0, &[(0x10, 2, 5, 6)]);
        w.add(3, &[(0x10, 1, 4, 9), (0x20, 1, 7, 7)]);
        let t = w.text();
        assert!(
            t.contains("pc 0x10: 3 stores, passes 0..3, DM(0x4)..DM(0x9)"),
            "{t}"
        );
        assert!(
            t.contains("pc 0x20: 1 stores, passes 3..3, DM(0x7)..DM(0x7)"),
            "{t}"
        );
    }
}
