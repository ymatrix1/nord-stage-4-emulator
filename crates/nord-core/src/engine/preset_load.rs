//! Section and layer application of decoded presets (manual OS v1.6x p.41-43).
//!
//! `apply_program` is built from the same three section functions, so a Preset Library entry
//! (`.ns4o` / `.ns4n` / `.ns4y`, whose payload is the program's section block, VERIFIED in
//! `cbin/section_preset.rs`) and a program set a section's parameters the same way.
//!
//! Real-time rule: these functions copy values into pre-built layers and may allocate (FX chain
//! set-up); run them on the control thread or at a load boundary, never inside `process_block`.

use super::*;
use crate::cbin::{
    FxDecoded, OrganPresetDecoded, PianoLayerDecoded, PianoPresetDecoded, SectionPreset,
    SynthLayerDecoded, SynthPresetDecoded,
};

/// Bit of `scene_*_mask` for each section (Organ A, B / Piano A, B / Synth A, B, C).
const ORGAN_BITS: u8 = 0b000_0011;
const PIANO_BITS: u8 = 0b000_1100;
const SYNTH_BITS: u8 = 0b111_0000;

impl Stage4Engine {
    fn set_layer_level(&mut self, idx: usize, level: f32, morph: crate::cbin::MorphDeltas) {
        let slot = &mut self.layer_levels[idx];
        slot.base = level.clamp(0.0, 1.0);
        slot.delta_wheel = morph.wheel;
        slot.delta_aftertouch = morph.aftertouch;
        slot.delta_pedal = morph.ctrl_pedal;
    }

    /// Organ section: both layers, the shared effect chain on both Organ chains, the levels.
    pub fn apply_organ_section(&mut self, org: &OrganPresetDecoded) {
        self.organ_a.model = org.model;
        self.organ_b.model = org.model_b;
        self.organ_a.b3_bass_mode = org.b3_bass_a;
        self.organ_b.b3_bass_mode = org.b3_bass_b;
        self.organ_a.drawbars = org.drawbars_a;
        self.organ_b.drawbars = org.drawbars_b;
        self.organ_a.octave_shift = org.layer_a_octave.clamp(-2, 2);
        self.organ_b.octave_shift = org.layer_b_octave.clamp(-2, 2);
        for (organ, vib_on) in [
            (&mut self.organ_a, org.vibrato_on),
            (&mut self.organ_b, org.vibrato_on_b),
        ] {
            organ.percussion_enabled = org.percussion_on;
            organ.percussion_soft = org.percussion_soft;
            organ.percussion_fast_decay = org.percussion_fast;
            organ.percussion_third_harmonic = org.percussion_third;
            organ.scanner_mode = org.scanner_mode(vib_on);
            organ.rotary_enabled = org.rotary_enabled;
            organ.rotary_speed = org.rotary_speed;
            // preset unit 0..10 (`(v / 127) * 10`) to the engine unit 0..1 = the firmware `v / 127`
            // (VERIFIED, docs/research/fx-type-map.md section 9); `rotary.rs` takes the fraction.
            organ.rotary_drive = (org.rotary_drive / 10.0).clamp(0.0, 1.0);
        }
        Self::apply_fx_chain(&mut self.fx_chains[0], &org.fx);
        Self::apply_fx_chain(&mut self.fx_chains[1], &org.fx);
        self.set_layer_level(0, org.layer_a_level, org.layer_a_level_morph);
        self.set_layer_level(1, org.layer_b_level, org.layer_b_level_morph);
    }

    /// One Piano layer (`slot_b`: Piano B) from a decoded layer block and its effect chain.
    fn apply_piano_layer(&mut self, slot_b: bool, dec: &PianoLayerDecoded, fx: &FxDecoded) {
        let (piano, idx) = if slot_b {
            (&mut self.piano_b, 3)
        } else {
            (&mut self.piano_a, 2)
        };
        piano.category = dec.category;
        piano.timbre = dec.timbre;
        piano.touch = dec.touch;
        piano.string_resonance_enabled = dec.string_res;
        piano.pedal_noise_enabled = dec.pedal_noise;
        piano.soft_release = dec.soft_release;
        piano.unison = dec.unison.min(3);
        piano.dyn_comp = dec.dyn_comp.min(3);
        piano.octave_shift = dec.octave.clamp(-2, 2);
        Self::apply_fx_chain(&mut self.fx_chains[idx], fx);
        self.set_layer_level(idx, dec.level, dec.level_morph);
    }

    /// Piano section: both layers with their effect chains and levels.
    pub fn apply_piano_section(&mut self, pno: &PianoPresetDecoded) {
        self.apply_piano_layer(false, &pno.layer_a, &pno.fx_a);
        self.apply_piano_layer(true, &pno.layer_b, &pno.fx_b);
    }

    /// One Synth layer `0..3` (A, B, C) from a decoded layer, its effect chain and the level.
    fn apply_synth_layer(&mut self, layer: usize, dec: &SynthLayerDecoded, fx: &FxDecoded) {
        let (synth, cutoff_morph) = match layer {
            0 => (&mut self.synth_a, &mut self.synth_cutoff_morph[0]),
            1 => (&mut self.synth_b, &mut self.synth_cutoff_morph[1]),
            _ => (&mut self.synth_c, &mut self.synth_cutoff_morph[2]),
        };
        crate::synth_params::apply_decoded_layer(synth, cutoff_morph, dec);
        let idx = 4 + layer.min(2);
        Self::apply_fx_chain(&mut self.fx_chains[idx], fx);
        self.set_layer_level(idx, dec.level, dec.level_morph);
    }

    /// Synth section: three layers with cutoff morph, effect chains and levels.
    pub fn apply_synth_section(&mut self, syn: &SynthPresetDecoded) {
        self.apply_synth_layer(0, &syn.layer_a, &syn.fx_a);
        self.apply_synth_layer(1, &syn.layer_b, &syn.fx_b);
        self.apply_synth_layer(2, &syn.layer_c, &syn.fx_c);
    }

    /// Scene masks after a preset load: `on` are the layers of `section_bits` switched on in the
    /// active Layer Scene. A section preset turns the section off in the other scene (manual
    /// p.43); a single-layer preset (`single`) leaves the other scene alone.
    fn set_preset_scene(&mut self, section_bits: u8, on: u8, single: bool) {
        let scene = &mut self.scene;
        let (active, other) = if scene.scene_2_active {
            (&mut scene.scene_2_mask, &mut scene.scene_1_mask)
        } else {
            (&mut scene.scene_1_mask, &mut scene.scene_2_mask)
        };
        *active = (*active & !section_bits) | (on & section_bits);
        if !single {
            *other &= !section_bits;
        }
    }

    /// Loads a Preset Library entry. `single` is the layer a single-layer preset goes to (Piano A
    /// / B, Synth A / B / C; Shift + PIANO / SYNTH, manual p.42): the preset's first layer is
    /// loaded there and the other layers of the section stay. `None` loads the whole section.
    /// An Organ preset always loads the whole section (manual p.41).
    ///
    /// ASSUMED: a single-layer load takes layer A of the file (the manual does not say which
    /// layer of a multi-layer preset a Single Layer load uses) and switches the target layer on
    /// in the active Layer Scene.
    pub fn load_section_preset(&mut self, preset: &SectionPreset, single: Option<LayerId>) {
        match (preset, single) {
            (SectionPreset::Organ(o), _) => {
                self.apply_organ_section(o);
                let on = u8::from(o.layer_a_enabled) | (u8::from(o.layer_b_enabled) << 1);
                self.set_preset_scene(ORGAN_BITS, on, false);
            }
            (SectionPreset::Piano(p), Some(id @ (LayerId::PianoA | LayerId::PianoB))) => {
                self.apply_piano_layer(id == LayerId::PianoB, &p.layer_a, &p.fx_a);
                self.set_preset_scene(PIANO_BITS, 1 << (id as u8), true);
            }
            (SectionPreset::Piano(p), _) => {
                self.apply_piano_section(p);
                let on = (u8::from(p.layer_a.enabled) << 2) | (u8::from(p.layer_b.enabled) << 3);
                self.set_preset_scene(PIANO_BITS, on, false);
            }
            (
                SectionPreset::Synth(s),
                Some(id @ (LayerId::SynthA | LayerId::SynthB | LayerId::SynthC)),
            ) => {
                let layer = usize::from(id as u8) - 4;
                self.apply_synth_layer(layer, &s.layer_a, &s.fx_a);
                self.set_preset_scene(SYNTH_BITS, 1 << (id as u8), true);
            }
            (SectionPreset::Synth(s), _) => {
                self.apply_synth_section(s);
                let on = (u8::from(s.layer_a.enabled) << 4)
                    | (u8::from(s.layer_b.enabled) << 5)
                    | (u8::from(s.layer_c.enabled) << 6);
                self.set_preset_scene(SYNTH_BITS, on, false);
            }
        }
    }

    /// [`Self::load_section_preset`] as an undoable "Load Preset" (UNDO, manual p.43).
    pub fn load_section_preset_undoable(
        &mut self,
        preset: &SectionPreset,
        single: Option<LayerId>,
    ) {
        self.begin_undoable(UndoOp::LoadPreset);
        self.load_section_preset(preset, single);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::PresetSection;
    use crate::organ::OrganModel;
    use crate::piano::PianoCategory;

    fn organ_preset() -> SectionPreset {
        let mut o = OrganPresetDecoded::default();
        o.model = OrganModel::Vox;
        o.drawbars_a = [1, 2, 3, 4, 5, 6, 7, 8, 0];
        o.layer_a_level = 0.5;
        o.rotary_drive = 5.0;
        SectionPreset::Organ(Box::new(o))
    }

    #[test]
    fn organ_preset_sets_model_drawbars_level_and_scene() {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_2_mask = 0b111_1111;
        e.load_section_preset(&organ_preset(), None);
        assert_eq!(e.organ_a.model, OrganModel::Vox);
        assert_eq!(e.organ_a.drawbars, [1, 2, 3, 4, 5, 6, 7, 8, 0]);
        assert!((e.layer_levels[0].base - 0.5).abs() < 1e-6);
        assert!((e.organ_a.rotary_drive - 0.5).abs() < 1e-6);
        assert_eq!(e.scene.scene_1_mask & ORGAN_BITS, 0b01, "layer A on, B off");
        assert_eq!(
            e.scene.scene_2_mask & ORGAN_BITS,
            0,
            "off in the other scene"
        );
        assert_ne!(e.scene.scene_1_mask & !ORGAN_BITS, 0, "other sections stay");
    }

    #[test]
    fn single_layer_piano_goes_to_the_chosen_layer_and_keeps_the_rest() {
        let mut p = PianoPresetDecoded::default();
        p.layer_a.category = PianoCategory::Clav;
        p.layer_a.level = 0.25;
        let preset = SectionPreset::Piano(Box::new(p));
        let mut e = Stage4Engine::new(48_000.0);
        e.piano_a.category = PianoCategory::Digital;
        e.scene.scene_2_mask = 0b111_1111;
        e.load_section_preset(&preset, Some(LayerId::PianoB));
        assert_eq!(e.piano_b.category, PianoCategory::Clav);
        assert_eq!(
            e.piano_a.category,
            PianoCategory::Digital,
            "layer A untouched"
        );
        assert!((e.layer_levels[3].base - 0.25).abs() < 1e-6);
        assert_ne!(
            e.scene.scene_1_mask & (1 << 3),
            0,
            "target layer switched on"
        );
        assert_ne!(
            e.scene.scene_2_mask & PIANO_BITS,
            0,
            "other scene left alone"
        );
        e.load_section_preset(&preset, None);
        assert_eq!(
            e.piano_a.category,
            PianoCategory::Clav,
            "section load sets A"
        );
        assert_eq!(e.scene.scene_2_mask & PIANO_BITS, 0);
    }

    #[test]
    fn synth_preset_sets_every_layer_and_the_active_scene_only() {
        let mut s = SynthPresetDecoded::default();
        s.layer_a.level = 0.4;
        s.layer_b.level = 0.6;
        s.layer_b.enabled = true;
        let preset = SectionPreset::Synth(Box::new(s));
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_2_active = true;
        e.load_section_preset(&preset, None);
        assert!((e.layer_levels[4].base - 0.4).abs() < 1e-6);
        assert!((e.layer_levels[5].base - 0.6).abs() < 1e-6);
        assert_ne!(e.scene.scene_2_mask & (1 << 5), 0, "active scene II");
        assert_eq!(e.scene.scene_1_mask & SYNTH_BITS, 0, "scene I off");
        e.scene.scene_2_active = false;
        e.load_section_preset(&preset, Some(LayerId::SynthC));
        assert_ne!(e.scene.scene_1_mask & (1 << 6), 0);
    }

    #[test]
    fn undoable_load_restores_the_previous_state() {
        let mut e = Stage4Engine::new(48_000.0);
        let before = e.organ_a.drawbars;
        e.load_section_preset_undoable(&organ_preset(), None);
        assert_ne!(e.organ_a.drawbars, before);
        assert!(e.undo().is_some());
        assert_eq!(e.organ_a.drawbars, before);
    }

    #[test]
    fn init_presets_load_without_panicking_for_every_section() {
        let mut e = Stage4Engine::new(48_000.0);
        for s in PresetSection::ALL {
            e.load_section_preset(&SectionPreset::init(s), None);
        }
    }
}
