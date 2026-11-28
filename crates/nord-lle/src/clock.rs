//! Virtual time: who owns time in the engine.
//!
//! One ARM timer tick is 1/24,000 s (D130). One DSP pass period (one bus tick) is two audio
//! frames at `F = 1,388,875 / 32` Hz (`dsp-frame-rate.md`), so passes per tick are
//! `1,388,875 / 1,536,000`. The passes owed after `t` ticks are `floor(t * NUM / DEN)`: integer
//! arithmetic only, so equal inputs give equal pass schedules on every host.

/// Numerator of passes per ARM tick. VERIFIED: arithmetic of `tools/arm_dsp_link.py`
/// (`PASS_NUM`), same check as [`passes_due`] test `matches_the_python_harness_values`.
pub const PASS_NUM: u64 = 1_388_875;
/// Denominator of passes per ARM tick (`tools/arm_dsp_link.py` `PASS_DEN`).
pub const PASS_DEN: u64 = 1_536_000;

/// Passes elapsed after `ticks` ARM timer ticks.
pub fn passes_due(ticks: u64) -> u64 {
    ticks * PASS_NUM / PASS_DEN
}

/// Counts ARM ticks and the passes already run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub ticks: u64,
    pub passes: u64,
}

impl Clock {
    /// Advances one ARM tick; returns the DSP passes now owed.
    pub fn tick(&mut self) -> u64 {
        self.ticks += 1;
        let owed = passes_due(self.ticks) - self.passes;
        self.passes += owed;
        owed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_python_harness_values() {
        // 1 tick: 0 passes (0.9042), 2 ticks: 1, 24,000 ticks (1 s): 21,701 passes
        assert_eq!(passes_due(1), 0);
        assert_eq!(passes_due(2), 1);
        assert_eq!(passes_due(24_000), 21_701);
    }

    #[test]
    fn ticks_hand_out_passes_without_drift() {
        let mut c = Clock::default();
        let total: u64 = (0..24_000).map(|_| c.tick()).sum();
        assert_eq!((total, c.passes, c.ticks), (21_701, 21_701, 24_000));
        assert!(
            (0..100).all(|_| c.tick() <= 1),
            "never more than one pass per tick"
        );
    }
}
