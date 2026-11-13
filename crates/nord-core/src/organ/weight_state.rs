//! Per-layer state of [`WeightMode::Firmware`](super::WeightMode): the held keys' velocities, the
//! smoothed wheel weights and the change tracking that tells when the targets must be rebuilt.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.5. Targets are rebuilt only on key, drawbar or
//! octave changes, never per sample; the per-sample work is the 91-wheel one-pole and the mix.

use super::types::NUM_TONEWHEELS;
use super::weights::{targets, wheel_of, WheelWeights};

/// Velocity `1..=127` of a `0.0..=1.0` engine key gain (`gain * 127`, rounded).
///
/// ASSUMED: the engine's note gain is `velocity / 127` (the firmware key-down takes the raw MIDI
/// velocity; `docs/research/organ-weights.md` section 2 leaves the organ velocity source open).
pub fn velocity_for_gain(gain: f32) -> u8 {
    (gain.clamp(0.0, 1.0) * 127.0).round().clamp(1.0, 127.0) as u8
}

/// Weight state of one organ layer.
#[derive(Debug, Clone)]
pub(super) struct WeightState {
    weights: WheelWeights,
    velocity: [u8; 128],
    seen_drawbars: [u8; 9],
    seen_octave: i8,
    events: u32,
    seen_events: u32,
    rate: f32,
}

impl WeightState {
    /// Silent state for engine rate `fs` and the layer's initial drawbars.
    pub(super) fn new(fs: f32, drawbars: [u8; 9]) -> Self {
        Self {
            weights: WheelWeights::new(fs),
            velocity: [0; 128],
            seen_drawbars: drawbars,
            seen_octave: 0,
            events: 0,
            seen_events: 0,
            rate: fs,
        }
    }

    /// A drawbar or octave change is a full update: every wheel moves with `a = 0.0043`.
    fn note_params(&mut self, drawbars: &[u8; 9], octave: i8) {
        if *drawbars != self.seen_drawbars || octave != self.seen_octave {
            self.seen_drawbars = *drawbars;
            self.seen_octave = octave;
            self.weights.clear_fast();
            self.events += 1;
        }
    }

    /// Key-down handler: stores the velocity and marks the key's wheels `a = 0.0096`.
    pub(super) fn key_down(&mut self, note: u8, velocity: u8, drawbars: &[u8; 9], octave: i8) {
        self.note_params(drawbars, octave);
        let idx = usize::from(note.min(127));
        self.velocity[idx] = velocity;
        let midi = idx as i32 + i32::from(octave) * 12;
        for (j, &level) in drawbars.iter().enumerate() {
            if level > 0 {
                self.weights.mark_key_down(wheel_of(midi, j));
            }
        }
        self.events += 1;
    }

    /// Key-up: a full update (assumed, the key-up handler was not read).
    pub(super) fn key_up(&mut self, note: u8) {
        self.velocity[usize::from(note.min(127))] = 0;
        self.weights.clear_fast();
        self.events += 1;
    }

    /// Rebuilds the targets if anything changed, then advances the smoothing by one sample and
    /// returns the smoothed weights.
    pub(super) fn step(
        &mut self,
        held: &[bool; 128],
        drawbars: &[u8; 9],
        octave: i8,
        fs: f32,
    ) -> &[f32; NUM_TONEWHEELS] {
        if fs != self.rate {
            self.rate = fs;
            self.weights.set_rate(fs);
        }
        self.note_params(drawbars, octave);
        if self.events != self.seen_events {
            self.seen_events = self.events;
            let shift = i32::from(octave) * 12;
            let keys = held
                .iter()
                .zip(self.velocity.iter())
                .enumerate()
                .filter(|(_, (&h, _))| h)
                .map(|(n, (_, &v))| (n as i32 + shift, v.max(1)));
            let t = targets(keys, drawbars);
            self.weights.set_targets(&t);
        }
        self.weights.step()
    }

    /// `true` while a wheel weight is still moving (the release tail must keep rendering).
    pub(super) fn active(&self) -> bool {
        self.events != self.seen_events || self.weights.active()
    }

    /// Smoothed weights without advancing.
    #[cfg(test)]
    pub(super) fn weights(&self) -> &WheelWeights {
        &self.weights
    }
}

#[cfg(test)]
mod tests {
    use super::super::weights::{a_key_down, a_update, UNIT_WEIGHT};
    use super::*;

    const D888: [u8; 9] = [8, 8, 8, 0, 0, 0, 0, 0, 0];

    #[test]
    fn velocity_follows_the_gain() {
        assert_eq!(velocity_for_gain(1.0), 127);
        assert_eq!(velocity_for_gain(0.0), 1);
        assert_eq!(velocity_for_gain(0.5), 64);
        assert_eq!(velocity_for_gain(7.0), 127);
    }

    #[test]
    fn key_down_ramps_with_the_fast_coefficient_and_key_up_with_the_slow_one() {
        let fs = 43_156.4;
        let mut s = WeightState::new(fs, D888);
        let mut held = [false; 128];
        held[60] = true;
        s.key_down(60, 127, &D888, 0);
        let first = s.step(&held, &D888, 0, fs)[36];
        let want = (a_key_down() * f64::from(UNIT_WEIGHT)) as f32;
        assert!((first - want).abs() < 1e-8, "{first} vs {want}");
        for _ in 0..40_000 {
            s.step(&held, &D888, 0, fs);
        }
        assert_eq!(s.weights().current()[36], UNIT_WEIGHT);
        held[60] = false;
        s.key_up(60);
        let after = s.step(&held, &D888, 0, fs)[36];
        let slow = (1.0 - a_update()) as f32;
        let want_off = UNIT_WEIGHT * slow;
        assert!((after - want_off).abs() < 1e-8, "{after} vs {want_off}");
        for _ in 0..40_000 {
            s.step(&held, &D888, 0, fs);
        }
        assert_eq!(s.weights().current()[36], 0.0);
    }

    #[test]
    fn drawbar_change_is_a_full_update_and_chords_add() {
        let fs = 43_156.4;
        let mut s = WeightState::new(fs, D888);
        let mut held = [false; 128];
        for n in [60usize, 64, 67] {
            held[n] = true;
            s.key_down(n as u8, 127, &D888, 0);
        }
        for _ in 0..40_000 {
            s.step(&held, &D888, 0, fs);
        }
        let unit = UNIT_WEIGHT;
        assert_eq!(s.weights().current()[43], 2.0 * unit);
        // drawbars to 008000808: wheel 36 (8' of C4) stays, new wheels appear, one step moves with a_update
        let d2 = [0, 0, 8, 0, 0, 0, 0, 8, 8];
        let w43 = s.step(&held, &d2, 0, fs)[43];
        let slow = (1.0 - a_update()) as f32;
        let want = 2.0 * unit * slow + unit * (1.0 - slow);
        assert!(
            (w43 - want).abs() < 2e-8,
            "G4 8' stays, C4 5 1/3' gone: {w43} vs {want}"
        );
    }
}
