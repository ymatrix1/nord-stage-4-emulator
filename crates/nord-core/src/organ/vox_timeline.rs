//! Vox key-down weight timeline: the ARM sends the weights of one key in bursts, not at once.
//!
//! Research: `docs/research/keydown-zones.md` section 5 and `docs/research/organ-vox-click.md`.
//! The weight queue of the Vox engine (`0xC0224360`) drains the footage slots of a key at fixed
//! delays after the key event; the captured key-down of note 60 sends slot 0 (16') at tick 3,
//! slots 2 and 5 (8', 4') at tick 17 and slot 10 (2') at tick 47, the key-up zeroes slot 10 at
//! tick 1, slots 2 and 5 at tick 7 and slot 0 at tick 37 (`keydown-organ-keys.csv`, model 1, all
//! 24 notes). This module replays that timeline per key; the weights are instantaneous steps at
//! those ticks, which replaces the legacy key click on this source.

use super::vox_keys::{self, ROWS};

/// Seconds per ARM tick. ASSUMED 1 ms (the tick of the capture script; the ARM timer was not read).
pub(super) const TICK_SECONDS: f32 = 0.001;

/// Delay in ticks after key-down before slot `slot` is sent.
///
/// VERIFIED for slots 0, 2, 5, 10 (24 notes, `keydown-organ-keys.csv`: 3, 17, 17, 47). ASSUMED
/// for the other slots, which are zero in the default program: slots 0..1 at 3, 2..5 at 17, the
/// rest at 47 (the queue drains in slot order).
pub(super) fn on_delay(slot: usize) -> u32 {
    match slot {
        0..=1 => 3,
        2..=5 => 17,
        _ => 47,
    }
}

/// Delay in ticks after key-up before slot `slot` is zeroed (VERIFIED for slots 0, 2, 5, 10:
/// 37, 7, 7, 1; the other slots follow the same grouping, ASSUMED).
pub(super) fn off_delay(slot: usize) -> u32 {
    match slot {
        0..=1 => 37,
        2..=5 => 7,
        _ => 1,
    }
}

/// Per-key age tracking and the resulting row weights.
#[derive(Debug, Clone)]
pub struct VoxTimeline {
    held: [bool; 128],
    /// Samples since the last key event of the key.
    age: [u32; 128],
    /// Key has weights on or pending (held, or released inside its off delays).
    live: [bool; 128],
    weights: [f32; ROWS],
    countdown: u32,
}

/// Samples between weight recomputations (0.2 ms at 48 kHz, far below the 1 ms tick).
const REFRESH: u32 = 8;

impl Default for VoxTimeline {
    fn default() -> Self {
        Self {
            held: [false; 128],
            age: [0; 128],
            live: [false; 128],
            weights: [0.0; ROWS],
            countdown: 0,
        }
    }
}

impl VoxTimeline {
    /// Advances one sample for the held keys (`shift` semitones of octave shift) and returns the
    /// current row weights.
    pub fn step(
        &mut self,
        held: &[bool; 128],
        shift: i32,
        levels: &[f32; 20],
        fs: f32,
    ) -> &[f32; ROWS] {
        let mut changed = false;
        for (n, &h) in held.iter().enumerate() {
            if h != self.held[n] {
                self.held[n] = h;
                self.age[n] = 0;
                self.live[n] = true;
                changed = true;
            } else {
                self.age[n] = self.age[n].saturating_add(1);
            }
        }
        if changed || self.countdown == 0 {
            self.refresh(shift, levels, fs);
            self.countdown = REFRESH;
        }
        self.countdown -= 1;
        &self.weights
    }

    fn refresh(&mut self, shift: i32, levels: &[f32; 20], fs: f32) {
        self.weights.fill(0.0);
        let ticks = |age: u32| (age as f32 / (TICK_SECONDS * fs)) as u32;
        for n in 0..128 {
            if !self.live[n] {
                continue;
            }
            let t = ticks(self.age[n]);
            let mut any = false;
            for (slot, &level) in levels.iter().enumerate().filter(|(_, &l)| l != 0.0) {
                let on = if self.held[n] {
                    t >= on_delay(slot)
                } else {
                    t < off_delay(slot)
                };
                any |= self.held[n] || t < off_delay(slot);
                if on {
                    self.weights[vox_keys::slot_row(n as i32 + shift, slot)] += level;
                }
            }
            if !any {
                self.live[n] = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn run(t: &mut VoxTimeline, held: &[bool; 128], samples: usize) -> [f32; ROWS] {
        let levels = vox_keys::default_levels();
        let mut w = [0.0; ROWS];
        for _ in 0..samples {
            w = *t.step(held, 0, &levels, FS);
        }
        w
    }

    #[test]
    fn key_down_sends_the_footages_at_ticks_3_17_17_47() {
        let mut t = VoxTimeline::default();
        let mut held = [false; 128];
        held[60] = true;
        let ms = |m: f32| (m * FS / 1000.0) as usize;
        let at = |t: &mut VoxTimeline, extra: usize| run(t, &held, extra);
        let w = at(&mut t, ms(1.0));
        assert_eq!(w.iter().filter(|&&x| x != 0.0).count(), 0);
        let w = at(&mut t, ms(3.3) - ms(1.0));
        assert!(
            w[24] > 0.0 && w[36] == 0.0 && w[60] == 0.0,
            "16' only at 3 ms"
        );
        let w = at(&mut t, ms(17.3) - ms(3.3));
        assert!(
            w[36] > 0.0 && w[48] > 0.0 && w[60] == 0.0,
            "8' and 4' at 17 ms"
        );
        let w = at(&mut t, ms(47.3) - ms(17.3));
        assert!(w[60] > 0.0, "2' at 47 ms");
    }

    #[test]
    fn key_up_zeroes_in_the_reverse_order_and_the_key_dies() {
        let mut t = VoxTimeline::default();
        let mut held = [false; 128];
        held[60] = true;
        run(&mut t, &held, FS as usize / 10);
        held[60] = false;
        let ms = |m: f32| (m * FS / 1000.0) as usize;
        let w = run(&mut t, &held, ms(2.0));
        assert!(w[60] == 0.0 && w[48] > 0.0 && w[24] > 0.0, "2' first");
        let w = run(&mut t, &held, ms(8.0) - ms(2.0));
        assert!(
            w[48] == 0.0 && w[36] == 0.0 && w[24] > 0.0,
            "8' and 4' next"
        );
        let w = run(&mut t, &held, ms(40.0));
        assert!(w.iter().all(|&x| x == 0.0), "16' last");
        assert!(!t.live[60]);
    }

    #[test]
    fn rows_that_fold_together_add() {
        let mut t = VoxTimeline::default();
        let mut held = [false; 128];
        held[60] = true;
        held[72] = true; // 16' of C5 is the 8' of C4
        let w = run(&mut t, &held, FS as usize / 10);
        assert!((w[36] - 2.0 * vox_keys::SLOT_LEVEL).abs() < 1e-6);
    }
}
