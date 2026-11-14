//! Sharing of the legacy (float-phase) tonewheel waveforms between two organ layers.
//!
//! The legacy source evaluates `sin` for 91 wheels (plus 24 harmonics on the B3 bottom octave)
//! every sample. The waveform of wheel `i` is a pure function of the model and the wheel's phase,
//! and the phases of two layers that started together and advanced at the same rates are
//! bit-identical, so the second layer can take the first layer's waveforms instead of repeating
//! every `sin` (perf task TD-14). The check is a bitwise comparison of the 91 phases, so the shared
//! values are exactly the ones the layer would have computed itself.

use super::types::NUM_TONEWHEELS;
use super::OrganModel;

/// Waveforms one layer computed this sample, with the state they were computed from.
#[derive(Debug, Clone, Copy)]
pub struct WheelCache {
    valid: bool,
    model: OrganModel,
    /// Phases the waveforms were evaluated at (before the step to the next sample).
    phases: [f32; NUM_TONEWHEELS],
    raw: [f32; NUM_TONEWHEELS],
}

impl WheelCache {
    /// An empty cache (nothing to share).
    pub(super) fn new() -> Self {
        Self {
            valid: false,
            model: OrganModel::B3,
            phases: [0.0; NUM_TONEWHEELS],
            raw: [0.0; NUM_TONEWHEELS],
        }
    }

    /// Marks the cache as not holding this sample's waveforms.
    #[inline]
    pub(super) fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Records the waveforms `raw` evaluated at `phases` for `model`.
    #[inline]
    pub(super) fn store(
        &mut self,
        model: OrganModel,
        phases: &[f32; NUM_TONEWHEELS],
        raw: &[f32; NUM_TONEWHEELS],
    ) {
        self.valid = true;
        self.model = model;
        self.phases = *phases;
        self.raw = *raw;
    }

    /// The stored waveforms when they were evaluated for `model` at exactly `phases`.
    #[inline]
    pub(super) fn waveforms_for(
        &self,
        model: OrganModel,
        phases: &[f32; NUM_TONEWHEELS],
    ) -> Option<&[f32; NUM_TONEWHEELS]> {
        if self.valid
            && self.model == model
            && self
                .phases
                .iter()
                .zip(phases)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        {
            Some(&self.raw)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::OrganLayer;
    use super::*;

    /// Renders `n` frames of layer `b` three ways (shared with `a`, alone, shared again) and
    /// asserts bit-equality of the shared and the solo output.
    fn assert_sharing_is_invisible(a: &mut OrganLayer, b: &mut OrganLayer, n: usize) {
        let mut solo = b.clone();
        for i in 0..n {
            a.render_frame();
            let shared = b.render_frame_sharing(Some(a.wheel_cache()));
            let alone = solo.render_frame();
            assert_eq!(
                [shared[0].to_bits(), shared[1].to_bits()],
                [alone[0].to_bits(), alone[1].to_bits()],
                "frame {i}"
            );
        }
    }

    #[test]
    fn a_layer_in_lockstep_gets_the_same_output_from_the_shared_waveforms() {
        let mut a = OrganLayer::new(48_000.0);
        let mut b = OrganLayer::new(48_000.0);
        for n in [48, 60, 64, 72] {
            a.note_on(n);
            b.note_on(n);
        }
        assert_sharing_is_invisible(&mut a, &mut b, 3000);
        assert!(
            !b.wheel_cache().valid,
            "B took A's waveforms, it computed none of its own"
        );
        assert!(a.wheel_cache().valid);
    }

    #[test]
    fn diverged_phases_fall_back_to_the_layers_own_waveforms() {
        let mut a = OrganLayer::new(48_000.0);
        let mut b = OrganLayer::new(48_000.0);
        b.bend_ratio = 1.01;
        b.note_on(60);
        a.note_on(60);
        assert_sharing_is_invisible(&mut a, &mut b, 3000);
        assert!(b.wheel_cache().valid, "B computed its own waveforms");
    }

    #[test]
    fn serves_only_the_exact_model_and_phases() {
        let mut c = WheelCache::new();
        let phases = [0.25f32; NUM_TONEWHEELS];
        let raw = [0.5f32; NUM_TONEWHEELS];
        assert!(c.waveforms_for(OrganModel::B3, &phases).is_none());
        c.store(OrganModel::B3, &phases, &raw);
        assert_eq!(c.waveforms_for(OrganModel::B3, &phases), Some(&raw));
        assert!(c.waveforms_for(OrganModel::Vox, &phases).is_none());
        let mut other = phases;
        other[90] = f32::from_bits(other[90].to_bits() + 1);
        assert!(c.waveforms_for(OrganModel::B3, &other).is_none());
        c.invalidate();
        assert!(c.waveforms_for(OrganModel::B3, &phases).is_none());
    }
}
