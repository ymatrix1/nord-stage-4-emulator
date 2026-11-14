//! The control table of the Program area (one row per button of the inventory) and the egui
//! button painter / gesture feeder.

use super::model::Support;
use crate::panel::prelude::*;
use crate::panel::ui_state::ProgramUi;
use egui::FontId;
use nord_panel::behaviour::{resolve, Gesture, Trigger};

/// The function printed below a button (Shift+button, or hold-to-shift).
#[derive(Debug, Clone, Copy)]
pub struct ShiftFn {
    pub id: &'static str,
    pub label: &'static str,
    pub support: Support,
}

/// One physical button.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Inventory id of the button; its layout rectangle is the button's place.
    pub id: &'static str,
    pub label: &'static str,
    pub shift: Option<ShiftFn>,
    pub support: Support,
    /// A long press means the primary function (screen / hold-assign), not the shifted one.
    pub hold_primary: bool,
}

const fn sf(id: &'static str, label: &'static str) -> Option<ShiftFn> {
    Some(ShiftFn {
        id,
        label,
        support: Support::Full,
    })
}

const fn menu(id: &'static str, label: &'static str) -> Option<ShiftFn> {
    Some(ShiftFn {
        id,
        label,
        support: Support::Full,
    })
}

const fn b(
    id: &'static str,
    label: &'static str,
    shift: Option<ShiftFn>,
    hold_primary: bool,
) -> Spec {
    Spec {
        id,
        label,
        shift,
        support: Support::Full,
        hold_primary,
    }
}

/// Program buttons 1-8 with their Shift+n menu shortcuts.
pub const PROGRAM_BUTTONS: [Spec; 8] = [
    b(
        "program.button.1",
        "1",
        menu("program.menu.system", "System"),
        false,
    ),
    b(
        "program.button.2",
        "2",
        menu("program.menu.sound", "Sound"),
        false,
    ),
    b(
        "program.button.3",
        "3",
        menu("program.menu.organize", "Organize"),
        false,
    ),
    b(
        "program.button.4",
        "4",
        menu("program.menu.aux_kb", "Aux KB"),
        false,
    ),
    b(
        "program.button.5",
        "5",
        menu("program.menu.output", "Output"),
        false,
    ),
    b(
        "program.button.6",
        "6",
        menu("program.menu.pedal", "Pedal"),
        false,
    ),
    b(
        "program.button.7",
        "7",
        menu("program.menu.midi", "MIDI"),
        false,
    ),
    b(
        "program.button.8",
        "8",
        menu("program.menu.extern", "Extern"),
        false,
    ),
];

/// Every other button except the three Morph Assign buttons (see [`MORPH_BUTTONS`]).
pub const BUTTONS: [Spec; 17] = [
    b("program.shift.left", "SHIFT", None, false),
    b("program.shift.right", "SHIFT", None, false),
    b("program.page_down", "PAGE-", sf("program.bank_down", "BANK-"), false),
    b("program.page_up", "PAGE+", sf("program.bank_up", "BANK+"), false),
    b("program.prog_view", "PROG VIEW", sf("program.preset_name", "PRESET NAME"), false),
    b("program.live_mode", "LIVE MODE", sf("program.num_pad", "NUM PAD"), false),
    b("program.store", "STORE", sf("program.store_as", "STORE AS"), false),
    Spec {
        id: "program.solo",
        label: "SOLO",
        shift: sf("program.undo", "UNDO"),
        support: Support::Unsupported("Solo needs the Layer buttons of the other bays; the state is tracked"),
        hold_primary: false,
    },
    Spec {
        id: "program.section_edit",
        label: "SECT EDIT",
        shift: sf("program.layer_init", "LAYER INIT"),
        support: Support::Partial("State is ProgramUi::section_edit_active(); bays apply it with the engine section_edit helpers"),
        hold_primary: true,
    },
    b("program.mon_copy", "MON/COPY", sf("program.paste", "PASTE"), false),
    b("program.layer_scene_ii", "SCENE II", None, false),
    b("split.on_set", "ON/SET", sf("split.set_key", "SET KEY"), true),
    Spec {
        id: "mst_clk.tap_set",
        label: "TAP/SET",
        shift: Some(ShiftFn {
            id: "mst_clk.pedal_tap",
            label: "PEDAL TAP",
            support: Support::Unsupported("The standalone has no pedal input yet (ns4-ymci)"),
        }),
        support: Support::Partial("KBS and an external MIDI clock are not modelled by the engine"),
        hold_primary: true,
    },
    b("transp.on_set", "ON/SET", sf("transp.panic", "PANIC"), true),
    b("preset_library.organ", "ORGAN", None, false),
    Spec {
        id: "preset_library.piano",
        label: "PIANO",
        shift: sf("preset_library.single_piano", "SINGLE"),
        support: Support::Partial("Needs an open .ns4b backup; Cancel does not revert"),
        hold_primary: false,
    },
    Spec {
        id: "preset_library.synth",
        label: "SYNTH",
        shift: sf("preset_library.single_synth", "SINGLE"),
        support: Support::Partial("Needs an open .ns4b backup; Cancel does not revert"),
        hold_primary: false,
    },
];

/// MORPH ASSIGN buttons: WHEEL, A.T., CTRLPED (Shift = CLEAR on all three).
pub const MORPH_BUTTONS: [Spec; 3] = [
    Spec {
        id: "morph.wheel",
        label: "WHEEL",
        shift: sf("morph.clear", "CLEAR"),
        support: Support::Partial(
            "Assign edits layer levels and synth cutoffs; knob-move assign needs bay hooks",
        ),
        hold_primary: true,
    },
    Spec {
        id: "morph.at",
        label: "A.T.",
        shift: sf("morph.clear", "CLEAR"),
        support: Support::Partial(
            "Assign edits layer levels and synth cutoffs; knob-move assign needs bay hooks",
        ),
        hold_primary: true,
    },
    Spec {
        id: "morph.ctrlped",
        label: "CTRLPED",
        shift: sf("morph.clear", "CLEAR"),
        support: Support::Partial(
            "Assign edits layer levels and synth cutoffs; knob-move assign needs bay hooks",
        ),
        hold_primary: true,
    },
];

/// Feeds the pointer state of one button into its tracker; returns the recognised gesture and
/// the down / up edges of this frame.
pub struct Fed {
    pub gesture: Option<Gesture>,
    pub went_down: bool,
    pub went_up: bool,
}

/// Run the button state machine for `id` with the pointer `down` at time `now`.
pub fn feed(pu: &mut ProgramUi, id: &'static str, down: bool, now: u32) -> Fed {
    let t = pu.trackers.entry(id).or_default();
    let was = t.is_down();
    let mut gesture = None;
    if down && !was {
        gesture = t.press(now);
    }
    if down {
        if let Some(h) = t.tick(now) {
            gesture = Some(h);
        }
    }
    if !down && was {
        gesture = t.release(now).or(gesture);
    }
    Fed {
        gesture,
        went_down: down && !was,
        went_up: !down && was,
    }
}

/// Resolve a gesture for `spec`, honouring `hold_primary`.
pub fn trigger_for(spec: &Spec, g: Gesture, shift_down: bool) -> Trigger {
    if spec.hold_primary && g == Gesture::Hold {
        return Trigger::Hold;
    }
    resolve(g, shift_down, spec.shift.is_some())
}

/// Paint one button with its label, shift legend and LED state; returns whether the pointer is
/// down on it. `shift_lit` lights the shift legend (for latched shifted functions).
pub fn draw_button(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    spec: &Spec,
    lit: bool,
    shift_lit: bool,
) -> bool {
    let dim = spec.support.is_dimmed();
    let resp = ui.interact(rect, ui.id().with(spec.id), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let fade = |c: Color32| if dim { c.gamma_multiply(0.45) } else { c };
    let body_base = fade(BUTTON_BLACK);
    let body = if down {
        body_base.gamma_multiply(0.7)
    } else {
        body_base
    };
    painter.rect_filled(rect.expand(1.5), 2.5, BUTTON_BEZEL);
    painter.rect_filled(rect, 2.0, body);
    let pill = rect.shrink2(vec2(rect.width() * 0.16, rect.height() * 0.22));
    let pill_fill = if lit {
        fade(LED_RED)
    } else {
        body.gamma_multiply(1.3)
    };
    painter.rect_filled(pill, pill.height() * 0.5, pill_fill);
    painter.rect_stroke(
        pill,
        pill.height() * 0.5,
        Stroke::new(0.8_f32, fade(Color32::from_gray(70))),
        StrokeKind::Inside,
    );
    let size = (rect.height() * 0.5).clamp(4.5, 7.0);
    painter.text(
        pos2(rect.center().x, rect.top() - 2.5),
        Align2::CENTER_BOTTOM,
        spec.label,
        FontId::proportional(size.max(MIN_FONT_PX)),
        if dim {
            dim_legend(Color32::WHITE)
        } else {
            Color32::WHITE
        },
    );
    if let Some(s) = spec.shift {
        let col = if shift_lit {
            LED_AMBER
        } else {
            Color32::from_rgb(235, 190, 190)
        };
        painter.text(
            pos2(rect.center().x, rect.bottom() + 3.0),
            Align2::CENTER_TOP,
            s.label,
            FontId::proportional((size * 0.9).max(MIN_FONT_PX)),
            if s.support.is_dimmed() {
                dim_legend(col)
            } else {
                col
            },
        );
    }
    let mut tip = spec.label.to_string();
    if let Some(s) = spec.shift {
        tip.push_str(&format!("  |  Shift: {}", s.label));
        if let Some(n) = s.support.note() {
            tip.push_str(&format!("\n{}: {n}", s.label));
        }
    }
    if let Some(n) = spec.support.note() {
        tip.push_str(&format!("\n{n}"));
    }
    resp.on_hover_text(tip);
    down
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::{all_controls, ControlKind};

    #[test]
    fn table_ids_exist_in_the_inventory() {
        let ids: Vec<String> = all_controls().iter().map(|c| c.id.clone()).collect();
        for s in PROGRAM_BUTTONS
            .iter()
            .chain(BUTTONS.iter())
            .chain(MORPH_BUTTONS.iter())
        {
            assert!(ids.iter().any(|i| i == s.id), "{}", s.id);
            if let Some(f) = s.shift {
                assert!(ids.iter().any(|i| i == f.id), "{}", f.id);
            }
        }
    }

    #[test]
    fn every_program_button_in_the_inventory_has_a_spec_or_a_shift_legend() {
        let mut have: Vec<&str> = Vec::new();
        for s in PROGRAM_BUTTONS
            .iter()
            .chain(BUTTONS.iter())
            .chain(MORPH_BUTTONS.iter())
        {
            have.push(s.id);
            if let Some(f) = s.shift {
                have.push(f.id);
            }
        }
        for c in all_controls() {
            let mine = [
                "program.",
                "morph.",
                "split.",
                "mst_clk.",
                "transp.",
                "preset_library.",
            ]
            .iter()
            .any(|p| c.id.starts_with(p));
            if mine && c.kind == ControlKind::Button {
                assert!(have.contains(&c.id.as_str()), "no spec for {}", c.id);
            }
        }
    }

    #[test]
    fn hold_primary_buttons_hold_as_primary_and_others_as_shifted() {
        let split = BUTTONS.iter().find(|s| s.id == "split.on_set").unwrap();
        assert_eq!(trigger_for(split, Gesture::Hold, false), Trigger::Hold);
        assert_eq!(trigger_for(split, Gesture::Tap, true), Trigger::Shifted);
        let store = BUTTONS.iter().find(|s| s.id == "program.store").unwrap();
        assert_eq!(trigger_for(store, Gesture::Hold, false), Trigger::Shifted);
        assert_eq!(
            trigger_for(store, Gesture::DoubleTap, false),
            Trigger::Latch
        );
    }

    #[test]
    fn feed_reports_edges_taps_and_holds() {
        let mut pu = ProgramUi::default();
        let f = feed(&mut pu, "program.store", true, 0);
        assert!(f.went_down && f.gesture.is_none());
        let f = feed(&mut pu, "program.store", true, 600);
        assert_eq!(f.gesture, Some(Gesture::Hold));
        let f = feed(&mut pu, "program.store", false, 700);
        assert!(f.went_up && f.gesture.is_none(), "no tap after a hold");
        feed(&mut pu, "program.store", true, 1000);
        let f = feed(&mut pu, "program.store", false, 1050);
        assert_eq!(f.gesture, Some(Gesture::Tap));
    }
}
