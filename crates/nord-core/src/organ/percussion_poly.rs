//! POLY percussion: one decaying envelope per key instead of the single layer-wide one.
//!
//! Manual p.20 (OS v1.6x): percussion is normally "a single-triggered non-legato effect", present
//! only when a key is hit while no other note sounds; in POLY mode (Shift+Volume) the percussion
//! is polyphonic, giving the percussive attack on each new stroke regardless of how many keys are
//! already pressed. Default off (`OrganLayer::percussion_poly == false`), which keeps the
//! single-trigger path bit-identical.
//!
//! The envelope shape and decay times are the existing layer ones (`0.18 s` fast / `0.55 s` slow,
//! ASSUMED, see `docs/specs/organ-engine.md`); poly only changes which keys carry an envelope.

use super::OrganLayer;

/// Per-key percussion envelopes (fixed size, no allocation).
#[derive(Debug, Clone)]
pub(super) struct PolyPercussion {
    env: [f32; 128],
    /// Maximum of `env` (exact after every `decay`); idle frames skip the loop.
    peak: f32,
}

/// Envelope below which a key is treated as silent and zeroed.
const FLOOR: f32 = 1e-6;

impl PolyPercussion {
    pub(super) fn new() -> Self {
        Self {
            env: [0.0; 128],
            peak: 0.0,
        }
    }

    /// Starts the percussive attack of `note` (full level).
    pub(super) fn trigger(&mut self, note: usize) {
        self.env[note.min(127)] = 1.0;
        self.peak = 1.0;
    }

    /// Current envelope of `note`.
    pub(super) fn level(&self, note: usize) -> f32 {
        self.env[note.min(127)]
    }

    /// Multiplies every envelope by `factor` (`< 1`) and returns the new maximum.
    pub(super) fn decay(&mut self, factor: f32) -> f32 {
        if self.peak <= 0.0 {
            return 0.0;
        }
        let mut peak = 0.0f32;
        for e in self.env.iter_mut() {
            *e *= factor;
            if *e < FLOOR {
                *e = 0.0;
            }
            peak = peak.max(*e);
        }
        self.peak = peak;
        peak
    }
}

impl OrganLayer {
    /// Percussion envelope that scales the percussion partial of `note`: the layer-wide
    /// single-trigger envelope, or the key's own envelope in POLY mode.
    pub(super) fn perc_env_for(&self, note: usize) -> f32 {
        if self.percussion_poly {
            self.poly_perc.level(note)
        } else {
            self.percussion_env
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Squared difference of the dry output with and without percussion over 50 ms after a second
    /// note joins a held first one (0.5 s into the slow decay), i.e. the second note's percussion
    /// plus the tail of the first.
    fn percussion_energy(poly: bool) -> f32 {
        let run = |perc: bool| {
            let mut o = OrganLayer::new(48_000.0);
            o.rotary_enabled = false;
            o.key_click_enabled = false;
            o.percussion_enabled = perc;
            o.percussion_poly = poly;
            o.percussion_fast_decay = false;
            o.note_on(60);
            for _ in 0..24_000 {
                o.render_frame();
            }
            o.note_on(67);
            (0..2400).map(|_| o.render_frame()[0]).collect::<Vec<f32>>()
        };
        let (with, without) = (run(true), run(false));
        with.iter()
            .zip(&without)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
    }

    #[test]
    fn only_poly_mode_gives_the_second_note_a_fresh_percussion_attack() {
        let single = percussion_energy(false);
        let poly = percussion_energy(true);
        assert!(single > 0.0 && poly > 0.0);
        assert!(poly > 1.5 * single, "poly {poly} vs single {single}");
    }

    #[test]
    fn poly_envelope_is_per_key() {
        let mut o = OrganLayer::new(48_000.0);
        o.percussion_poly = true;
        o.note_on(60);
        for _ in 0..4800 {
            o.render_frame();
        }
        let first = o.poly_perc.level(60);
        assert!(first < 1.0 && first > 0.0);
        o.note_on(64);
        assert_eq!(o.poly_perc.level(64), 1.0);
        assert_eq!(o.perc_env_for(64), 1.0);
        assert_eq!(o.perc_env_for(60), first);
    }

    #[test]
    fn single_mode_never_touches_the_per_key_state() {
        let mut o = OrganLayer::new(48_000.0);
        o.note_on(60);
        o.note_on(64);
        for _ in 0..1000 {
            o.render_frame();
        }
        assert_eq!(o.poly_perc.level(60), 0.0);
        assert_eq!(o.perc_env_for(64), o.percussion_env);
    }

    #[test]
    fn decay_zeroes_silent_keys_and_reports_peak() {
        let mut p = PolyPercussion::new();
        assert_eq!(p.decay(0.5), 0.0);
        p.trigger(10);
        assert_eq!(p.decay(0.5), 0.5);
        for _ in 0..40 {
            p.decay(0.5);
        }
        assert_eq!(p.level(10), 0.0);
        assert_eq!(p.decay(0.5), 0.0);
    }

    #[test]
    fn poly_with_b3_bass_and_every_model_stays_finite() {
        use super::super::OrganModel;
        for model in [
            OrganModel::B3,
            OrganModel::Vox,
            OrganModel::Farfisa,
            OrganModel::Pipe1,
        ] {
            let mut o = OrganLayer::new(48_000.0);
            o.model = model;
            o.b3_bass_mode = true;
            o.percussion_poly = true;
            o.note_on(36);
            o.note_on(40);
            for _ in 0..4800 {
                let [l, r] = o.render_frame();
                assert!(l.is_finite() && r.is_finite());
            }
        }
    }
}
