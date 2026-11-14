//! The engine the standalone runs, built identically for the audio callback and for the control
//! thread's shadow so both start from the same state (TD-12).

use nord_core::engine::Stage4Engine;
use nord_core::synth::VaWaveform;

/// New engine with the panel's start-up state: Organ A and Piano A sounding, Synth A muted until
/// enabled, SuperSaw on the synth.
pub fn standalone_engine(sample_rate: f32) -> Stage4Engine {
    let mut engine = Stage4Engine::new(sample_rate);
    engine.layer_levels[0].base = 0.85;
    engine.layer_levels[1].base = 0.0;
    engine.layer_levels[2].base = 0.90;
    for level in &mut engine.layer_levels[3..] {
        level.base = 0.0;
    }
    engine.synth_a.waveform = VaWaveform::SuperSaw;
    engine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_up_levels_match_the_panel_defaults() {
        let e = standalone_engine(48_000.0);
        let base: Vec<f32> = e.layer_levels.iter().map(|l| l.base).collect();
        assert_eq!(base, [0.85, 0.0, 0.90, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(e.synth_a.waveform, VaWaveform::SuperSaw);
    }
}
