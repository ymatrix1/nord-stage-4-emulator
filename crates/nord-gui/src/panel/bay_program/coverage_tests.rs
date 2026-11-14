//! Evidence for B8d / B8g: every Program-area inventory id has a painted widget, the dimmed set
//! is exactly the documented one, and a real pointer press reaches the behaviour.

use super::controls::{BUTTONS, MORPH_BUTTONS, PROGRAM_BUTTONS};
use super::render_bay_program;
use crate::audio::{AudioCommand, AudioHandle, ProgramEdit};
use crate::panel::keybed::render_keybed;
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crossbeam_channel::Receiver;
use egui::epaint::ClippedShape;
use egui::{Event, PointerButton, RawInput};
use nord_panel::{all_controls, layout};

const PREFIXES: [&str; 7] = [
    "program.",
    "morph.",
    "split.",
    "mst_clk.",
    "transp.",
    "preset_library.",
    "keybed.",
];

/// Inventory ids this PR owns.
fn owned_ids() -> Vec<String> {
    all_controls()
        .into_iter()
        .map(|c| c.id)
        .filter(|id| PREFIXES.iter().any(|p| id.starts_with(p)))
        .collect()
}

struct Rig {
    ctx: egui::Context,
    state: Stage4PanelState,
    audio: AudioHandle,
    rx: Receiver<AudioCommand>,
    origin: egui::Pos2,
    time: f64,
}

impl Rig {
    fn new() -> Self {
        let (audio, rx) = AudioHandle::new();
        Self {
            ctx: egui::Context::default(),
            state: Stage4PanelState::default(),
            audio,
            rx,
            origin: pos2(0.0, 0.0),
            time: 1.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<ClippedShape> {
        self.time += 0.05;
        let raw = RawInput {
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let (state, audio) = (&mut self.state, &self.audio);
        let mut origin = self.origin;
        let out = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(1560.0, 420.0), Sense::hover());
                origin = rect.min;
                let painter = ui.painter().clone();
                render_bay_program(ui, &painter, rect, state, audio);
                render_keybed(ui, rect, state, audio);
            });
        });
        self.origin = origin;
        out.shapes
    }

    fn click(&mut self, id: &str) {
        let p = rect_or_empty(id, self.origin).center();
        let m = egui::Modifiers::NONE;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: m,
        }]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: m,
        }]);
    }

    fn commands(&self) -> Vec<AudioCommand> {
        self.rx.try_iter().collect()
    }
}

#[test]
fn every_program_and_keybed_inventory_id_is_painted() {
    assert_eq!(owned_ids().len(), 58, "40 + 16 program ids + 2 keybed ids");
    let mut rig = Rig::new();
    rig.frame(Vec::new());
    let painted = &rig.state.program_ui.painted;
    let mut missing: Vec<String> = Vec::new();
    for id in owned_ids() {
        if !painted.iter().any(|p| *p == id) {
            missing.push(id);
        }
    }
    assert!(missing.is_empty(), "not painted: {missing:?}");
}

#[test]
fn every_own_rect_id_has_a_shape_inside_its_layout_rectangle() {
    let mut rig = Rig::new();
    let shapes = rig.frame(Vec::new());
    let boxes: Vec<egui::Rect> = shapes
        .iter()
        .map(|s| s.shape.visual_bounding_rect())
        .collect();
    let mut bare: Vec<String> = Vec::new();
    for id in owned_ids() {
        if layout::shared_base(&id).is_some() {
            continue; // shares the rectangle of its base control
        }
        let area = rect_or_empty(&id, rig.origin).expand(8.0);
        if !boxes
            .iter()
            .any(|b| area.contains_rect(*b) && b.width() > 0.0)
        {
            bare.push(id);
        }
    }
    assert!(bare.is_empty(), "no shape inside the rectangle of {bare:?}");
}

#[test]
fn dimmed_ids_are_exactly_the_documented_unsupported_set() {
    let mut dimmed: Vec<&str> = Vec::new();
    for s in PROGRAM_BUTTONS.iter().chain(&BUTTONS).chain(&MORPH_BUTTONS) {
        if s.support.is_dimmed() {
            dimmed.push(s.id);
        }
        if let Some(f) = s.shift {
            if f.support.is_dimmed() {
                dimmed.push(f.id);
            }
        }
    }
    dimmed.sort_unstable();
    assert_eq!(dimmed, ["mst_clk.pedal_tap", "program.solo",]);
}

#[test]
fn rendering_is_deterministic() {
    let count = || {
        let mut rig = Rig::new();
        rig.frame(Vec::new()).len()
    };
    let (a, b) = (count(), count());
    assert_eq!(a, b);
    assert!(a > 300, "bay and keybed painted {a} shapes");
}

#[test]
fn a_pointer_click_on_program_button_3_selects_slot_3() {
    let mut rig = Rig::new();
    rig.frame(Vec::new());
    rig.click("program.button.3");
    assert_eq!(rig.state.program_ui.loc.slot, 2);
    assert_eq!(rig.state.prog_id, "A:13");
    assert_eq!(rig.state.active_program_idx, 3);
}

#[test]
fn clicking_split_and_scene_sends_engine_edits() {
    let mut rig = Rig::new();
    rig.frame(Vec::new());
    rig.click("split.on_set");
    rig.click("program.layer_scene_ii");
    assert!(rig.state.split_active);
    let cmds = rig.commands();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::Program(ProgramEdit::Split(cfg)) if cfg.on)));
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::Program(ProgramEdit::LayerScene2(true)))));
}

#[test]
fn clicking_store_opens_the_store_screen_and_the_split_led_lights() {
    let mut rig = Rig::new();
    rig.frame(Vec::new());
    rig.click("program.store");
    assert_eq!(rig.state.program_ui.screen, super::model::Screen::StoreTo);
    rig.state.program_ui.split.on = true;
    let before = rig.frame(Vec::new()).len();
    assert!(before > 0);
    assert_eq!(rig.state.program_ui.split.leds(), [false, true, false]);
}
