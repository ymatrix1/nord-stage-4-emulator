//! TAP/SET tempo of the Delay (manual p.51): tap in the song's tempo, the delay time follows.
//!
//! ASSUMED: the manual only says "tap a number of times"; this averages up to the last four
//! intervals and starts over after a pause of more than two seconds (longest delay is 1.5 s).

/// Longest gap (ms) that still counts as part of one tapping sequence.
pub const MAX_GAP_MS: u32 = 2000;
/// Intervals averaged.
const WINDOW: usize = 4;

/// Tap tempo detector with caller supplied time.
#[derive(Debug, Clone, Default)]
pub struct TapTempo {
    last: Option<u32>,
    gaps: [u32; WINDOW],
    count: usize,
}

impl TapTempo {
    /// Register a tap at time `t` (ms). Returns the averaged interval once two taps are in.
    pub fn tap(&mut self, t: u32) -> Option<f32> {
        if let Some(prev) = self.last {
            let gap = t.saturating_sub(prev);
            if gap > MAX_GAP_MS || gap == 0 {
                self.count = 0;
            } else {
                self.gaps[self.count % WINDOW] = gap;
                self.count += 1;
            }
        }
        self.last = Some(t);
        let n = self.count.min(WINDOW);
        (n > 0).then(|| self.gaps[..n].iter().sum::<u32>() as f32 / n as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tap_gives_nothing() {
        assert_eq!(TapTempo::default().tap(100), None);
    }

    #[test]
    fn steady_taps_give_their_interval() {
        let mut t = TapTempo::default();
        t.tap(0);
        assert_eq!(t.tap(500), Some(500.0));
        assert_eq!(t.tap(1000), Some(500.0));
        assert_eq!(t.tap(1500), Some(500.0));
    }

    #[test]
    fn average_uses_the_last_four_intervals() {
        let mut t = TapTempo::default();
        let mut time = 0;
        t.tap(time);
        for gap in [1000, 400, 400, 400, 400] {
            time += gap;
            t.tap(time);
        }
        assert_eq!(t.tap(time + 400), Some(400.0));
    }

    #[test]
    fn a_long_pause_starts_a_new_sequence() {
        let mut t = TapTempo::default();
        t.tap(0);
        t.tap(500);
        assert_eq!(t.tap(500 + MAX_GAP_MS + 1), None);
        assert_eq!(t.tap(500 + MAX_GAP_MS + 301), Some(300.0));
    }
}
