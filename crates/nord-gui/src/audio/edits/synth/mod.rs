//! Synth edits applied on the DSP thread (one `SynthParam` variant per engine-affecting control).
//!
//! `AudioCommand::Synth` carries a [`SynthEdit`]. A `Param` edit is written to the focused layer,
//! or to all three layers when a GROUP / Section Edit is on ([`SynthTarget::All`]); a `Group` edit
//! copies one settings group from a source layer to the others. Nothing here allocates.

mod group;
mod param;

pub use group::SynthGroup;
pub use param::SynthParam;

use nord_core::engine::Stage4Engine;
use nord_core::synth::SynthLayer;

/// Which Synth layers an edit reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthTarget {
    /// One layer: 0 = A, 1 = B, 2 = C.
    Layer(u8),
    /// All three layers (GROUP on, or a section-wide control such as KB Hold).
    All,
}

/// Panel edits for the Synth area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynthEdit {
    /// Write one control value.
    Param(SynthTarget, SynthParam),
    /// GROUP: copy a settings group from layer `src` to the other layers.
    Group { src: u8, group: SynthGroup },
}

/// The three synth layers, in A, B, C order.
fn layers(engine: &mut Stage4Engine) -> [&mut SynthLayer; 3] {
    [
        &mut engine.synth_a,
        &mut engine.synth_b,
        &mut engine.synth_c,
    ]
}

impl SynthEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            Self::Param(SynthTarget::Layer(i), p) => {
                if let Some(l) = layers(engine).into_iter().nth(usize::from(i)) {
                    p.apply(l);
                }
            }
            Self::Param(SynthTarget::All, p) => {
                for l in layers(engine) {
                    p.apply(l);
                }
            }
            Self::Group { src, group } => {
                let [a, b, c] = layers(engine);
                match src {
                    0 => {
                        group.copy(a, b);
                        group.copy(a, c);
                    }
                    1 => {
                        group.copy(b, a);
                        group.copy(b, c);
                    }
                    _ => {
                        group.copy(c, a);
                        group.copy(c, b);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth_types::LfoShape;

    #[test]
    fn param_edit_reaches_only_the_target_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Param(SynthTarget::Layer(1), SynthParam::Cutoff(1234.0)).apply(&mut e);
        assert_eq!(e.synth_b.cutoff_hz, 1234.0);
        assert_ne!(e.synth_a.cutoff_hz, 1234.0);
        assert_ne!(e.synth_c.cutoff_hz, 1234.0);
    }

    #[test]
    fn all_target_reaches_every_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        SynthEdit::Param(SynthTarget::All, SynthParam::Resonance(0.77)).apply(&mut e);
        assert_eq!(e.synth_a.resonance, 0.77);
        assert_eq!(e.synth_b.resonance, 0.77);
        assert_eq!(e.synth_c.resonance, 0.77);
    }

    #[test]
    fn out_of_range_layer_is_ignored() {
        let mut e = Stage4Engine::new(48_000.0);
        let before = e.synth_a.cutoff_hz;
        SynthEdit::Param(SynthTarget::Layer(9), SynthParam::Cutoff(50.0)).apply(&mut e);
        assert_eq!(e.synth_a.cutoff_hz, before);
    }

    #[test]
    fn group_edit_copies_from_the_source_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_c.lfo_shape = LfoShape::Square;
        SynthEdit::Group {
            src: 2,
            group: SynthGroup::Lfo,
        }
        .apply(&mut e);
        assert_eq!(e.synth_a.lfo_shape, LfoShape::Square);
        assert_eq!(e.synth_b.lfo_shape, LfoShape::Square);
    }

    #[test]
    fn edits_arrive_through_the_audio_command_path() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut master = 0.8;
        crate::audio::apply_audio_command(
            crate::audio::AudioCommand::Synth(SynthEdit::Param(
                SynthTarget::Layer(2),
                SynthParam::Glide(0.5),
            )),
            &mut e,
            &mut master,
        );
        assert_eq!(e.synth_c.glide_sec, 0.5);
    }
}
