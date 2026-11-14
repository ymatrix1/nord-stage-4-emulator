//! MST CLK tap tempo / KBS (manual p.40) and TRANSP ON/SET.

use super::model::KbsMode;

/// A gap longer than this restarts the tap sequence.
///
/// ASSUMED: 2 s; the manual only says to tap at least four times (p.40).
pub const TAP_RESET_MS: u32 = 2000;
/// Taps needed before a tempo is derived (manual p.40: "a minimum of four times").
pub const MIN_TAPS: usize = 4;
/// Tempo range of the engine clock.
pub const BPM_RANGE: (f32, f32) = (30.0, 300.0);

/// Master Clock panel state.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockUi {
    pub bpm: f32,
    pub kbs: KbsMode,
    /// Beat `0..4` of the engine clock (the lit box of the Master Clock popup).
    pub beat: u8,
    last_step: Option<u8>,
    taps: [u32; 8],
    count: usize,
}

impl Default for ClockUi {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            kbs: KbsMode::Off,
            beat: 0,
            last_step: None,
            taps: [0; 8],
            count: 0,
        }
    }
}

impl ClockUi {
    /// A TAP at `now_ms`. Returns the new tempo once [`MIN_TAPS`] taps are in.
    pub fn tap(&mut self, now_ms: u32) -> Option<f32> {
        if self.count > 0 && now_ms.saturating_sub(self.taps[self.count - 1]) > TAP_RESET_MS {
            self.count = 0;
        }
        if self.count == self.taps.len() {
            self.taps.copy_within(1.., 0);
            self.count -= 1;
        }
        self.taps[self.count] = now_ms;
        self.count += 1;
        if self.count < MIN_TAPS {
            return None;
        }
        let span = self.taps[self.count - 1].saturating_sub(self.taps[0]) as f32;
        let avg = span / (self.count - 1) as f32;
        if avg <= 0.0 {
            return None;
        }
        self.bpm = (60_000.0 / avg).clamp(BPM_RANGE.0, BPM_RANGE.1);
        Some(self.bpm)
    }

    /// Follows the engine clock: `step` is the sixteenth `0..=3` inside the current beat
    /// (`Telemetry::arp_step`); a step that falls back (3 to 0) is the next beat. `None` (no
    /// engine) keeps the beat.
    ///
    /// ASSUMED: the real unit counts beats of a bar (four boxes); the engine publishes no beat
    /// counter, so the beat is counted here from the wraps of the step, which a frame rate above
    /// 20 Hz sees at every tempo of 30 to 300 BPM.
    pub fn observe_step(&mut self, step: Option<u8>) {
        if let (Some(last), Some(now)) = (self.last_step, step) {
            if now < last {
                self.beat = (self.beat + 1) % 4;
            }
        }
        self.last_step = step.or(self.last_step);
    }

    /// Dial turned by `delta` steps of 1 BPM.
    pub fn dial(&mut self, delta: i32) -> f32 {
        self.bpm = (self.bpm + delta as f32)
            .round()
            .clamp(BPM_RANGE.0, BPM_RANGE.1);
        self.bpm
    }
}

/// TRANSP ON/SET state (manual p.40): +/-6 semitones, per program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransposeUi {
    pub on: bool,
    pub semitones: i8,
}

impl TransposeUi {
    /// Semitones the engine should use (0 while off).
    pub fn effective(&self) -> i8 {
        if self.on {
            self.semitones
        } else {
            0
        }
    }

    /// Press: switch on/off. Switching on with 0 semitones keeps the LED lit, as hardware does.
    pub fn toggle(&mut self) {
        self.on = !self.on;
    }

    /// Dial while the set screen is open: also switches the transposition on.
    pub fn dial(&mut self, delta: i32) {
        let r = i32::from(nord_core::engine::TRANSPOSE_RANGE);
        self.semitones = (i32::from(self.semitones) + delta).clamp(-r, r) as i8;
        self.on = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beat_counts_the_wraps_of_the_engine_step() {
        let mut c = ClockUi::default();
        for step in [0, 1, 2, 3, 0, 1] {
            c.observe_step(Some(step));
        }
        assert_eq!(c.beat, 1, "3 to 0 is the next beat");
        c.observe_step(None);
        assert_eq!(c.beat, 1, "no engine keeps the beat");
        for step in [2, 3, 0, 3, 0, 3, 0] {
            c.observe_step(Some(step));
        }
        assert_eq!(c.beat, 0, "four wraps come back to the first box");
    }

    #[test]
    fn four_taps_at_500_ms_make_120_bpm() {
        let mut c = ClockUi::default();
        c.bpm = 90.0;
        assert_eq!(c.tap(0), None);
        assert_eq!(c.tap(500), None);
        assert_eq!(c.tap(1000), None);
        let bpm = c.tap(1500).unwrap();
        assert!((bpm - 120.0).abs() < 0.01, "{bpm}");
    }

    #[test]
    fn a_long_gap_restarts_the_sequence() {
        let mut c = ClockUi::default();
        for t in [0, 400, 800] {
            c.tap(t);
        }
        assert_eq!(
            c.tap(800 + TAP_RESET_MS + 1),
            None,
            "restarted: one tap only"
        );
    }

    #[test]
    fn tempo_is_clamped_and_taps_keep_refining() {
        let mut c = ClockUi::default();
        for i in 0..12 {
            c.tap(i * 100); // 600 bpm requested
        }
        assert_eq!(c.bpm, BPM_RANGE.1);
        assert_eq!(c.dial(-1000), BPM_RANGE.0);
        assert_eq!(c.dial(1), 31.0);
    }

    #[test]
    fn transpose_toggles_and_clamps_to_six() {
        let mut t = TransposeUi::default();
        assert_eq!(t.effective(), 0);
        t.dial(20);
        assert_eq!((t.semitones, t.on), (6, true));
        assert_eq!(t.effective(), 6);
        t.toggle();
        assert_eq!(t.effective(), 0);
        t.dial(-50);
        assert_eq!(t.semitones, -6);
    }
}
