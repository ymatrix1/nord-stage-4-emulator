//! STORE of a section preset into the Preset Library (manual OS v1.6x p.42).
//!
//! Flow: STORE, then the ORGAN / PIANO / SYNTH button of the Preset Library ("Store Preset To"),
//! the dial and PAGE / BANK buttons pick the location, STORE confirms. Also: a Preset Library
//! button first, then STORE. STORE AS names the preset first, like a program. Memory Protect
//! blocks the write like it does for programs.
//!
//! The preset is taken from the DSP thread as a program snapshot (`ProgramEdit::Snapshot`) and
//! kept in the in-memory library (`preset_user.rs`); Export backup writes it into a copy of the
//! `.ns4b`. The capacity of a section is `PresetSection::capacity` (ASSUMED, see
//! `backup_presets.rs`); the display text of the screen is ASSUMED too: only the title "Store
//! Preset To" is in the manual.

use super::catalog::display_name;
use super::model::{PresetKind, Screen};
use super::name::NameEditor;
use super::nav::cmd;
use super::oled::OledText;
use super::organize_presets::preset_label;
use super::preset_user::UserPreset;
use super::store::PendingSnapshot;
use crate::audio::AudioCommand;
use crate::panel::ui_state::ProgramUi;
use nord_core::engine::ProgramState;

/// Locations per page of the Preset Library (the PAGE buttons step by one page).
const PAGE: i32 = 8;
/// Locations per bank.
const BANK: i32 = 64;

fn kind_word(kind: PresetKind) -> &'static str {
    match kind {
        PresetKind::Organ => "Organ",
        PresetKind::Piano => "Piano",
        PresetKind::Synth => "Synth",
    }
}

impl ProgramUi {
    /// A Preset Library button was pressed while the Store location is being chosen.
    pub(super) fn preset_store_begin(&mut self, kind: PresetKind) -> Vec<AudioCommand> {
        let section = kind.section();
        let loc = self.library.presets.store_start(section);
        // Without a typed name the preset keeps the name of the preset it was loaded from.
        if self.store.editor.name() == self.title {
            let loaded = self.library.presets.loaded(section);
            if let Some(e) = loaded {
                self.store.editor = NameEditor::new(&display_name(e));
            }
        }
        self.store.preset = Some((kind, loc));
        self.screen = Screen::StoreTo;
        Vec::new()
    }

    /// Moves the target location by `delta` (clamped to the section).
    fn preset_store_move(&mut self, delta: i32) {
        if let Some((kind, loc)) = self.store.preset {
            let last = i32::from(kind.section().capacity()) - 1;
            let next = (i32::from(loc) + delta).clamp(0, last);
            self.store.preset = Some((kind, next as u16));
        }
    }

    /// PROGRAM dial.
    pub(super) fn preset_store_step(&mut self, delta: i32) {
        self.preset_store_move(delta);
    }

    /// PAGE (`bank = false`) or BANK buttons.
    pub(super) fn preset_store_page(&mut self, delta: i32, bank: bool) {
        self.preset_store_move(delta.signum() * if bank { BANK } else { PAGE });
    }

    /// Program button `n` (1..=8): that position of the page the target is on.
    pub(super) fn preset_store_button(&mut self, n: u8) {
        if let Some((kind, loc)) = self.store.preset {
            if (1..=8).contains(&n) {
                self.store.preset = Some((kind, loc / 8 * 8 + u16::from(n) - 1));
            }
        }
    }

    /// STORE pressed again: asks the DSP thread for the panel state.
    pub(super) fn preset_store_confirm(&mut self) -> Vec<AudioCommand> {
        let Some((kind, loc)) = self.store.preset else {
            return Vec::new();
        };
        let out = if self.store_blocked() {
            Vec::new()
        } else {
            let typed = self.store.editor.name();
            let name = if typed.is_empty() {
                kind_word(kind).to_string()
            } else {
                typed
            };
            self.set_message(format!("Stored {} {name}", preset_label(loc)));
            vec![cmd(self.request_snapshot(PendingSnapshot::Preset {
                kind,
                loc,
                name,
            }))]
        };
        self.store.cancel();
        self.screen = Screen::Home;
        out
    }

    /// The snapshot of a preset STORE arrived: the preset goes into the library.
    pub(super) fn accept_preset_snapshot(
        &mut self,
        kind: PresetKind,
        loc: u16,
        name: String,
        state: Box<ProgramState>,
    ) {
        let section = kind.section();
        let presets = &mut self.library.presets;
        let base = presets.loaded(section).and_then(|e| presets.base_entry(e));
        let preset = UserPreset {
            name,
            section,
            state,
            base,
        };
        presets.store_user(loc, preset);
        self.organize.preset.reset(); // the cells changed under the Undo snapshot
    }

    /// "Store Preset To" screen, while a Preset Library button picked the section.
    pub fn preset_store_text(&self) -> Option<OledText> {
        let (kind, loc) = self.store.preset?;
        let section = kind.section();
        let existing = self
            .library
            .presets
            .name_at(section, loc)
            .unwrap_or_else(|| super::nav::EMPTY_NAME.to_string());
        Some(OledText {
            big: "STORE PRESET TO".into(),
            sub: format!("{} {}", kind_word(kind), preset_label(loc)),
            rows: vec![existing, format!("As: {}", self.store.editor.name())],
            ..Default::default()
        })
    }
}

#[cfg(test)]
#[path = "preset_store_tests.rs"]
mod more_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_text_names_the_section_location_and_the_name_to_store() {
        let mut u = ProgramUi::default();
        assert!(u.preset_store_text().is_none());
        u.store.preset = Some((PresetKind::Organ, 64));
        let t = u.preset_store_text().unwrap();
        assert_eq!(
            (t.big.as_str(), t.sub.as_str()),
            ("STORE PRESET TO", "Organ 2:01")
        );
        assert_eq!(t.rows[0], super::super::nav::EMPTY_NAME);
        assert_eq!(kind_word(PresetKind::Piano), "Piano");
    }
}
