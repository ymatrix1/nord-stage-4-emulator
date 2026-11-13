//! LAYER INIT (Shift+Section Edit, manual p.43): All / Organ AB / Piano A,B / Synth A,B,C.
//!
//! Allocates (fresh layers and a snapshot for Undo): control thread only.

use super::*;

/// Default layer level, equal to `Stage4Engine::new`.
const DEFAULT_LEVEL: f32 = 0.8;
/// Default synth cutoff morph base, equal to `Stage4Engine::new`.
const DEFAULT_CUTOFF_NORM: f32 = 0.5;

/// What a Layer Init resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerInitTarget {
    /// Whole program: only Piano A active, no effects.
    All,
    /// Both Organ layers to a B3 sound with Layer A on and Rotary on, other organ FX off.
    OrganAb,
    PianoA,
    PianoB,
    SynthA,
    SynthB,
    SynthC,
}

impl Stage4Engine {
    /// Runs a Layer Init. The state before it is kept for [`Stage4Engine::undo`].
    ///
    /// Piano layers keep their loaded sound (`ASSUMED`: the hardware default piano is whatever
    /// the factory bank provides; the host owns sound loading). Synth layers drop their loaded
    /// sample and return to the default waveform (manual p.43).
    pub fn layer_init(&mut self, target: LayerInitTarget) {
        self.begin_undoable(UndoOp::LayerInit);
        let fx = FxParams::defaults(self.sample_rate);
        match target {
            LayerInitTarget::All => {
                for layer in LayerId::ALL {
                    self.init_layer(layer, &fx);
                }
                let piano_a_only = 1 << (LayerId::PianoA as u8);
                self.scene = LayerSceneState {
                    scene_1_mask: piano_a_only,
                    scene_2_mask: piano_a_only,
                    scene_2_active: false,
                };
                self.split_router = KeybedSplitRouter::default();
                self.set_transpose(0);
                self.set_program_level_db(0.0);
            }
            LayerInitTarget::OrganAb => {
                self.init_layer(LayerId::OrganA, &fx);
                self.init_layer(LayerId::OrganB, &fx);
                let (a, b) = (
                    1u8 << (LayerId::OrganA as u8),
                    1u8 << (LayerId::OrganB as u8),
                );
                self.scene.scene_1_mask = (self.scene.scene_1_mask | a) & !b;
                self.scene.scene_2_mask = (self.scene.scene_2_mask | a) & !b;
            }
            LayerInitTarget::PianoA => self.init_layer(LayerId::PianoA, &fx),
            LayerInitTarget::PianoB => self.init_layer(LayerId::PianoB, &fx),
            LayerInitTarget::SynthA => self.init_layer(LayerId::SynthA, &fx),
            LayerInitTarget::SynthB => self.init_layer(LayerId::SynthB, &fx),
            LayerInitTarget::SynthC => self.init_layer(LayerId::SynthC, &fx),
        }
    }

    fn init_layer(&mut self, layer: LayerId, fx: &FxParams) {
        let fs = self.sample_rate;
        match layer {
            LayerId::OrganA => self.organ_a = OrganLayer::new(fs),
            LayerId::OrganB => self.organ_b = OrganLayer::new(fs),
            LayerId::PianoA => Self::reset_piano(&mut self.piano_a, fs),
            LayerId::PianoB => Self::reset_piano(&mut self.piano_b, fs),
            LayerId::SynthA => self.synth_a = SynthLayer::new(fs),
            LayerId::SynthB => self.synth_b = SynthLayer::new(fs),
            LayerId::SynthC => self.synth_c = SynthLayer::new(fs),
        }
        if let Some(i) = (layer as usize).checked_sub(LayerId::SynthA as usize) {
            self.synth_cutoff_morph[i] = MorphParameter::new(DEFAULT_CUTOFF_NORM);
        }
        fx.apply(&mut self.fx_chains[layer as usize]);
        self.layer_levels[layer as usize] = MorphParameter::new(DEFAULT_LEVEL);
    }

    fn reset_piano(piano: &mut PianoLayer, fs: f32) {
        let sound = piano.sound.clone();
        *piano = PianoLayer::new(fs);
        piano.set_sound(sound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_init_resets_only_that_layer_and_is_undoable() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_b.cutoff_hz = 777.0;
        e.synth_c.cutoff_hz = 888.0;
        e.fx_chains[5].delay_enabled = true;
        e.layer_levels[5].base = 0.1;
        e.synth_cutoff_morph[1].delta_wheel = 0.4;
        e.layer_init(LayerInitTarget::SynthB);
        assert_eq!(e.synth_b.cutoff_hz, SynthLayer::new(48_000.0).cutoff_hz);
        assert_eq!(e.synth_c.cutoff_hz, 888.0);
        assert!(!e.fx_chains[5].delay_enabled);
        assert_eq!(e.layer_levels[5].base, DEFAULT_LEVEL);
        assert_eq!(e.synth_cutoff_morph[1].delta_wheel, 0.0);
        assert_eq!(e.undo(), Some(UndoOp::LayerInit));
        assert_eq!(e.synth_b.cutoff_hz, 777.0);
        assert!(e.fx_chains[5].delay_enabled);
    }

    #[test]
    fn init_all_leaves_only_piano_a_active() {
        let mut e = Stage4Engine::new(48_000.0);
        e.set_transpose(3);
        e.set_program_level_db(-4.0);
        e.fx_chains[2].reverb_mix = 0.9;
        e.layer_init(LayerInitTarget::All);
        assert!(e.scene.is_layer_active(LayerId::PianoA));
        assert!(!e.scene.is_layer_active(LayerId::OrganA));
        assert!(!e.scene.is_layer_active(LayerId::SynthA));
        e.scene.scene_2_active = true;
        assert!(!e.scene.is_layer_active(LayerId::SynthC));
        assert_eq!(e.transpose(), 0);
        assert_eq!(e.program_level_db(), 0.0);
        assert!(e
            .fx_chains
            .iter()
            .all(|c| !c.delay_enabled && !c.comp_enabled));
    }

    #[test]
    fn organ_init_turns_a_on_b_off_with_rotary_on() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.rotary_enabled = false;
        e.organ_a.drawbars = [0; 9];
        e.scene.scene_1_mask = 1 << (LayerId::OrganB as u8);
        e.layer_init(LayerInitTarget::OrganAb);
        assert!(e.organ_a.rotary_enabled && e.organ_b.rotary_enabled);
        assert_eq!(e.organ_a.drawbars, OrganLayer::new(48_000.0).drawbars);
        assert!(e.scene.is_layer_active(LayerId::OrganA));
        assert!(!e.scene.is_layer_active(LayerId::OrganB));
    }

    #[test]
    fn piano_init_keeps_the_loaded_sound_slot_and_resets_params() {
        let mut e = Stage4Engine::new(48_000.0);
        e.piano_b.unison = 3;
        e.layer_init(LayerInitTarget::PianoB);
        assert_eq!(e.piano_b.unison, PianoLayer::new(48_000.0).unison);
    }
}
