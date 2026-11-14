//! Soft buttons (Program buttons 1-4 on screens that label them), the Layer Init / Copy / Paste
//! choices and Shift/Exit.

use super::fw_messages::FwMessage;
use super::model::{ListMode, Screen};
use super::nav::{cmd, sibling};
use crate::audio::{AudioCommand, CopyItem, ProgramEdit};
use crate::panel::ui_state::ProgramUi;
use nord_core::engine::LayerInitTarget;
use nord_core::morph::{LayerId, MorphSource};

impl ProgramUi {
    /// Soft button `n` (1..=4) pressed on the current screen.
    pub fn soft_down(&mut self, n: u8) -> Vec<AudioCommand> {
        let screen = self.screen;
        match screen {
            Screen::List => {
                match n {
                    1 => self.list_mode = ListMode::Num,
                    2 => {
                        self.list_mode = ListMode::Abc;
                        self.list_letter_held = true;
                    }
                    3 => {
                        self.list_mode = ListMode::Cat;
                        self.list_cat_held = true;
                    }
                    _ => {}
                }
                self.sync_list_cursor();
                Vec::new()
            }
            Screen::StoreAs => {
                match n {
                    1 => self.store.editor.press_abc(),
                    2 => self.store.editor.cat_held = true,
                    3 => self.store.editor.delete(),
                    4 => self.store.editor.insert(),
                    _ => {}
                }
                Vec::new()
            }
            Screen::Split => {
                self.split.soft(n);
                vec![cmd(ProgramEdit::Split(self.split.cfg()))]
            }
            Screen::Clock => {
                if n == 1 {
                    self.clock.kbs = self.clock.kbs.next();
                }
                Vec::new()
            }
            Screen::Preset { .. } => {
                if n == 3 {
                    self.screen = Screen::Home;
                }
                Vec::new()
            }
            Screen::Undo => {
                self.screen = Screen::Home;
                match n {
                    1 if !self.undo_blocked => vec![cmd(ProgramEdit::Undo)],
                    2 if self.redo_available => vec![cmd(ProgramEdit::Redo)],
                    _ => Vec::new(),
                }
            }
            Screen::LayerInit => self.layer_init_choice(n),
            Screen::Copy => self.copy_choice(n),
            Screen::Paste => self.paste_choice(n),
            Screen::Menu(_) => self.menu_soft(n),
            _ => Vec::new(),
        }
    }

    /// Soft button `n` released.
    pub fn soft_up(&mut self, n: u8) {
        match (self.screen, n) {
            (Screen::StoreAs, 1) => self.store.editor.release_abc(),
            (Screen::StoreAs, 2) => self.store.editor.cat_held = false,
            (Screen::List, 2) => self.list_letter_held = false,
            (Screen::List, 3) => self.list_cat_held = false,
            _ => {}
        }
    }

    fn layer_init_choice(&mut self, n: u8) -> Vec<AudioCommand> {
        let target = match n {
            1 => LayerInitTarget::All,
            2 => LayerInitTarget::OrganAb,
            3 if self.focus == LayerId::PianoB => LayerInitTarget::PianoB,
            3 => LayerInitTarget::PianoA,
            _ => match self.focus {
                LayerId::SynthB => LayerInitTarget::SynthB,
                LayerId::SynthC => LayerInitTarget::SynthC,
                _ => LayerInitTarget::SynthA,
            },
        };
        self.screen = Screen::Home;
        self.set_message("Layer Init");
        vec![cmd(ProgramEdit::LayerInit(target))]
    }

    fn copy_choice(&mut self, n: u8) -> Vec<AudioCommand> {
        let item = match n {
            1 => CopyItem::Layer(self.focus),
            2 => CopyItem::Fx(self.focus),
            3 => CopyItem::Morph(self.morph.viewed.unwrap_or(MorphSource::ModWheel)),
            _ => CopyItem::Program,
        };
        self.copied = Some(item);
        self.screen = Screen::Home;
        self.set_message("Copied");
        vec![cmd(ProgramEdit::Copy(item))]
    }

    fn paste_choice(&mut self, n: u8) -> Vec<AudioCommand> {
        let Some(item) = self.copied else {
            self.show(if n == 2 {
                FwMessage::NothingToSwapWith
            } else {
                FwMessage::NothingToPaste
            });
            return Vec::new();
        };
        self.screen = Screen::Home;
        match (n, item) {
            // ASSUMED: pasting a whole program is a stored-memory write, so Memory Protect refuses
            // it with the firmware's `Unable to Paste` + `%c Memory Protected` (0x283790); layer,
            // FX and morph pastes only edit the working program and stay allowed.
            (1, CopyItem::Program) if self.menu.memory_protect() => {
                self.set_message(format!(
                    "{}\n{}",
                    FwMessage::UnableToPaste.text(),
                    FwMessage::MemoryProtected.text()
                ));
                Vec::new()
            }
            (1, _) => {
                // Paste into the focused layer / same morph source / program.
                let target = match item {
                    CopyItem::Layer(_) => CopyItem::Layer(self.focus),
                    CopyItem::Fx(_) => CopyItem::Fx(self.focus),
                    other => other,
                };
                self.show(FwMessage::Pasted);
                vec![cmd(ProgramEdit::Paste(target))]
            }
            (2, _) => {
                self.show(FwMessage::Swapped);
                vec![cmd(ProgramEdit::Swap(self.focus, sibling(self.focus)))]
            }
            _ => Vec::new(),
        }
    }

    /// Shift/Exit pressed alone: leave the open screen, latches and a running Store.
    pub fn exit(&mut self) {
        if self.store.is_active() {
            self.store.cancel();
        }
        self.morph.exit();
        self.section_latch.exit();
        self.live_armed = false;
        self.num_first = None;
        self.split.set_key_armed = false;
        self.solo = false;
        self.screen = Screen::Home;
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::StoreStage;
    use super::*;

    #[test]
    fn exit_cancels_a_store_in_progress() {
        let mut u = ProgramUi::default();
        u.store.begin("x", Default::default());
        assert_eq!(u.store.stage, StoreStage::Choosing);
        u.exit();
        assert!(!u.store.is_active());
    }

    #[test]
    fn soft_buttons_on_a_screen_without_labels_do_nothing() {
        let mut u = ProgramUi::default();
        assert!(u.soft_down(1).is_empty());
        u.soft_up(1);
        u.screen = Screen::List;
        u.soft_down(3);
        assert_eq!(u.list_mode, ListMode::Cat);
        u.soft_up(3);
        assert!(!u.list_cat_held);
    }
}
