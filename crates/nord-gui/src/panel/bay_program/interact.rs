//! Wires one painted button to the Program behaviour: soft-button role, Morph hold/latch,
//! Section Edit hold, and the generic tap / hold / double-tap resolution against SHIFT.

use super::controls::{draw_button, feed, trigger_for, Spec};
use super::model::{PresetKind, Screen};
use super::morph::SOURCES;
use crate::audio::AudioCommand;
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::ProgramUi;
use nord_core::morph::MorphSource;
use nord_panel::behaviour::{Gesture, Trigger};

/// Program button number `1..=4` when `id` is one of the four soft-button capable buttons.
pub fn soft_index(id: &str) -> Option<u8> {
    let n: u8 = id.strip_prefix("program.button.")?.parse().ok()?;
    (1..=4).contains(&n).then_some(n)
}

fn morph_source(id: &str) -> Option<MorphSource> {
    match id {
        "morph.wheel" => Some(SOURCES[0]),
        "morph.at" => Some(SOURCES[1]),
        "morph.ctrlped" => Some(SOURCES[2]),
        _ => None,
    }
}

/// LED state of a button.
pub fn lit_state(pu: &ProgramUi, id: &str) -> bool {
    if let Some(n) = id
        .strip_prefix("program.button.")
        .and_then(|n| n.parse::<u8>().ok())
    {
        return if pu.live.is_active() {
            pu.live.slot() == Some(n - 1)
        } else {
            pu.loc.slot + 1 == n
        };
    }
    if let Some(s) = morph_source(id) {
        return pu.morph.assigning() == Some(s);
    }
    let preset = |k: PresetKind| matches!(pu.screen, Screen::Preset { kind, .. } if kind == k);
    match id {
        "program.live_mode" => pu.live.is_active() || pu.live_armed,
        "program.store" => pu.store.is_active() && (pu.now_ms / 300).is_multiple_of(2),
        "program.solo" => pu.solo,
        "program.section_edit" => pu.section_edit_active(),
        "program.layer_scene_ii" => pu.scene2,
        "split.on_set" => pu.split.on,
        "transp.on_set" => pu.transp.on,
        "mst_clk.tap_set" => pu.screen == Screen::Clock,
        "preset_library.organ" => preset(PresetKind::Organ),
        "preset_library.piano" => preset(PresetKind::Piano),
        "preset_library.synth" => preset(PresetKind::Synth),
        _ => false,
    }
}

/// State of a latched shifted function (lights its legend).
pub fn shift_state(pu: &ProgramUi, shift_id: &str) -> bool {
    match shift_id {
        "program.preset_name" => pu.preset_name,
        "program.num_pad" => pu.num_pad,
        _ => false,
    }
}

/// Paint `spec` and act on the pointer. Commands go to `cmds`.
#[allow(clippy::too_many_arguments)]
pub fn handle_button(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    pu: &mut ProgramUi,
    spec: &Spec,
    now: u32,
    shift_down: bool,
    cmds: &mut Vec<AudioCommand>,
) {
    let rect = rect_or_empty(spec.id, origin);
    let lit = lit_state(pu, spec.id);
    let shift_lit = spec.shift.is_some_and(|s| shift_state(pu, s.id));
    let down = draw_button(ui, painter, rect, spec, lit, shift_lit);
    pu.mark(spec.id);
    if let Some(s) = spec.shift {
        pu.mark(s.id);
    }
    let fed = feed(pu, spec.id, down, now);
    press(pu, spec, &fed, shift_down, cmds);
}

/// The behaviour half of [`handle_button`] (no painting; testable with a scripted `Fed`).
pub fn press(
    pu: &mut ProgramUi,
    spec: &Spec,
    fed: &super::controls::Fed,
    shift_down: bool,
    cmds: &mut Vec<AudioCommand>,
) {
    if let Some(n) = soft_index(spec.id) {
        if pu.screen.has_soft_buttons() && !shift_down {
            if fed.went_down {
                cmds.extend(pu.soft_down(n));
            }
            if fed.went_up {
                pu.soft_up(n);
            }
            return;
        }
    }
    if let Some(src) = morph_source(spec.id) {
        if fed.went_down && !shift_down {
            pu.morph_down(src);
        }
        if fed.went_up {
            pu.morph_up(src);
        }
        match fed.gesture {
            Some(Gesture::DoubleTap) => pu.morph_latch(src),
            Some(Gesture::Tap) if shift_down => {
                pu.shift_used = true;
                cmds.extend(pu.morph_clear(src));
            }
            Some(Gesture::Tap) => pu.morph_tap(src),
            _ => {}
        }
        return;
    }
    if spec.id == "program.section_edit" {
        if fed.went_down {
            pu.section_held = true;
        }
        if fed.went_up {
            pu.section_held = false;
        }
    }
    if let Some(g) = fed.gesture {
        let trig = trigger_for(spec, g, shift_down);
        let id = match (trig, spec.shift) {
            (Trigger::Shifted, Some(s)) => {
                pu.shift_used = true;
                s.id
            }
            _ => spec.id,
        };
        cmds.extend(pu.activate(id, trig));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ProgramEdit;
    use crate::panel::bay_program::controls::{Fed, BUTTONS, MORPH_BUTTONS, PROGRAM_BUTTONS};

    fn spec(id: &str) -> Spec {
        *PROGRAM_BUTTONS
            .iter()
            .chain(BUTTONS.iter())
            .chain(MORPH_BUTTONS.iter())
            .find(|s| s.id == id)
            .unwrap()
    }

    fn tap() -> Fed {
        Fed {
            gesture: Some(Gesture::Tap),
            went_down: false,
            went_up: true,
        }
    }

    #[test]
    fn soft_index_only_for_buttons_one_to_four() {
        assert_eq!(soft_index("program.button.4"), Some(4));
        assert_eq!(soft_index("program.button.5"), None);
        assert_eq!(soft_index("program.store"), None);
    }

    #[test]
    fn program_buttons_one_to_four_become_soft_buttons_on_soft_screens() {
        let mut pu = ProgramUi::default();
        let mut cmds = Vec::new();
        pu.screen = Screen::Split;
        let down = Fed {
            gesture: None,
            went_down: true,
            went_up: false,
        };
        press(&mut pu, &spec("program.button.4"), &down, false, &mut cmds);
        assert_eq!(cmds.len(), 1, "soft button sent the split config");
        assert!(pu.split.enabled[2] && pu.split.point == 2, "High selected");
        // Shift+button still opens the menu.
        pu.screen = Screen::Split;
        cmds.clear();
        press(&mut pu, &spec("program.button.2"), &tap(), true, &mut cmds);
        assert!(matches!(pu.screen, Screen::Menu(_)));
        assert!(pu.shift_used);
    }

    #[test]
    fn plain_program_button_selects_and_shift_tap_resolves_the_shifted_id() {
        let mut pu = ProgramUi::default();
        let mut cmds = Vec::new();
        press(&mut pu, &spec("program.button.6"), &tap(), false, &mut cmds);
        assert_eq!(pu.loc.slot, 5);
        press(&mut pu, &spec("program.store"), &tap(), true, &mut cmds);
        assert_eq!(pu.screen, Screen::StoreAs, "Shift+Store = STORE AS");
    }

    #[test]
    fn morph_buttons_hold_assign_double_tap_latch_and_shift_clear() {
        let mut pu = ProgramUi::default();
        let mut cmds = Vec::new();
        let wheel = spec("morph.wheel");
        let down = Fed {
            gesture: None,
            went_down: true,
            went_up: false,
        };
        press(&mut pu, &wheel, &down, false, &mut cmds);
        assert_eq!(pu.assigning_morph(), Some(SOURCES[0]));
        let up = Fed {
            gesture: Some(Gesture::Hold),
            went_down: false,
            went_up: true,
        };
        press(&mut pu, &wheel, &up, false, &mut cmds);
        assert_eq!(pu.assigning_morph(), None);
        let dbl = Fed {
            gesture: Some(Gesture::DoubleTap),
            went_down: true,
            went_up: false,
        };
        press(&mut pu, &spec("morph.at"), &dbl, false, &mut cmds);
        assert_eq!(pu.assigning_morph(), Some(SOURCES[1]));
        press(&mut pu, &spec("morph.ctrlped"), &tap(), true, &mut cmds);
        assert!(matches!(
            &cmds[0],
            AudioCommand::Program(ProgramEdit::ClearMorph(MorphSource::ControlPedal))
        ));
    }

    #[test]
    fn section_edit_is_held_while_down_and_latches_on_double_tap() {
        let mut pu = ProgramUi::default();
        let mut cmds = Vec::new();
        let se = spec("program.section_edit");
        let down = Fed {
            gesture: None,
            went_down: true,
            went_up: false,
        };
        press(&mut pu, &se, &down, false, &mut cmds);
        assert!(pu.section_edit_active());
        let hold_up = Fed {
            gesture: Some(Gesture::Hold),
            went_down: false,
            went_up: true,
        };
        press(&mut pu, &se, &hold_up, false, &mut cmds);
        assert!(!pu.section_edit_active(), "hold is not Layer Init");
        assert_eq!(pu.screen, Screen::Home);
        let dbl = Fed {
            gesture: Some(Gesture::DoubleTap),
            went_down: true,
            went_up: false,
        };
        press(&mut pu, &se, &dbl, false, &mut cmds);
        assert!(pu.section_latch.is_active());
        press(&mut pu, &se, &tap(), true, &mut cmds);
        assert_eq!(
            pu.screen,
            Screen::LayerInit,
            "Shift+Section Edit = Layer Init"
        );
    }

    #[test]
    fn lit_state_follows_the_ui_state() {
        let mut pu = ProgramUi::default();
        assert!(lit_state(&pu, "program.button.1"));
        assert!(!lit_state(&pu, "program.button.2"));
        pu.split.on = true;
        pu.scene2 = true;
        pu.preset_name = true;
        assert!(lit_state(&pu, "split.on_set") && lit_state(&pu, "program.layer_scene_ii"));
        assert!(shift_state(&pu, "program.preset_name") && !shift_state(&pu, "program.num_pad"));
        pu.live.select(3);
        assert!(lit_state(&pu, "program.button.3") && !lit_state(&pu, "program.button.1"));
    }
}
