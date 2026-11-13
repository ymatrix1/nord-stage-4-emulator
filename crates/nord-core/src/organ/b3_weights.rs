//! Per-wheel weights of the B3 path (`ToneWheelSource::B3Wheels`): `A[wheel] * W(key, footage)`
//! summed over held keys and drawbars, ramped, and the dry sum of the processor `0x121E2B`.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. The processor forms
//! `s = sum over ring notes n of W[n] * ring[n]` with `ring[n] = A[n] * wheel table(phase_n)`
//! and adds `s * DM(0xB44C0)` to the stream (`docs/research/b3-generator.md` section 4). The
//! tables `A` and `W` are the captured key-down values ([`super::b3_keys`]); neither depends on
//! velocity, so held keys have no velocity gain on this path. Real-time safe: fixed arrays.

use super::b3_keys::{fold_wheel, footage_column, key_footage_weights, WHEEL_GAIN};
use super::drawbar::{drawbar_gain_law, DrawbarLaw, DRAWBAR_OFFSETS};
use super::types::NUM_TONEWHEELS;
use super::{OrganLayer, OrganModel, ToneWheelSource};

/// Dry gain of the B3 processor, `DM(0xB44C0)`.
///
/// ASSUMED: 1.0. No ARM message writes it in any capture (`docs/research/organ-farfisa-bass.md`
/// section 1: the B3 is silent at 0 and sounds at 1.0). The oracle output at this gain equals
/// `A * W * wheel` times the processor tail's frequency response `G(f)` (`b3_tail.rs`, applied in
/// the wheel tables, measured with the dry gain at 1.0), so the tail table is only valid for this
/// value. The ARM writer is OQ16.
pub const DRY_GAIN: f32 = 1.0;

/// ASSUMED: weights ramp to their targets in 2 ms (the firmware weight writes are instantaneous;
/// the ramp only avoids a click, as in the Vox source).
const RAMP_SECONDS: f32 = 0.002;

/// Weight state of one B3 layer.
#[derive(Debug, Clone)]
pub(super) struct B3Weights {
    target: [f32; NUM_TONEWHEELS],
    current: [f32; NUM_TONEWHEELS],
    delta: [f32; NUM_TONEWHEELS],
    remaining: u32,
    ramp_samples: u32,
    seen_drawbars: [u8; 9],
    seen_octave: i8,
    seen_law: DrawbarLaw,
    dirty: bool,
}

impl B3Weights {
    /// Silent state for engine rate `fs`.
    pub(super) fn new(fs: f32) -> Self {
        Self {
            target: [0.0; NUM_TONEWHEELS],
            current: [0.0; NUM_TONEWHEELS],
            delta: [0.0; NUM_TONEWHEELS],
            remaining: 0,
            ramp_samples: Self::ramp_for(fs),
            seen_drawbars: [0; 9],
            seen_octave: 0,
            seen_law: DrawbarLaw::Firmware,
            dirty: false,
        }
    }

    fn ramp_for(fs: f32) -> u32 {
        (RAMP_SECONDS * fs.max(8000.0)).round().max(1.0) as u32
    }

    /// A key went down or up.
    pub(super) fn mark(&mut self) {
        self.dirty = true;
    }

    /// `true` while a wheel weight is still moving.
    pub(super) fn active(&self) -> bool {
        self.dirty || self.remaining > 0
    }

    /// Targets `A * W` of every wheel for the held keys, drawbars and octave shift.
    pub(super) fn targets(
        held: &[bool; 128],
        drawbars: &[u8; 9],
        octave_shift: i32,
        law: DrawbarLaw,
    ) -> [f32; NUM_TONEWHEELS] {
        let mut t = [0.0f32; NUM_TONEWHEELS];
        for (note, _) in held.iter().enumerate().filter(|(_, &h)| h) {
            let key = note as i32 + octave_shift;
            let w = key_footage_weights(key);
            for (j, (&level, &off)) in drawbars.iter().zip(DRAWBAR_OFFSETS.iter()).enumerate() {
                if level == 0 {
                    continue;
                }
                let wheel = fold_wheel(key - 24 + off);
                t[wheel] +=
                    WHEEL_GAIN[wheel - 12] * w[footage_column(j)] * drawbar_gain_law(level, law);
            }
        }
        t
    }

    /// Advances the ramp by one sample (retargeting first if keys, drawbars, octave or law
    /// changed) and returns the current weights.
    pub(super) fn step(
        &mut self,
        held: &[bool; 128],
        drawbars: &[u8; 9],
        octave_shift: i32,
        law: DrawbarLaw,
    ) -> &[f32; NUM_TONEWHEELS] {
        let octave = octave_shift as i8;
        if self.dirty
            || *drawbars != self.seen_drawbars
            || octave != self.seen_octave
            || law != self.seen_law
        {
            self.dirty = false;
            self.seen_drawbars = *drawbars;
            self.seen_octave = octave;
            self.seen_law = law;
            self.target = Self::targets(held, drawbars, octave_shift, law);
            let n = self.ramp_samples as f32;
            for ((d, &t), &c) in self.delta.iter_mut().zip(&self.target).zip(&self.current) {
                *d = (t - c) / n;
            }
            self.remaining = self.ramp_samples;
        }
        if self.remaining > 0 {
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            } else {
                for (c, &d) in self.current.iter_mut().zip(&self.delta) {
                    *c += d;
                }
            }
        }
        &self.current
    }

    /// Recomputes the ramp length for a new engine rate.
    pub(super) fn set_rate(&mut self, fs: f32) {
        self.ramp_samples = Self::ramp_for(fs);
    }
}

impl OrganLayer {
    /// `true` when the B3 per-key weight path is the tone source in force (`B3Wheels` on the B3
    /// model, not B3 Bass mode, which is a different processor).
    pub(super) fn b3_weights_active(&self) -> bool {
        self.model == OrganModel::B3
            && !self.b3_bass_mode
            && self.effective_tone_source() == ToneWheelSource::B3Wheels
    }

    /// Dry sum of the B3 weight path over the wheel bus `bus`, including the percussion partial
    /// (4' or 2 2/3' of the held keys with the `A * W(4')` weight, level relative to a full 4'
    /// drawbar as in the legacy percussion gains, ASSUMED).
    pub(super) fn sum_b3_weights(&mut self, bus: &[f32; NUM_TONEWHEELS]) -> f32 {
        let shift = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        self.b3_weights.set_rate(self.sample_rate);
        let w = *self
            .b3_weights
            .step(&self.held_notes, &self.drawbars, shift, self.drawbar_law);
        let mut sum: f32 = w.iter().zip(bus.iter()).map(|(w, x)| w * x).sum();
        if self.percussion_env > 1e-4 {
            let off = if self.percussion_third_harmonic {
                19
            } else {
                12
            };
            let gain = if self.percussion_soft { 0.06 } else { 0.12 } / 0.08;
            for (note, _) in self.held_notes.iter().enumerate().filter(|(_, &h)| h) {
                let key = note as i32 + shift;
                let wheel = fold_wheel(key - 24 + off);
                let a = WHEEL_GAIN[wheel - 12] * key_footage_weights(key)[3];
                sum += self.percussion_env * gain * a * bus[wheel];
            }
        }
        let dip = if self.percussion_enabled && !self.percussion_soft && self.percussion_env > 1e-4
        {
            0.708
        } else {
            1.0
        };
        DRY_GAIN * dip * sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D888: [u8; 9] = [8, 8, 8, 0, 0, 0, 0, 0, 0];

    fn held(notes: &[usize]) -> [bool; 128] {
        let mut h = [false; 128];
        for &n in notes {
            h[n] = true;
        }
        h
    }

    #[test]
    fn single_key_weights_are_a_times_w_on_the_footage_wheels() {
        // key 60, 16' 5 1/3' 8': wheels 24 (16'), 43, 36 with the captured A and W (VERIFIED rows)
        let t = B3Weights::targets(&held(&[60]), &D888, 0, DrawbarLaw::Firmware);
        let want = |wheel: usize, w: f32| WHEEL_GAIN[wheel - 12] * w;
        assert!((t[24] - want(24, 0.031733)).abs() < 1e-7);
        assert!((t[43] - want(43, 0.036577)).abs() < 1e-7);
        assert!((t[36] - want(36, 0.032944)).abs() < 1e-7);
        assert_eq!(t.iter().filter(|&&x| x != 0.0).count(), 3);
        // the 8' amplitude of C4: 0.294 * 0.032944 = 0.0097
        assert!((t[36] - 0.009_685).abs() < 2e-5, "{}", t[36]);
    }

    #[test]
    fn chords_add_on_shared_wheels_and_drawbar_levels_scale_by_the_curve() {
        // C4 5 1/3' (note 67) and G4 8' (note 67) share wheel 43
        let one = B3Weights::targets(&held(&[60]), &D888, 0, DrawbarLaw::Firmware);
        let two = B3Weights::targets(&held(&[60, 67]), &D888, 0, DrawbarLaw::Firmware);
        let g = B3Weights::targets(&held(&[67]), &D888, 0, DrawbarLaw::Firmware);
        assert!((two[43] - one[43] - g[43]).abs() < 1e-7);
        // drawbar 4 of the 8' bar: curve 22 / 127
        let half = B3Weights::targets(
            &held(&[60]),
            &[0, 0, 4, 0, 0, 0, 0, 0, 0],
            0,
            DrawbarLaw::Firmware,
        );
        let full = B3Weights::targets(
            &held(&[60]),
            &[0, 0, 8, 0, 0, 0, 0, 0, 0],
            0,
            DrawbarLaw::Firmware,
        );
        assert!((half[36] / full[36] - 22.0 / 127.0).abs() < 1e-5);
    }

    #[test]
    fn keyboard_ends_fold_footages_onto_wheels_12_to_88() {
        let t = B3Weights::targets(&held(&[24]), &[8; 9], 0, DrawbarLaw::Firmware);
        assert!(t[..12].iter().all(|&x| x == 0.0));
        assert!(t[89..].iter().all(|&x| x == 0.0));
        assert!(t[12] > 0.0);
    }

    #[test]
    fn ramp_reaches_the_target_in_two_milliseconds_and_releases_to_zero() {
        let fs = 48_000.0;
        let mut b = B3Weights::new(fs);
        let mut h = held(&[60]);
        b.mark();
        let first = b.step(&h, &D888, 0, DrawbarLaw::Firmware)[36];
        let tgt = B3Weights::targets(&h, &D888, 0, DrawbarLaw::Firmware)[36];
        assert!(first > 0.0 && first < tgt / 50.0);
        for _ in 0..200 {
            b.step(&h, &D888, 0, DrawbarLaw::Firmware);
        }
        assert_eq!(b.current[36], tgt);
        assert!(!b.active());
        h[60] = false;
        b.mark();
        for _ in 0..200 {
            b.step(&h, &D888, 0, DrawbarLaw::Firmware);
        }
        assert_eq!(b.current, [0.0; NUM_TONEWHEELS]);
        assert!(!b.active());
    }
}
