//! STORE of a Preset Library entry (manual OS v1.6x p.42): the panel state of one section as a
//! preset.
//!
//! The preset is the section block of `ProgramState::export_into` (the fields the engine maps:
//! Organ model, drawbars, percussion, rotary, octaves, levels; Piano type, timbre, touch,
//! resonance, noise, release, unison, compression, octave, levels; Synth octave and levels) on
//! top of a base preset, plus the Layer On/Off flags of the active Layer Scene. Everything the
//! engine does not map back (effect chains, the Synth sound itself, reserved bits) comes from
//! the base: the preset the section was loaded from, or the init preset. `ASSUMED` limitation,
//! shared with stored programs (`docs/specs/cbin-and-presets.md` section 5.5); the open item is
//! the inverse of `synth_params::apply_decoded_layer` (OQ28).

use super::{ProgramState, Stage4Engine};
use crate::cbin::{PresetSection, ProgramDecoded, SectionPreset};

impl ProgramState {
    /// Layer `idx` (`0..7`) is on in the Layer Scene that is playing.
    fn layer_on(&self, idx: u8) -> bool {
        let mask = if self.scene.scene_2_active {
            self.scene.scene_2_mask
        } else {
            self.scene.scene_1_mask
        };
        mask & (1 << idx) != 0
    }

    /// The `section` preset of this state, patched onto `base` (see the module docs).
    pub fn capture_section_preset(
        &self,
        section: PresetSection,
        base: Option<&SectionPreset>,
    ) -> SectionPreset {
        let mut prog = ProgramDecoded::default();
        match base {
            Some(SectionPreset::Organ(o)) if section == PresetSection::Organ => {
                prog.organ = (**o).clone();
            }
            Some(SectionPreset::Piano(p)) if section == PresetSection::Piano => {
                prog.piano = (**p).clone();
            }
            Some(SectionPreset::Synth(s)) if section == PresetSection::Synth => {
                prog.synth = (**s).clone();
            }
            _ => {}
        }
        self.export_into(&mut prog);
        match section {
            PresetSection::Organ => {
                prog.organ.layer_a_enabled = self.layer_on(0);
                prog.organ.layer_b_enabled = self.layer_on(1);
                SectionPreset::Organ(Box::new(prog.organ))
            }
            PresetSection::Piano => {
                prog.piano.layer_a.enabled = self.layer_on(2);
                prog.piano.layer_b.enabled = self.layer_on(3);
                SectionPreset::Piano(Box::new(prog.piano))
            }
            PresetSection::Synth => {
                prog.synth.layer_a.enabled = self.layer_on(4);
                prog.synth.layer_b.enabled = self.layer_on(5);
                prog.synth.layer_c.enabled = self.layer_on(6);
                SectionPreset::Synth(Box::new(prog.synth))
            }
        }
    }
}

impl Stage4Engine {
    /// The `section` preset of the current panel state (allocates; control thread only).
    pub fn capture_section_preset(
        &self,
        section: PresetSection,
        base: Option<&SectionPreset>,
    ) -> SectionPreset {
        self.capture_state().capture_section_preset(section, base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organ::OrganModel;

    #[test]
    fn a_stored_organ_preset_reloads_into_the_same_panel_state() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.model = OrganModel::Farfisa;
        e.organ_a.drawbars = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        e.layer_levels[0].base = 0.5;
        e.scene.scene_1_mask |= 1 << 1; // layer B on
        let preset = e.capture_section_preset(PresetSection::Organ, None);
        let SectionPreset::Organ(o) = &preset else {
            panic!("organ expected")
        };
        assert_eq!(o.model, OrganModel::Farfisa);
        assert!(o.layer_a_enabled && o.layer_b_enabled);
        let mut fresh = Stage4Engine::new(48_000.0);
        fresh.load_section_preset(&preset, None);
        assert_eq!(fresh.organ_a.model, OrganModel::Farfisa);
        assert_eq!(fresh.organ_a.drawbars, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert!((fresh.layer_levels[0].base - 0.5).abs() < 0.01);
    }

    #[test]
    fn the_base_supplies_what_the_engine_does_not_map() {
        let mut base = crate::cbin::SynthPresetDecoded::default();
        base.layer_a.filter_cutoff_hz = 1234.0;
        base.layer_a.level = 0.9;
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[4].base = 0.3;
        let preset = e.capture_section_preset(
            PresetSection::Synth,
            Some(&SectionPreset::Synth(Box::new(base))),
        );
        let SectionPreset::Synth(s) = preset else {
            panic!("synth expected")
        };
        assert_eq!(s.layer_a.filter_cutoff_hz, 1234.0, "from the base");
        assert!((s.layer_a.level - 0.3).abs() < 1e-6, "from the panel");
    }

    #[test]
    fn a_base_of_another_section_is_ignored_and_piano_flags_follow_the_scene() {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_2_active = true;
        e.scene.scene_2_mask = 1 << 3;
        let wrong = SectionPreset::init(PresetSection::Organ);
        let SectionPreset::Piano(p) = e.capture_section_preset(PresetSection::Piano, Some(&wrong))
        else {
            panic!("piano expected")
        };
        assert!(!p.layer_a.enabled && p.layer_b.enabled);
    }
}
