//! Panel controls after a Preset Library load: the display state of one section follows the
//! preset, as `Stage4PanelState::apply_program` does for a whole program.
//!
//! The engine side is `Stage4Engine::load_section_preset` (`ProgramEdit::LoadPreset`).

use super::state::Stage4PanelState;
use nord_core::cbin::SectionPreset;
use nord_core::morph::LayerId;

impl Stage4PanelState {
    /// Mirrors a loaded preset onto the panel. `single` is the layer a single-layer preset went
    /// to (Piano A / B, Synth A / B / C); the shared panel controls follow layer A of a section.
    pub fn apply_section_preset(&mut self, preset: &SectionPreset, single: Option<LayerId>) {
        match preset {
            SectionPreset::Organ(o) => {
                self.organ_a_active = o.layer_a_enabled;
                self.organ_b_active = o.layer_b_enabled;
                self.organ_a_level = o.layer_a_level;
                self.organ_b_level = o.layer_b_level;
                self.organ_model = o.model;
                self.organ_drawbars = o.drawbars_a;
                self.scanner_on = o.vibrato_on;
            }
            SectionPreset::Piano(p) => {
                match single {
                    Some(LayerId::PianoB) => {
                        self.piano_b_active = true;
                        self.piano_b_level = p.layer_a.level;
                        return;
                    }
                    Some(_) => self.piano_a_active = true,
                    None => {
                        self.piano_a_active = p.layer_a.enabled;
                        self.piano_b_active = p.layer_b.enabled;
                        self.piano_b_level = p.layer_b.level;
                    }
                }
                self.piano_a_level = p.layer_a.level;
                self.piano_cat = p.layer_a.category;
                self.piano_timbre = p.layer_a.timbre;
                self.piano_touch = p.layer_a.touch;
                self.string_res = p.layer_a.string_res;
                self.pedal_noise = p.layer_a.pedal_noise;
            }
            SectionPreset::Synth(s) => {
                let layers = [
                    (&s.layer_a, LayerId::SynthA),
                    (&s.layer_b, LayerId::SynthB),
                    (&s.layer_c, LayerId::SynthC),
                ];
                for (i, (dec, id)) in layers.into_iter().enumerate() {
                    let target = single.is_none() || single == Some(id);
                    if !target {
                        continue;
                    }
                    let on = single.is_some() || dec.enabled;
                    let (active, level) = match i {
                        0 => (&mut self.synth_a_active, &mut self.synth_a_level),
                        1 => (&mut self.synth_b_active, &mut self.synth_b_level),
                        _ => (&mut self.synth_c_active, &mut self.synth_c_level),
                    };
                    *active = on;
                    *level = dec.level;
                }
                // A single-layer load takes layer A of the file; the section controls follow A.
                if single.is_none() || single == Some(LayerId::SynthA) {
                    self.synth_osc_mode = s.layer_a.osc_mode;
                    self.synth_waveform = s.layer_a.waveform;
                    self.synth_filter_model = s.layer_a.filter_model;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::cbin::{PianoPresetDecoded, SynthPresetDecoded};
    use nord_core::organ::OrganModel;
    use nord_core::piano::PianoCategory;

    #[test]
    fn organ_preset_sets_model_bars_and_layer_flags() {
        let mut o = nord_core::cbin::OrganPresetDecoded::default();
        o.model = OrganModel::Vox;
        o.drawbars_a = [1; 9];
        o.layer_b_enabled = true;
        let mut p = Stage4PanelState::default();
        p.apply_section_preset(&SectionPreset::Organ(Box::new(o)), None);
        assert_eq!(p.organ_model, OrganModel::Vox);
        assert_eq!(p.organ_drawbars, [1; 9]);
        assert!(p.organ_a_active && p.organ_b_active);
    }

    #[test]
    fn single_piano_b_leaves_the_shared_controls_alone() {
        let mut d = PianoPresetDecoded::default();
        d.layer_a.category = PianoCategory::Clav;
        d.layer_a.level = 0.3;
        let preset = SectionPreset::Piano(Box::new(d));
        let mut p = Stage4PanelState::default();
        p.piano_cat = PianoCategory::Grand;
        p.apply_section_preset(&preset, Some(LayerId::PianoB));
        assert_eq!(p.piano_cat, PianoCategory::Grand);
        assert!(p.piano_b_active && (p.piano_b_level - 0.3).abs() < 1e-6);
        p.apply_section_preset(&preset, None);
        assert_eq!(p.piano_cat, PianoCategory::Clav);
    }

    #[test]
    fn single_synth_c_sets_only_that_layer() {
        let mut d = SynthPresetDecoded::default();
        d.layer_a.level = 0.4;
        let preset = SectionPreset::Synth(Box::new(d));
        let mut p = Stage4PanelState::default();
        let a_level = p.synth_a_level;
        p.apply_section_preset(&preset, Some(LayerId::SynthC));
        assert!(p.synth_c_active && (p.synth_c_level - 0.4).abs() < 1e-6);
        assert_eq!(p.synth_a_level, a_level);
    }
}
