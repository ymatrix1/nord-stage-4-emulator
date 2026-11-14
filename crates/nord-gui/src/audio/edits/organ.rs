//! Organ and Rotary edits applied on the DSP thread (one variant per engine-affecting panel
//! control that has no older `AudioCommand`).
//!
//! Handled in [`OrganEdit::apply`]; `AudioCommand::Organ` carries them. Layer indices are
//! `0 = Organ A`, anything else `Organ B`.

use nord_core::engine::Stage4Engine;
use nord_core::organ::{OrganLayer, OrganModel, RotarySpeed, ScannerMode};

/// Panel edits for the Organ area.
#[derive(Debug, Clone, PartialEq)]
pub enum OrganEdit {
    /// Octave shift of one layer (`-2..=2`, manual p.18); host `organx_octave_{n}`.
    Octave { layer: u8, shift: i8 },
    /// ORGAN MODEL selector (six LEDs): model plus the B3 Bass flavour, both layers.
    Model { model: OrganModel, b3_bass: bool },
    /// Vibrato/Chorus type of one layer; `Off` when the layer's VIB/CHORUS ON is off.
    Scanner { layer: u8, mode: ScannerMode },
    /// Rotary speed button. With STOP MODE the slow position is the brake (manual p.53), so the
    /// engine brake flag is set only while the button sits on the slow/stop side.
    Rotary { fast: bool, stop_mode: bool },
    /// Rotary CLOSE MIC (both layers share the one speaker).
    CloseMic(bool),
}

fn layer_mut(engine: &mut Stage4Engine, layer: u8) -> &mut OrganLayer {
    if layer == 0 {
        &mut engine.organ_a
    } else {
        &mut engine.organ_b
    }
}

impl OrganEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            OrganEdit::Octave { layer, shift } => {
                layer_mut(engine, layer).octave_shift = shift.clamp(-2, 2);
            }
            OrganEdit::Model { model, b3_bass } => {
                for l in [&mut engine.organ_a, &mut engine.organ_b] {
                    l.model = model;
                    l.b3_bass_mode = b3_bass;
                }
            }
            OrganEdit::Scanner { layer, mode } => {
                layer_mut(engine, layer).scanner_mode = mode;
            }
            OrganEdit::Rotary { fast, stop_mode } => {
                let speed = if fast {
                    RotarySpeed::Fast
                } else {
                    RotarySpeed::Slow
                };
                for l in [&mut engine.organ_a, &mut engine.organ_b] {
                    l.rotary_speed = speed;
                    l.rotary_stop_mode = stop_mode && !fast;
                }
            }
            OrganEdit::CloseMic(on) => {
                engine.organ_a.rotary_close_mic = on;
                engine.organ_b.rotary_close_mic = on;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        Stage4Engine::new(44_100.0)
    }

    #[test]
    fn octave_targets_one_layer_and_clamps() {
        let mut e = engine();
        OrganEdit::Octave { layer: 1, shift: 5 }.apply(&mut e);
        assert_eq!((e.organ_a.octave_shift, e.organ_b.octave_shift), (0, 2));
        OrganEdit::Octave {
            layer: 0,
            shift: -9,
        }
        .apply(&mut e);
        assert_eq!(e.organ_a.octave_shift, -2);
    }

    #[test]
    fn model_sets_both_layers_and_b3_bass() {
        let mut e = engine();
        OrganEdit::Model {
            model: OrganModel::Vox,
            b3_bass: true,
        }
        .apply(&mut e);
        assert_eq!(e.organ_a.model, OrganModel::Vox);
        assert!(e.organ_b.b3_bass_mode);
    }

    #[test]
    fn scanner_targets_one_layer() {
        let mut e = engine();
        OrganEdit::Scanner {
            layer: 1,
            mode: ScannerMode::V2,
        }
        .apply(&mut e);
        assert_eq!(e.organ_b.scanner_mode, ScannerMode::V2);
        assert_ne!(e.organ_a.scanner_mode, ScannerMode::V2);
    }

    #[test]
    fn rotary_brake_only_on_the_slow_side_in_stop_mode() {
        let mut e = engine();
        OrganEdit::Rotary {
            fast: false,
            stop_mode: true,
        }
        .apply(&mut e);
        assert!(e.organ_a.rotary_stop_mode && e.organ_b.rotary_stop_mode);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Slow);
        OrganEdit::Rotary {
            fast: true,
            stop_mode: true,
        }
        .apply(&mut e);
        assert!(!e.organ_a.rotary_stop_mode);
        assert_eq!(e.organ_b.rotary_speed, RotarySpeed::Fast);
    }

    #[test]
    fn close_mic_sets_both_layers() {
        let mut e = engine();
        OrganEdit::CloseMic(true).apply(&mut e);
        assert!(e.organ_a.rotary_close_mic && e.organ_b.rotary_close_mic);
    }
}
