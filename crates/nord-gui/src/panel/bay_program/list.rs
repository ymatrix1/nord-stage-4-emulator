//! LIST view and Preset Library browsing: cursor, letter / category jumps, organ / piano / sample presets.

use super::catalog::{display_name, LoadRequest};
use super::model::PresetKind;
use super::nav::ORGAN_PRESETS;
use crate::audio::AudioCommand;
use crate::panel::ui_state::ProgramUi;
use nord_core::morph::LayerId;

impl ProgramUi {
    /// Cursor position of the current program in the LIST order.
    pub fn sync_list_cursor(&mut self) {
        let order = self.catalog.order(self.list_mode);
        let here = usize::from(self.loc.index());
        self.list_cursor = order.iter().position(|&i| i == here).unwrap_or(0);
    }

    pub(super) fn list_dial(&mut self, delta: i32) -> Vec<AudioCommand> {
        let order = self.catalog.order(self.list_mode);
        if order.is_empty() {
            self.set_message("No backup library open");
            return Vec::new();
        }
        let step = if self.list_letter_held || self.list_cat_held {
            self.jump_size(&order, delta)
        } else {
            delta
        };
        let last = order.len() as i32 - 1;
        self.list_cursor = (self.list_cursor as i32 + step).clamp(0, last) as usize;
        self.list_load_due = Some((self.now_ms.saturating_add(200), order[self.list_cursor]));
        Vec::new()
    }

    /// Steps to the first entry of the next/previous letter (Abc) or category (Cat).
    fn jump_size(&self, order: &[usize], delta: i32) -> i32 {
        let key = |i: usize| -> String {
            let e = &self.catalog.programs[order[i]];
            if self.list_cat_held {
                super::catalog::category_of(e).to_lowercase()
            } else {
                display_name(e)
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_lowercase().to_string())
                    .unwrap_or_default()
            }
        };
        let start = self.list_cursor.min(order.len() - 1);
        let base = key(start);
        let mut i = start as i32;
        while (0..order.len() as i32).contains(&(i + delta.signum())) {
            i += delta.signum();
            if key(i as usize) != base {
                break;
            }
        }
        i - start as i32
    }

    pub(super) fn preset_dial(&mut self, kind: PresetKind, delta: i32) -> Vec<AudioCommand> {
        let n = match kind {
            PresetKind::Organ => ORGAN_PRESETS.len(),
            PresetKind::Piano => self.catalog.pianos.len(),
            PresetKind::Synth => self.catalog.samples.len(),
        };
        if n == 0 {
            self.set_message("No backup library open");
            return Vec::new();
        }
        self.preset_cursor = (self.preset_cursor as i32 + delta).clamp(0, n as i32 - 1) as usize;
        match kind {
            PresetKind::Organ => {
                let m = ORGAN_PRESETS[self.preset_cursor];
                self.organ_request = Some(m);
                vec![AudioCommand::SetOrganModel(m)]
            }
            PresetKind::Piano => {
                let entry = self.catalog.pianos[self.preset_cursor].clone();
                let slot_b = self.focus == LayerId::PianoB;
                self.requests.push(LoadRequest::Piano { entry, slot_b });
                Vec::new()
            }
            PresetKind::Synth => {
                let entry = self.catalog.samples[self.preset_cursor].clone();
                let layer = match self.focus {
                    LayerId::SynthB => 1,
                    LayerId::SynthC => 2,
                    _ => 0,
                };
                self.requests.push(LoadRequest::Sample { entry, layer });
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{ListMode, Screen};
    use super::*;

    #[test]
    fn list_cursor_follows_the_current_program_and_clamps() {
        let mut u = ProgramUi::default();
        let names: Vec<String> = (0..5).map(|i| format!("P/p{i}.ns4p")).collect();
        u.sync_catalog(&names, &[], &[]);
        u.loc = super::super::store::ProgLoc::from_index(3);
        u.sync_list_cursor();
        assert_eq!(u.list_cursor, 3);
        u.screen = Screen::List;
        u.dial(100, false);
        assert_eq!(u.list_cursor, 4);
        assert_eq!(u.list_mode, ListMode::Num);
    }

    #[test]
    fn preset_browser_reports_when_no_backup_is_open() {
        let mut u = ProgramUi::default();
        u.screen = Screen::Preset {
            kind: PresetKind::Synth,
            single: false,
        };
        assert!(u.dial(1, false).is_empty());
        assert!(u.message.is_some());
        assert!(u.requests.is_empty());
        assert_eq!(display_name("S/a.nsmp4"), "a");
    }
}
