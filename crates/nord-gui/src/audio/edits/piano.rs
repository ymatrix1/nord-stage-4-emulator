//! Piano edits applied on the DSP thread (one variant per panel control).
//!
//! `AudioCommand::Piano` carries them. Layer indices are `0` (Piano A) and `1` (Piano B).
//! [`PianoEdit::apply`] only writes engine fields (no allocation, no lock), so it is safe on the
//! audio thread.

use nord_core::engine::Stage4Engine;
use nord_core::morph::LayerId;
use nord_core::piano::{PianoCategory, PianoLayer, PianoTimbre, VelocityTouch};

/// Index of Piano A in `layer_levels` and the bit of Piano A in the scene masks.
const PIANO_FIRST_LAYER: u8 = 2;

/// Panel edits for the Piano area.
#[derive(Debug, Clone, PartialEq)]
pub enum PianoEdit {
    /// Morphable level of one layer (0..=1), the fader.
    Level { layer: u8, level: f32 },
    /// Layer button / hold-off: add or remove the layer from both scenes.
    LayerActive { layer: u8, on: bool },
    /// Replace the whole 7-layer scene mask (SOLO and its restore).
    SceneMask(u8),
    /// PIANO SELECT type, applied to both layers (host `piano_category` is global).
    Category(PianoCategory),
    /// TIMBRE, applied to both layers (host `piano_timbre` is global).
    Timbre(PianoTimbre),
    /// KB TOUCH, applied to both layers (host `piano_touch` is global).
    Touch(VelocityTouch),
    /// STRING RES, applied to both layers.
    StringRes(bool),
    /// PED NOISE, applied to both layers.
    PedalNoise(bool),
    /// SOFT RELEASE of one layer.
    SoftRelease { layer: u8, on: bool },
    /// UNISON 0 (off) ..= 3 of one layer.
    Unison { layer: u8, amount: u8 },
    /// DYN COMP 0 (off) ..= 3 of one layer.
    DynComp { layer: u8, amount: u8 },
    /// OCTAVE SHIFT -2..=2 of one layer.
    OctaveShift { layer: u8, octaves: i8 },
    /// SUSTPED of the Piano section (both layers follow the sustain pedal).
    Sustped(bool),
    /// PSTICK of the Piano section (both layers follow the pitch stick).
    Pstick(bool),
    /// KB ZONE of one layer as a set of the four zone LEDs (bit `u` = zone `u + 1`).
    Zone { layer: u8, leds: u8 },
    /// Aux KB LED of one layer: the layer is played by the aux keyboard.
    AuxKb { layer: u8, on: bool },
}

fn layer_id(layer: u8) -> LayerId {
    if layer == 0 {
        LayerId::PianoA
    } else {
        LayerId::PianoB
    }
}

fn layer_mut(engine: &mut Stage4Engine, layer: u8) -> &mut PianoLayer {
    if layer == 0 {
        &mut engine.piano_a
    } else {
        &mut engine.piano_b
    }
}

impl PianoEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            Self::Level { layer, level } => {
                let idx = usize::from(PIANO_FIRST_LAYER + layer.min(1));
                engine.layer_levels[idx].base = level.clamp(0.0, 1.0);
            }
            Self::LayerActive { layer, on } => {
                let layer = layer.min(1);
                let bit = 1u8 << (PIANO_FIRST_LAYER + layer);
                if on {
                    engine.scene.scene_1_mask |= bit;
                    engine.scene.scene_2_mask |= bit;
                } else {
                    engine.scene.scene_1_mask &= !bit;
                    engine.scene.scene_2_mask &= !bit;
                    // Release anything still sounding on the layer (same as SetLayerActive).
                    for n in 0..128u8 {
                        layer_mut(engine, layer).note_off(n);
                    }
                }
            }
            Self::SceneMask(mask) => {
                let mask = mask & 0x7F;
                engine.scene.scene_1_mask = mask;
                engine.scene.scene_2_mask = mask;
            }
            Self::Category(c) => {
                engine.piano_a.category = c;
                engine.piano_b.category = c;
            }
            Self::Timbre(t) => {
                engine.piano_a.timbre = t;
                engine.piano_b.timbre = t;
            }
            Self::Touch(t) => {
                engine.piano_a.touch = t;
                engine.piano_b.touch = t;
            }
            Self::StringRes(on) => {
                engine.piano_a.string_resonance_enabled = on;
                engine.piano_b.string_resonance_enabled = on;
            }
            Self::PedalNoise(on) => {
                engine.piano_a.pedal_noise_enabled = on;
                engine.piano_b.pedal_noise_enabled = on;
            }
            Self::SoftRelease { layer, on } => layer_mut(engine, layer).soft_release = on,
            Self::Unison { layer, amount } => layer_mut(engine, layer).unison = amount.min(3),
            Self::DynComp { layer, amount } => layer_mut(engine, layer).dyn_comp = amount.min(3),
            Self::OctaveShift { layer, octaves } => {
                layer_mut(engine, layer).octave_shift = octaves.clamp(-2, 2);
            }
            Self::Sustped(on) => {
                let a = usize::from(PIANO_FIRST_LAYER);
                engine.pedals.sustped[a] = on;
                engine.pedals.sustped[a + 1] = on;
                engine.refresh_pedal_routing();
            }
            Self::Pstick(on) => {
                let a = usize::from(PIANO_FIRST_LAYER);
                engine.pedals.pstick[a] = on;
                engine.pedals.pstick[a + 1] = on;
                engine.refresh_pedal_routing();
            }
            Self::Zone { layer, leds } => engine.set_layer_zone_leds(layer_id(layer), leds),
            Self::AuxKb { layer, on } => engine.set_layer_aux(layer_id(layer), on),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eng() -> Stage4Engine {
        Stage4Engine::new(48_000.0)
    }

    #[test]
    fn level_targets_the_piano_layer_slots_and_clamps() {
        let mut e = eng();
        PianoEdit::Level {
            layer: 0,
            level: 0.3,
        }
        .apply(&mut e);
        PianoEdit::Level {
            layer: 1,
            level: 2.0,
        }
        .apply(&mut e);
        assert_eq!(e.layer_levels[2].base, 0.3);
        assert_eq!(e.layer_levels[3].base, 1.0);
    }

    #[test]
    fn layer_active_sets_and_clears_the_scene_bits() {
        let mut e = eng();
        PianoEdit::LayerActive { layer: 1, on: true }.apply(&mut e);
        assert_ne!(e.scene.scene_1_mask & 0b1000, 0);
        assert_ne!(e.scene.scene_2_mask & 0b1000, 0);
        PianoEdit::LayerActive {
            layer: 1,
            on: false,
        }
        .apply(&mut e);
        assert_eq!(e.scene.scene_1_mask & 0b1000, 0);
        assert_eq!(e.scene.scene_2_mask & 0b1000, 0);
    }

    #[test]
    fn scene_mask_replaces_both_scenes() {
        let mut e = eng();
        PianoEdit::SceneMask(0b0000_1100).apply(&mut e);
        assert_eq!(
            (e.scene.scene_1_mask, e.scene.scene_2_mask),
            (0b1100, 0b1100)
        );
        PianoEdit::SceneMask(0xFF).apply(&mut e);
        assert_eq!(e.scene.scene_1_mask, 0x7F);
    }

    #[test]
    fn global_switches_reach_both_layers() {
        let mut e = eng();
        PianoEdit::Category(PianoCategory::Clav).apply(&mut e);
        PianoEdit::Timbre(PianoTimbre::Dyno2).apply(&mut e);
        PianoEdit::Touch(VelocityTouch::Heavy).apply(&mut e);
        PianoEdit::StringRes(false).apply(&mut e);
        PianoEdit::PedalNoise(false).apply(&mut e);
        for l in [&e.piano_a, &e.piano_b] {
            assert_eq!(l.category, PianoCategory::Clav);
            assert_eq!(l.timbre, PianoTimbre::Dyno2);
            assert_eq!(l.touch, VelocityTouch::Heavy);
            assert!(!l.string_resonance_enabled);
            assert!(!l.pedal_noise_enabled);
        }
    }

    #[test]
    fn per_layer_edits_touch_only_their_layer() {
        let mut e = eng();
        PianoEdit::SoftRelease { layer: 1, on: true }.apply(&mut e);
        PianoEdit::Unison {
            layer: 1,
            amount: 2,
        }
        .apply(&mut e);
        PianoEdit::DynComp {
            layer: 0,
            amount: 3,
        }
        .apply(&mut e);
        PianoEdit::OctaveShift {
            layer: 0,
            octaves: -1,
        }
        .apply(&mut e);
        assert!(e.piano_b.soft_release && !e.piano_a.soft_release);
        assert_eq!((e.piano_b.unison, e.piano_a.unison), (2, 0));
        assert_eq!((e.piano_a.dyn_comp, e.piano_b.dyn_comp), (3, 0));
        assert_eq!((e.piano_a.octave_shift, e.piano_b.octave_shift), (-1, 0));
    }

    #[test]
    fn sustped_and_pstick_set_both_piano_layers_only() {
        let mut e = eng();
        PianoEdit::Sustped(false).apply(&mut e);
        PianoEdit::Pstick(true).apply(&mut e);
        assert_eq!(&e.pedals.sustped[2..4], &[false, false]);
        assert_eq!(&e.pedals.pstick[2..4], &[true, true]);
        assert!(!e.pedals.pstick[0] && !e.pedals.pstick[4]);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut e = eng();
        PianoEdit::Unison {
            layer: 0,
            amount: 9,
        }
        .apply(&mut e);
        PianoEdit::DynComp {
            layer: 0,
            amount: 9,
        }
        .apply(&mut e);
        PianoEdit::OctaveShift {
            layer: 0,
            octaves: 9,
        }
        .apply(&mut e);
        assert_eq!(
            (e.piano_a.unison, e.piano_a.dyn_comp, e.piano_a.octave_shift),
            (3, 3, 2)
        );
    }

    #[test]
    fn zone_and_aux_edits_target_one_piano_layer() {
        let mut e = eng();
        PianoEdit::Zone {
            layer: 1,
            leds: 0b0011,
        }
        .apply(&mut e);
        assert_eq!(e.layer_zone_leds(LayerId::PianoB), 0b0011);
        assert_eq!(e.layer_zone_leds(LayerId::PianoA), 0b1111);
        assert_eq!(e.layer_zone_leds(LayerId::OrganB), 0b1111);
        PianoEdit::AuxKb { layer: 0, on: true }.apply(&mut e);
        assert!(e.aux_kb.is_aux_layer(LayerId::PianoA));
        assert!(!e.aux_kb.is_aux_layer(LayerId::PianoB));
    }
}
