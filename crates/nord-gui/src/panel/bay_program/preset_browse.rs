//! Preset Library browser over the real presets of the opened backup (manual OS v1.6x p.41-42).
//!
//! The PROGRAM dial walks the occupied locations of the section in order (Num); every step asks
//! the library to load that entry (`LoadRequest::Preset`). Without a backup that holds presets the
//! browser keeps the earlier behaviour (Organ models, `.npno` models, samples): see `list.rs`.

use super::catalog::LoadRequest;
use super::model::{PresetKind, Screen};
use super::organize_presets::preset_label;
use crate::panel::ui_state::ProgramUi;
use nord_core::cbin::PresetSection;
use nord_core::morph::LayerId;

impl PresetKind {
    /// The Preset Library section of this browser.
    pub fn section(self) -> PresetSection {
        match self {
            Self::Organ => PresetSection::Organ,
            Self::Piano => PresetSection::Piano,
            Self::Synth => PresetSection::Synth,
        }
    }
}

impl ProgramUi {
    /// Layer the open Single Layer browser loads into (`None` for a whole-section browser).
    fn single_target(&self, kind: PresetKind) -> Option<LayerId> {
        match (self.screen, kind) {
            (Screen::Preset { single: true, .. }, PresetKind::Piano | PresetKind::Synth) => {
                Some(self.focus)
            }
            _ => None,
        }
    }

    /// The preset browser of `kind` lists real presets (the backup holds some for the section).
    pub(super) fn has_presets(&self, kind: PresetKind) -> bool {
        !self.library.presets.occupied(kind.section()).is_empty()
    }

    /// Cursor position when the browser opens: the loaded preset, else the first one.
    pub(super) fn preset_start_cursor(&self, kind: PresetKind) -> usize {
        let section = kind.section();
        let lib = &self.library.presets;
        let cells = lib.occupied(section);
        lib.loaded(section)
            .and_then(|e| cells.iter().position(|&l| lib.entry_at(section, l) == Some(e)))
            .unwrap_or(0)
    }

    /// Dial step in the preset browser: moves the cursor and asks for the preset under it.
    pub(super) fn preset_library_step(&mut self, kind: PresetKind, delta: i32) {
        let section = kind.section();
        let cells = self.library.presets.occupied(section);
        let last = cells.len().saturating_sub(1) as i32;
        self.preset_cursor = (self.preset_cursor as i32 + delta).clamp(0, last) as usize;
        let loc = cells[self.preset_cursor.min(cells.len() - 1)];
        let Some(entry) = self.library.presets.entry_at(section, loc).map(String::from) else {
            return;
        };
        let name = self.library.presets.name_at(section, loc).unwrap_or_default();
        self.set_message(format!("{} {name}", preset_label(loc)));
        self.library.presets.mark_loaded(section, &entry);
        let single = self.single_target(kind);
        self.requests.push(LoadRequest::Preset {
            section,
            entry,
            single,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::cbin::PresetSection::{Organ, Synth};

    fn ui() -> ProgramUi {
        let mut u = ProgramUi::default();
        u.library.presets.set_factory(
            Synth,
            &[
                (0, "Synth Preset/Bank 1/Alpha.ns4y".into()),
                (1, "Synth Preset/Bank 1/Beta.ns4y".into()),
                (70, "Synth Preset/Bank 2/Gamma.ns4y".into()),
            ],
        );
        u
    }

    #[test]
    fn sections_map_and_empty_sections_use_the_model_browser() {
        let u = ui();
        assert_eq!(PresetKind::Piano.section(), PresetSection::Piano);
        assert!(u.has_presets(PresetKind::Synth) && !u.has_presets(PresetKind::Organ));
        assert_eq!(u.library.presets.count(Organ), 0);
    }

    #[test]
    fn the_dial_walks_occupied_locations_and_requests_each_entry() {
        let mut u = ui();
        u.screen = Screen::Preset {
            kind: PresetKind::Synth,
            single: false,
        };
        u.preset_library_step(PresetKind::Synth, 1);
        u.preset_library_step(PresetKind::Synth, 5); // clamps to the last location (bank 2, 71)
        let reqs = u.take_requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(
            reqs[0],
            LoadRequest::Preset {
                section: Synth,
                entry: "Synth Preset/Bank 1/Beta.ns4y".into(),
                single: None
            }
        );
        assert!(matches!(&reqs[1], LoadRequest::Preset { entry, .. } if entry.ends_with("Gamma.ns4y")));
        assert_eq!(u.message.as_ref().unwrap().0, "2:07 Gamma");
        assert_eq!(u.library.presets.loaded(Synth), Some("Synth Preset/Bank 2/Gamma.ns4y"));
        assert_eq!(u.preset_start_cursor(PresetKind::Synth), 2);
    }

    #[test]
    fn single_layer_browsing_targets_the_focused_layer() {
        let mut u = ui();
        u.focus = LayerId::SynthC;
        u.screen = Screen::Preset {
            kind: PresetKind::Synth,
            single: true,
        };
        u.preset_library_step(PresetKind::Synth, 1);
        assert!(matches!(
            u.take_requests()[0],
            LoadRequest::Preset {
                single: Some(LayerId::SynthC),
                ..
            }
        ));
        u.screen = Screen::Preset {
            kind: PresetKind::Organ,
            single: true,
        };
        assert_eq!(u.single_target(PresetKind::Organ), None);
    }
}
