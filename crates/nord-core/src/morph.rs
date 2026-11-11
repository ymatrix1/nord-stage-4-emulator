//! 3-Source Morphing Matrix, Layer Scene I/II Toggle, and 4-Zone Keybed Split Router.
//!
//! Spec: `docs/specs/morph-and-zones.md`

use serde::{Deserialize, Serialize};

/// Physical controller sources in the Nord Stage 4 Morphing Matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MorphSource {
    /// Modulation Wheel (`CC1`).
    ModWheel,
    /// Control / Expression Pedal (`CC11`).
    ControlPedal,
    /// Channel / Keybed Aftertouch.
    Aftertouch,
}

/// Real-time normalized state (`0.0..=1.0`) of the three Morphing physical controllers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MorphControllers {
    pub mod_wheel: f32,
    pub control_pedal: f32,
    pub aftertouch: f32,
}

/// Continuous parameter with a base value (`0.0..=1.0`) and 3 signed morph deltas (`-1.0..=1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphParameter {
    pub base: f32,
    pub delta_wheel: f32,
    pub delta_pedal: f32,
    pub delta_aftertouch: f32,
}

impl MorphParameter {
    pub const fn new(base: f32) -> Self {
        Self {
            base,
            delta_wheel: 0.0,
            delta_pedal: 0.0,
            delta_aftertouch: 0.0,
        }
    }

    /// Evaluates the effective parameter value clamped to `[0.0, 1.0]`.
    pub fn evaluate(&self, ctrl: &MorphControllers) -> f32 {
        (self.base
            + ctrl.mod_wheel.clamp(0.0, 1.0) * self.delta_wheel
            + ctrl.control_pedal.clamp(0.0, 1.0) * self.delta_pedal
            + ctrl.aftertouch.clamp(0.0, 1.0) * self.delta_aftertouch)
            .clamp(0.0, 1.0)
    }
}

/// Identifier for each of the 7 multitimbral layers in the Nord Stage 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LayerId {
    OrganA = 0,
    OrganB = 1,
    PianoA = 2,
    PianoB = 3,
    SynthA = 4,
    SynthB = 5,
    SynthC = 6,
}

impl LayerId {
    pub const ALL: [LayerId; 7] = [
        Self::OrganA,
        Self::OrganB,
        Self::PianoA,
        Self::PianoB,
        Self::SynthA,
        Self::SynthB,
        Self::SynthC,
    ];
}

/// Dual-scene layer activation state (`Scene I` vs `Scene II`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerSceneState {
    pub scene_1_mask: u8,
    pub scene_2_mask: u8,
    pub scene_2_active: bool,
}

impl Default for LayerSceneState {
    fn default() -> Self {
        Self {
            // Default: Organ A, Piano A, Synth A active in Scene I
            scene_1_mask: (1 << 0) | (1 << 2) | (1 << 4),
            // Scene II: all 7 layers active
            scene_2_mask: 0b0111_1111,
            scene_2_active: false,
        }
    }
}

impl LayerSceneState {
    pub fn is_layer_active(&self, layer: LayerId) -> bool {
        let mask = if self.scene_2_active {
            self.scene_2_mask
        } else {
            self.scene_1_mask
        };
        (mask & (1 << (layer as u8))) != 0
    }
}

/// Crossfade width around a keybed split point (`Off`, `Small` = ±3 st, `Large` = ±6 st).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossfadeWidth {
    Off,
    Small,
    Large,
}

impl CrossfadeWidth {
    pub const fn half_semitones(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Small => 3.0,
            Self::Large => 6.0,
        }
    }
}

/// Single split boundary between two adjacent keyboard zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPoint {
    pub enabled: bool,
    pub midi_note: u8,
    pub crossfade: CrossfadeWidth,
}

/// 4-zone keybed split router with crossfade calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeybedSplitRouter {
    pub splits: [SplitPoint; 3],
    pub layer_zone_masks: [u8; 7],
}

impl Default for KeybedSplitRouter {
    fn default() -> Self {
        Self {
            splits: [
                SplitPoint {
                    enabled: false,
                    midi_note: 48,
                    crossfade: CrossfadeWidth::Off,
                },
                SplitPoint {
                    enabled: false,
                    midi_note: 60,
                    crossfade: CrossfadeWidth::Small,
                },
                SplitPoint {
                    enabled: false,
                    midi_note: 72,
                    crossfade: CrossfadeWidth::Large,
                },
            ],
            // Default: all layers assigned to all 4 zones (0b1111)
            layer_zone_masks: [0b1111; 7],
        }
    }
}

impl KeybedSplitRouter {
    /// Computes linear gain (`0.0..=1.0`) for `layer` at `midi_note` across active split boundaries.
    pub fn note_gain(&self, layer: LayerId, midi_note: u8) -> f32 {
        let mask = self.layer_zone_masks[layer as usize] & 0b1111;
        if mask == 0 {
            return 0.0;
        }
        if !self.splits.iter().any(|s| s.enabled) {
            return if (mask & 1) != 0 { 1.0 } else { 0.0 };
        }

        // Evaluate across the first enabled split boundary for clean 2-zone/4-zone transitions
        let mut gain = 1.0f32;
        for (idx, split) in self.splits.iter().enumerate() {
            if !split.enabled {
                continue;
            }
            let lower_active = (mask & (1 << idx)) != 0;
            let upper_active = (mask & (1 << (idx + 1))) != 0;
            if lower_active && upper_active {
                continue;
            }
            if !lower_active && !upper_active {
                return 0.0;
            }
            let w = split.crossfade.half_semitones();
            let k = f32::from(midi_note);
            let ks = f32::from(split.midi_note);
            let low_g = if w <= 0.0 {
                if midi_note < split.midi_note {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((ks + w - k) / (2.0 * w)).clamp(0.0, 1.0)
            };
            gain *= if lower_active { low_g } else { 1.0 - low_g };
        }
        gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morph_parameter_additive_and_clamped() {
        let mut param = MorphParameter::new(0.4);
        param.delta_wheel = 0.5;
        param.delta_pedal = -0.3;
        param.delta_aftertouch = 0.2;

        let ctrl = MorphControllers {
            mod_wheel: 1.0,
            control_pedal: 0.5,
            aftertouch: 0.5,
        };
        // 0.4 + 0.5 - 0.15 + 0.1 = 0.85
        let val = param.evaluate(&ctrl);
        assert!((val - 0.85).abs() < 1e-6);

        // Clamping above 1.0
        param.delta_wheel = 0.9;
        assert!((param.evaluate(&ctrl) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_split_crossfade_unity_sum() {
        let mut router = KeybedSplitRouter::default();
        router.splits[0] = SplitPoint {
            enabled: true,
            midi_note: 60,
            crossfade: CrossfadeWidth::Large, // ±6 semitones
        };
        // Layer OrganA on Zone 1 (below 60), PianoA on Zone 2 (above 60)
        router.layer_zone_masks[LayerId::OrganA as usize] = 0b0001;
        router.layer_zone_masks[LayerId::PianoA as usize] = 0b0010;

        for note in 48..=72 {
            let g_low = router.note_gain(LayerId::OrganA, note);
            let g_high = router.note_gain(LayerId::PianoA, note);
            assert!(
                (g_low + g_high - 1.0).abs() < 1e-6,
                "crossfade unity sum failed at note {note}"
            );
        }
        assert!((router.note_gain(LayerId::OrganA, 60) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_layer_scene_toggle() {
        let mut scene = LayerSceneState::default();
        assert!(scene.is_layer_active(LayerId::OrganA));
        assert!(!scene.is_layer_active(LayerId::OrganB));
        scene.scene_2_active = true;
        assert!(scene.is_layer_active(LayerId::OrganB));
    }
}
