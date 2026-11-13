//! Unified 3-Engine, 7-Layer Multitimbral Nord Stage 4 DSP Coordinator (`Stage4Engine`).
//!
//! Spec: `docs/ARCHITECTURE.md`, `docs/specs/effects-and-routing.md`, `docs/specs/morph-and-zones.md`

use crate::fx::{AmpCabinet, DelayFilterMode, LayerFxChain, MasterBus, MasterClock, ReverbColor};
use crate::midi::{cc, MidiEvent};
use crate::morph::{KeybedSplitRouter, LayerId, LayerSceneState, MorphControllers, MorphParameter};
use crate::organ::{OrganLayer, RotarySpeed};
use crate::piano::PianoLayer;
use crate::synth::SynthLayer;

mod midi_dispatch;
mod program;
mod program_fx;
#[cfg(test)]
mod tests_program;
#[cfg(test)]
mod tests_routing;

/// Complete 7-layer Nord Stage 4 real-time DSP engine.
#[derive(Debug, Clone)]
pub struct Stage4Engine {
    pub sample_rate: f32,
    pub organ_a: OrganLayer,
    pub organ_b: OrganLayer,
    pub piano_a: PianoLayer,
    pub piano_b: PianoLayer,
    pub synth_a: SynthLayer,
    pub synth_b: SynthLayer,
    pub synth_c: SynthLayer,
    pub fx_chains: [LayerFxChain; 7],
    pub layer_levels: [MorphParameter; 7],
    pub synth_cutoff_morph: [MorphParameter; 3],
    pub morph_controllers: MorphControllers,
    pub scene: LayerSceneState,
    pub split_router: KeybedSplitRouter,
    pub master_bus: MasterBus,
    pub master_clock: MasterClock,
    /// MIDI channel (0..15) used for global keybed, controller, and program input.
    pub global_midi_channel: Option<u8>,
    /// Optional direct MIDI channel for each layer; these notes bypass keybed splits.
    pub layer_midi_channels: [Option<u8>; 7],
    note_hold_counts: [[[u8; 7]; 128]; 16],
}

impl Stage4Engine {
    /// Creates a pre-allocated 7-layer Stage 4 DSP engine at `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        Self {
            sample_rate: fs,
            organ_a: OrganLayer::new(fs),
            organ_b: OrganLayer::new(fs),
            piano_a: PianoLayer::new(fs),
            piano_b: PianoLayer::new(fs),
            synth_a: SynthLayer::new(fs),
            synth_b: SynthLayer::new(fs),
            synth_c: SynthLayer::new(fs),
            fx_chains: std::array::from_fn(|_| LayerFxChain::new(fs)),
            layer_levels: [MorphParameter::new(0.8); 7],
            synth_cutoff_morph: [MorphParameter::new(0.5); 3],
            morph_controllers: MorphControllers::default(),
            scene: LayerSceneState::default(),
            split_router: KeybedSplitRouter::default(),
            master_bus: MasterBus::default(),
            master_clock: MasterClock::default(),
            global_midi_channel: Some(0),
            layer_midi_channels: [None; 7],
            note_hold_counts: [[[0; 7]; 128]; 16],
        }
    }

    /// Renders a single stereo frame `[left, right]` across all 7 layers + 7 FX racks + Master Bus.
    pub fn render_frame(&mut self) -> [f32; 2] {
        self.master_clock.advance(self.sample_rate);
        self.sync_synth_clock();

        // Apply Morphing Matrix to Synth filter cutoffs via OS v1.66 14 Hz .. 32 kHz curve
        let cut_a = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[0].evaluate(&self.morph_controllers),
        );
        let cut_b = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[1].evaluate(&self.morph_controllers),
        );
        let cut_c = crate::cbin::cutoff_norm_to_hz(
            self.synth_cutoff_morph[2].evaluate(&self.morph_controllers),
        );
        if self.synth_cutoff_morph[0].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[0].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[0].delta_aftertouch.abs() > 1e-5
        {
            self.synth_a.cutoff_hz = cut_a;
        }
        if self.synth_cutoff_morph[1].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[1].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[1].delta_aftertouch.abs() > 1e-5
        {
            self.synth_b.cutoff_hz = cut_b;
        }
        if self.synth_cutoff_morph[2].delta_wheel.abs() > 1e-5
            || self.synth_cutoff_morph[2].delta_pedal.abs() > 1e-5
            || self.synth_cutoff_morph[2].delta_aftertouch.abs() > 1e-5
        {
            self.synth_c.cutoff_hz = cut_c;
        }

        let raw_layers = [
            self.organ_a.render_frame(),
            self.organ_b.render_frame(),
            self.piano_a.render_frame(),
            self.piano_b.render_frame(),
            self.synth_a.render_frame(),
            self.synth_b.render_frame(),
            self.synth_c.render_frame(),
        ];

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for (i, raw) in raw_layers.into_iter().enumerate() {
            self.fx_chains[i].apply_morph(&self.morph_controllers);
            let [fx_l, fx_r] = self.fx_chains[i].process_frame_with_bpm(raw, self.master_clock.bpm);
            let gain = self.layer_levels[i].evaluate(&self.morph_controllers);
            mix_l += fx_l * gain;
            mix_r += fx_r * gain;
        }

        self.master_bus.process_frame([mix_l, mix_r])
    }

    /// Feeds the Master Clock tempo to the Synth layers and applies their KB Sync clock resets.
    fn sync_synth_clock(&mut self) {
        let bpm = self.master_clock.bpm;
        let mut reset = false;
        for synth in [&mut self.synth_a, &mut self.synth_b, &mut self.synth_c] {
            synth.perf.master_bpm = bpm;
            reset |= synth.take_clock_reset();
        }
        if reset {
            self.master_clock.beat_phase = 0.0;
        }
    }

    /// Processes an interleaved or dual-channel block in place with zero heap allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        for i in 0..frames {
            let [l, r] = self.render_frame();
            left[i] = l;
            right[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_engine_is_silent_and_finite() {
        let mut e = Stage4Engine::new(48_000.0);
        for _ in 0..32 {
            assert_eq!(e.render_frame(), [0.0, 0.0]);
        }
    }
}
