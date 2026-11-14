//! Button actions of the Program area: what each inventory id does once the gesture is resolved.
//!
//! `activate` takes the *resolved* id (the shifted id for a shifted press, e.g. `program.undo`
//! for Shift+Solo) and the trigger, and returns the commands to send.

use super::model::{PresetKind, Screen};
use super::nav::cmd;
use super::store::{PendingSnapshot, StoreStage, StoreTarget, UserProgram};
use crate::audio::{AudioCommand, ProgramEdit};
use crate::panel::ui_state::ProgramUi;
use nord_core::morph::{LayerId, MorphSource};
use nord_panel::behaviour::Trigger;

impl ProgramUi {
    /// Perform the action of control `id` for `trig`.
    pub fn activate(&mut self, id: &str, trig: Trigger) -> Vec<AudioCommand> {
        let hold = trig == Trigger::Hold;
        match id {
            "program.shift.left" | "program.shift.right" => {
                // Shift used as a modifier is not an EXIT (manual p.44).
                if !self.shift_used {
                    self.exit();
                }
                Vec::new()
            }
            "program.page_down" => self.page(-1, false),
            "program.page_up" => self.page(1, false),
            "program.bank_down" => self.page(-1, true),
            "program.bank_up" => self.page(1, true),
            "program.prog_view" => {
                self.view = self.view.next();
                self.screen = Screen::Home;
                Vec::new()
            }
            "program.preset_name" => {
                self.preset_name = !self.preset_name;
                Vec::new()
            }
            "program.live_mode" => self.live_mode_press(),
            "program.num_pad" => {
                self.num_pad = !self.num_pad;
                self.num_first = None;
                self.screen = if self.num_pad {
                    Screen::NumPad
                } else {
                    Screen::Home
                };
                Vec::new()
            }
            "program.store" => self.store_press(),
            "program.store_as" => self.store_as(),
            "program.solo" => {
                self.solo = !self.solo;
                self.set_message("Solo: layer buttons are in the other bays");
                Vec::new()
            }
            "program.undo" => self.open(Screen::Undo),
            "program.section_edit" => {
                match trig {
                    Trigger::Latch => self.section_latch.double_tap(),
                    Trigger::Primary if self.section_latch.is_active() => self.section_latch.exit(),
                    _ => {}
                }
                Vec::new()
            }
            "program.layer_init" => self.open(Screen::LayerInit),
            "program.mon_copy" => self.open(Screen::Copy),
            "program.paste" => self.open(Screen::Paste),
            "program.layer_scene_ii" => {
                self.scene2 = !self.scene2;
                vec![cmd(ProgramEdit::LayerScene2(self.scene2))]
            }
            "split.on_set" if hold => self.open(Screen::Split),
            "split.on_set" => {
                self.split.on = !self.split.on;
                vec![cmd(ProgramEdit::Split(self.split.cfg()))]
            }
            "split.set_key" => {
                self.split.set_key(None);
                self.set_message("SET KEY: press a key");
                Vec::new()
            }
            "mst_clk.tap_set" if hold => self.open(Screen::Clock),
            "mst_clk.tap_set" => match self.clock.tap(self.now_ms) {
                Some(bpm) => vec![cmd(ProgramEdit::MasterBpm(bpm))],
                None => Vec::new(),
            },
            "mst_clk.pedal_tap" => {
                self.pedal_tap = !self.pedal_tap;
                self.set_message(if self.pedal_tap {
                    "Pedal Tap: on"
                } else {
                    "Pedal Tap: off"
                });
                vec![cmd(ProgramEdit::PedalTap(self.pedal_tap))]
            }
            "transp.on_set" if hold => self.open(Screen::Transpose),
            "transp.on_set" => {
                self.transp.toggle();
                vec![cmd(ProgramEdit::Transpose(self.transp.effective()))]
            }
            "transp.panic" => vec![cmd(ProgramEdit::Panic)],
            "preset_library.organ" => self.preset_open(PresetKind::Organ, false),
            "preset_library.piano" => self.preset_open(PresetKind::Piano, false),
            "preset_library.synth" => self.preset_open(PresetKind::Synth, false),
            "preset_library.single_piano" => self.preset_open(PresetKind::Piano, true),
            "preset_library.single_synth" => self.preset_open(PresetKind::Synth, true),
            id => {
                if let Some(m) = id
                    .strip_prefix("program.menu.")
                    .and_then(|_| menu_button(id))
                {
                    self.open_menu(m);
                } else if let Some(n) = id
                    .strip_prefix("program.button.")
                    .and_then(|n| n.parse::<u8>().ok())
                {
                    return self.program_button(n);
                }
                Vec::new()
            }
        }
    }

    /// Open `screen` (a second press of the same opener closes it).
    fn open(&mut self, screen: Screen) -> Vec<AudioCommand> {
        self.screen = if self.screen == screen {
            Screen::Home
        } else {
            screen
        };
        Vec::new()
    }

    fn preset_open(&mut self, kind: PresetKind, single: bool) -> Vec<AudioCommand> {
        if self.store.stage == StoreStage::Choosing {
            return self.preset_store_begin(kind);
        }
        self.preset_cursor = self.preset_start_cursor(kind);
        if single {
            self.focus = match (kind, self.focus) {
                (PresetKind::Piano, LayerId::PianoA | LayerId::PianoB) => self.focus,
                (PresetKind::Synth, LayerId::SynthA | LayerId::SynthB | LayerId::SynthC) => {
                    self.focus
                }
                (PresetKind::Piano, _) => LayerId::PianoA,
                _ => LayerId::SynthA,
            };
        }
        self.open(Screen::Preset { kind, single })
    }

    /// LIVE MODE: arm slot selection; pressed again (or during Store) it picks the Live target.
    fn live_mode_press(&mut self) -> Vec<AudioCommand> {
        if self.store.stage == StoreStage::Choosing {
            self.store.target = Some(StoreTarget::Live(0));
            return Vec::new();
        }
        self.live_armed = !self.live_armed;
        Vec::new()
    }

    /// Morph Assign button went down (assigning while held).
    pub fn morph_down(&mut self, s: MorphSource) {
        self.morph.down(s);
    }

    /// Morph Assign button went up.
    pub fn morph_up(&mut self, s: MorphSource) {
        self.morph.up(s);
    }

    /// Tap on a Morph Assign button: show / hide that source on the display.
    pub fn morph_tap(&mut self, s: MorphSource) {
        self.morph.viewed = Some(s);
        self.screen = if self.screen == Screen::Morph(s) {
            Screen::Home
        } else {
            Screen::Morph(s)
        };
    }

    /// Double tap on a Morph Assign button: latch assign mode (Exit leaves it).
    pub fn morph_latch(&mut self, s: MorphSource) {
        self.morph.double_tap(s);
    }

    /// Shift+Morph Assign: clear that source.
    pub fn morph_clear(&mut self, s: MorphSource) -> Vec<AudioCommand> {
        self.set_message("Morph cleared");
        vec![cmd(self.morph.clear(s))]
    }

    /// STORE: start choosing a location; pressed again it confirms (manual p.41).
    pub fn store_press(&mut self) -> Vec<AudioCommand> {
        match self.store.stage {
            StoreStage::Idle => {
                // A Preset Library button first, then STORE, also stores a preset (manual p.42).
                let from_browser = match self.screen {
                    Screen::Preset { kind, .. } => Some(kind),
                    _ => None,
                };
                self.store.begin(&self.title, self.loc);
                self.screen = Screen::StoreTo;
                match from_browser {
                    Some(kind) => self.preset_store_begin(kind),
                    None => Vec::new(),
                }
            }
            StoreStage::Naming => {
                self.store.finish_naming();
                self.screen = Screen::StoreTo;
                Vec::new()
            }
            StoreStage::Choosing => self.store_confirm(),
        }
    }

    /// STORE AS (Shift+Store): name the program first (manual p.42).
    pub fn store_as(&mut self) -> Vec<AudioCommand> {
        let library = &self.library;
        self.store
            .begin_as(&self.title, self.loc, |n| library.name_exists(n));
        if let Some(p) = self.library.get(self.loc) {
            self.store.editor.category = p.category;
        }
        self.screen = Screen::StoreAs;
        Vec::new()
    }

    fn store_confirm(&mut self) -> Vec<AudioCommand> {
        if self.store.preset.is_some() {
            return self.preset_store_confirm();
        }
        let name = {
            let n = self.store.editor.name();
            if n.is_empty() {
                self.title.clone()
            } else {
                n
            }
        };
        let category = self.store.editor.category;
        let target = self.store.target;
        if matches!(target, Some(StoreTarget::Bank(_))) && self.store_blocked() {
            // Memory Protect (System menu page 1): nothing is written, the screen closes.
            self.store.cancel();
            self.screen = Screen::Home;
            return Vec::new();
        }
        let out = match target {
            Some(StoreTarget::Bank(loc)) => {
                self.loc = loc;
                self.browse = loc;
                self.title = name.clone();
                self.set_message(format!("Stored {}", loc.label()));
                vec![cmd(self.request_snapshot(PendingSnapshot::Bank {
                    loc,
                    name,
                    category,
                }))]
            }
            Some(StoreTarget::Live(slot)) => {
                self.set_message(format!("Stored Live {}", slot + 1));
                vec![cmd(self.request_snapshot(PendingSnapshot::Live(slot)))]
            }
            None => Vec::new(),
        };
        self.store.cancel();
        self.screen = Screen::Home;
        out
    }

    /// Resolves a snapshot reply into the in-memory library.
    pub fn accept_snapshot(&mut self, ticket: u32, state: Box<nord_core::engine::ProgramState>) {
        let Some(pos) = self.pending.iter().position(|(t, _)| *t == ticket) else {
            return;
        };
        match self.pending.remove(pos).1 {
            PendingSnapshot::Bank {
                loc,
                name,
                category,
            } => {
                self.library.put(
                    loc,
                    UserProgram {
                        name,
                        category,
                        state,
                    },
                );
                self.mark_clean(); // STORE clears the boxed `E`
            }
            PendingSnapshot::Live(slot) => self.library.put_live(slot, state),
            PendingSnapshot::Preset { kind, loc, name } => {
                self.accept_preset_snapshot(kind, loc, name, state);
            }
        }
    }
}

fn menu_button(id: &str) -> Option<super::model::MenuId> {
    use super::model::MenuId::*;
    Some(match id.strip_prefix("program.menu.")? {
        "system" => System,
        "sound" => Sound,
        "organize" => Organize,
        "aux_kb" => AuxKb,
        "output" => Output,
        "pedal" => Pedal,
        "midi" => Midi,
        "extern" => Extern,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::apply_audio_command;
    use nord_core::engine::Stage4Engine;

    /// Plays the DSP thread: applies `cmds` and feeds snapshot replies back to the UI.
    fn run(u: &mut ProgramUi, e: &mut Stage4Engine, cmds: Vec<AudioCommand>) {
        let mut vol = 1.0;
        for c in cmds {
            apply_audio_command(c, e, &mut vol);
        }
        while let Ok((t, s)) = u.reply_rx.try_recv() {
            u.accept_snapshot(t, s);
        }
    }

    #[test]
    fn store_flow_store_choose_confirm_writes_the_library() {
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[2].base = 0.42;
        let mut u = ProgramUi::default();
        u.menu.set_memory_protect(false);
        u.title = "Rusty PWM".into();
        assert!(u.store_press().is_empty());
        assert_eq!(u.screen, Screen::StoreTo);
        u.page(1, true); // BANK + moves the target
        u.program_button(7); // program button picks the slot
        let target = u.store.bank_target().unwrap();
        assert_eq!(target.label(), "B:17");
        let out = u.store_press(); // confirm
        run(&mut u, &mut e, out);
        assert_eq!(u.screen, Screen::Home);
        assert!(!u.store.is_active());
        let p = u.library.get(target).expect("stored");
        assert_eq!(p.name, "Rusty PWM");
        let mut fresh = Stage4Engine::new(48_000.0);
        let mut vol = 1.0;
        apply_audio_command(
            AudioCommand::Program(ProgramEdit::Recall(p.state.clone())),
            &mut fresh,
            &mut vol,
        );
        assert_eq!(fresh.layer_levels[2].base, 0.42);
        assert_eq!(u.loc, target);
    }

    #[test]
    fn selecting_the_stored_location_recalls_it() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut u = ProgramUi::default();
        u.menu.set_memory_protect(false);
        e.layer_levels[0].base = 0.11;
        u.store_press();
        let target = u.store.bank_target().unwrap();
        let out = u.store_press();
        run(&mut u, &mut e, out);
        e.layer_levels[0].base = 0.99;
        let out = u.select_loc(target);
        run(&mut u, &mut e, out);
        assert_eq!(e.layer_levels[0].base, 0.11);
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod more_tests;
