//! Program navigation: dial, PAGE/BANK, Program buttons 1-8, soft buttons, Live Mode, NUM PAD.
//!
//! Every method returns the `AudioCommand`s the panel must send; none touches the engine.

use super::catalog::{display_name, LoadRequest};
use super::model::{ListMode, Screen};
use super::store::{PendingSnapshot, ProgLoc, StoreStage, StoreTarget};
use crate::audio::{AudioCommand, ProgramEdit};
use crate::panel::ui_state::ProgramUi;
use nord_core::engine::Section;
use nord_core::morph::LayerId;
use nord_core::organ::OrganModel;

/// Organ presets: the five organ models (the engine's only organ "sounds").
pub const ORGAN_PRESETS: [OrganModel; 5] = [
    OrganModel::B3,
    OrganModel::Vox,
    OrganModel::Farfisa,
    OrganModel::Pipe1,
    OrganModel::Pipe2,
];

/// The layer a swap pairs with `id` (same section, next layer, wraps).
pub fn sibling(id: LayerId) -> LayerId {
    let layers = Section::of(id).layers();
    let i = layers.iter().position(|&l| l == id).unwrap_or(0);
    layers[(i + 1) % layers.len()]
}

/// Next focus layer over all seven layers.
fn next_layer(id: LayerId, delta: i32) -> LayerId {
    let i = LayerId::ALL.iter().position(|&l| l == id).unwrap_or(0) as i32;
    LayerId::ALL[(i + delta).rem_euclid(7) as usize]
}

pub(super) fn cmd(e: ProgramEdit) -> AudioCommand {
    AudioCommand::Program(e)
}

/// Name the display shows for an unused program location.
///
/// VERIFIED: `G:82` shows `<empty>` in the DSoundman beginners video (`9OQdETpuIE8`, 19:35).
pub const EMPTY_NAME: &str = "<empty>";

impl ProgramUi {
    /// Load the program at `loc`: a stored user program, else the backup-library entry at the
    /// same position (ASSUMED mapping: location index = position in the sorted archive list).
    pub fn select_loc(&mut self, loc: ProgLoc) -> Vec<AudioCommand> {
        self.loc = loc;
        self.browse = loc;
        self.preset_name = false; // manual p.42: reset when a Program is loaded
        self.mark_clean();
        self.extern_led_until = self.now_ms.saturating_add(700);
        if let Some(p) = self.library.get(loc) {
            self.title = p.name.clone();
            return vec![cmd(ProgramEdit::Recall(p.state.clone()))];
        }
        // The archive layout (header locations, moved by Organize) wins over the sorted catalog.
        let entry = if self.library.has_factory_layout() {
            self.library.factory_at(loc)
        } else {
            self.catalog
                .programs
                .get(usize::from(loc.index()))
                .map(String::as_str)
        };
        match entry {
            Some(entry) => {
                self.title = display_name(entry);
                self.requests.push(LoadRequest::Program(entry.to_string()));
            }
            None => self.title = EMPTY_NAME.to_string(),
        }
        Vec::new()
    }

    /// Dial turned by `delta`; `shift` is Shift+dial (LIST view).
    pub fn dial(&mut self, delta: i32, shift: bool) -> Vec<AudioCommand> {
        if delta == 0 {
            return Vec::new();
        }
        if self.morph.assigning().is_some() {
            return self.morph.dial(delta).map(cmd).into_iter().collect();
        }
        if shift && self.screen == Screen::Home {
            self.screen = Screen::List;
            self.sync_list_cursor();
        }
        match self.screen {
            Screen::Home | Screen::NumPad => self.select_loc(self.loc.step(delta)),
            Screen::List => self.list_dial(delta),
            Screen::StoreTo => {
                self.store.target = match self.store.target {
                    Some(StoreTarget::Live(s)) => {
                        Some(StoreTarget::Live((i32::from(s) + delta).rem_euclid(8) as u8))
                    }
                    Some(StoreTarget::Bank(l)) => Some(StoreTarget::Bank(l.step(delta))),
                    None => None,
                };
                Vec::new()
            }
            Screen::StoreAs => {
                self.store.editor.dial(delta);
                Vec::new()
            }
            Screen::Split => {
                self.split.dial(delta);
                vec![cmd(ProgramEdit::Split(self.split.cfg()))]
            }
            Screen::Clock => vec![cmd(ProgramEdit::MasterBpm(self.clock.dial(delta)))],
            Screen::Transpose => {
                self.transp.dial(delta);
                vec![cmd(ProgramEdit::Transpose(self.transp.effective()))]
            }
            Screen::Preset { kind, .. } => self.preset_dial(kind, delta),
            // Shift + dial on Preset Organize picks the Preset Library section.
            Screen::Menu(_) if shift && self.on_preset_organize_page() => {
                self.step_organize_section(delta);
                Vec::new()
            }
            Screen::Menu(_) => self.menu_dial(delta),
            _ => Vec::new(),
        }
    }

    /// PAGE (`bank = false`) or BANK (`bank = true`, Shift+Page) by `delta`.
    pub fn page(&mut self, delta: i32, bank: bool) -> Vec<AudioCommand> {
        if self.morph.assigning().is_some() {
            self.morph.page(delta);
            return Vec::new();
        }
        let step = |l: ProgLoc| {
            if bank {
                l.step_bank(delta)
            } else {
                l.step_page(delta)
            }
        };
        match self.screen {
            Screen::Copy
            | Screen::Paste
            | Screen::LayerInit
            | Screen::Preset { single: true, .. } => {
                self.focus = next_layer(self.focus, delta);
            }
            Screen::List if self.list_mode == ListMode::Cat => {
                self.list_cat_held = true;
                let out = self.list_dial(delta);
                self.list_cat_held = false;
                return out;
            }
            Screen::Menu(_) => self.menu.step_page(delta),
            Screen::StoreTo => {
                if let Some(StoreTarget::Bank(l)) = self.store.target {
                    self.store.target = Some(StoreTarget::Bank(step(l)));
                }
            }
            _ => self.browse = step(self.browse),
        }
        Vec::new()
    }

    /// Program button `n` (1..=8) as a program / slot selector (no soft-button screen open).
    pub fn program_button(&mut self, n: u8) -> Vec<AudioCommand> {
        if self.live_armed {
            return self.live_select(n);
        }
        if self.num_pad {
            return self.num_digit(n);
        }
        if self.store.stage == StoreStage::Choosing {
            self.store.target = match self.store.target {
                Some(StoreTarget::Live(_)) => Some(StoreTarget::Live(n - 1)),
                Some(StoreTarget::Bank(l)) => Some(StoreTarget::Bank(l.with_button(n))),
                None => Some(StoreTarget::Bank(self.browse.with_button(n))),
            };
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.live.is_active() {
            self.live.edited();
            if let Some(slot) = self.live.leave() {
                out.push(cmd(self.request_snapshot(PendingSnapshot::Live(slot))));
            }
        }
        self.screen = Screen::Home;
        out.extend(self.select_loc(self.browse.with_button(n)));
        out
    }

    /// Live Mode: pick slot `n` (1..=8), saving the slot being left.
    fn live_select(&mut self, n: u8) -> Vec<AudioCommand> {
        self.live_armed = false;
        self.live.edited(); // ASSUMED: bays do not report edits yet, so every switch saves
        let mut out = Vec::new();
        if let Some(slot) = self.live.select(n) {
            out.push(cmd(self.request_snapshot(PendingSnapshot::Live(slot))));
        }
        self.screen = Screen::Home;
        self.title = format!("Live {n}");
        if let Some(state) = self.library.live(n - 1) {
            out.push(cmd(ProgramEdit::Recall(Box::new(state.clone()))));
        }
        out
    }

    /// NUM PAD: first digit picks the page, second the program (manual p.44).
    fn num_digit(&mut self, n: u8) -> Vec<AudioCommand> {
        match self.num_first.take() {
            None => {
                self.num_first = Some(n);
                Vec::new()
            }
            Some(page) => {
                let loc = ProgLoc {
                    bank: self.browse.bank,
                    page: page - 1,
                    slot: n - 1,
                };
                self.select_loc(loc)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_ui(n: usize) -> ProgramUi {
        let mut u = ProgramUi::default();
        let names: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    "Program/Bank {}/P{:03}.ns4p",
                    (b'A' + (i / 64) as u8) as char,
                    i
                )
            })
            .collect();
        u.sync_catalog(&names, &["Piano/a.npno".into()], &["Samp/b.nsmp4".into()]);
        u
    }

    #[test]
    fn program_buttons_select_slots_of_the_browsed_page_and_request_the_entry() {
        let mut u = catalog_ui(128);
        u.page(1, false);
        assert_eq!(u.browse.label(), "A:21");
        u.program_button(3);
        assert_eq!(u.loc.label(), "A:23");
        assert_eq!(
            u.take_requests(),
            vec![LoadRequest::Program("Program/Bank A/P010.ns4p".into())]
        );
        assert_eq!(u.title, "P010");
        u.page(1, true);
        assert_eq!(u.browse.bank, 1);
    }

    #[test]
    fn dial_steps_programs_and_empty_locations_say_empty() {
        let mut u = catalog_ui(2);
        u.dial(1, false);
        assert_eq!(u.loc.label(), "A:12");
        u.dial(1, false);
        assert_eq!(u.title, EMPTY_NAME);
        assert_eq!(u.title, "<empty>");
        assert!(u.requests.len() == 1);
    }
}

#[cfg(test)]
#[path = "nav_tests.rs"]
mod more_tests;
