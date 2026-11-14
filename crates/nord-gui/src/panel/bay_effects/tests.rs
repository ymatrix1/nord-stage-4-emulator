//! Headless tests of the Layer Effects bay: painted widgets per inventory id and input flows.

use super::ids::{draw_rect, HANDLED};
use super::render_bay_effects;
use crate::audio::{AudioCommand, AudioHandle, FxEdit};
use crate::panel::prelude::*;
use crate::panel::support::shapes_painted;
use crossbeam_channel::Receiver;
use egui::{Event, Modifiers, PointerButton, RawInput};

const PANEL: egui::Vec2 = egui::vec2(1560.0, 420.0);

/// One headless session: a context, a state and the receiving end of the audio queue.
struct Session {
    ctx: egui::Context,
    state: Stage4PanelState,
    handle: AudioHandle,
    rx: Receiver<AudioCommand>,
    origin: egui::Pos2,
    time: f64,
    /// Idle time before each click (above 0.3 s so it is never a double tap).
    idle: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Session {
    fn new() -> Self {
        let (handle, rx) = AudioHandle::new();
        let mut s = Self {
            ctx: egui::Context::default(),
            state: Stage4PanelState::default(),
            handle,
            rx,
            origin: egui::pos2(0.0, 0.0),
            time: 0.0,
            idle: 0.5,
            shapes: Vec::new(),
        };
        s.frame(Vec::new(), Modifiers::NONE);
        s
    }

    /// Run one frame `dt` seconds after the previous one.
    fn frame_at(&mut self, dt: f64, events: Vec<Event>, modifiers: Modifiers) {
        self.time += dt;
        let raw = RawInput {
            time: Some(self.time),
            events,
            modifiers,
            ..RawInput::default()
        };
        let Self {
            ctx,
            state,
            handle,
            shapes,
            origin,
            ..
        } = self;
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(PANEL, Sense::hover());
                *origin = rect.min;
                let painter = ui.painter().clone();
                render_bay_effects(ui, &painter, rect, state, handle);
            });
        });
        *shapes = out.shapes;
    }

    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.frame_at(0.016, events, modifiers);
    }

    fn center(&self, id: &str) -> egui::Pos2 {
        draw_rect(id, self.origin).center()
    }

    fn button(pos: egui::Pos2, pressed: bool, m: Modifiers) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: m,
        }
    }

    /// Press and release on control `id` after `hold_s` seconds.
    fn click_for(&mut self, id: &str, hold_s: f64, m: Modifiers) {
        let p = self.center(id);
        // Idle gap so a click right after another one is not a double tap.
        self.frame_at(self.idle, vec![Event::PointerMoved(p)], m);
        self.frame(vec![Self::button(p, true, m)], m);
        let steps = (hold_s / 0.1).ceil() as usize;
        for _ in 0..steps {
            self.frame_at(0.1, Vec::new(), m);
        }
        self.frame(vec![Self::button(p, false, m)], m);
        self.frame(Vec::new(), m);
    }

    fn click(&mut self, id: &str) {
        self.click_for(id, 0.0, Modifiers::NONE);
    }

    fn shift_click(&mut self, id: &str) {
        self.click_for(id, 0.0, Modifiers::SHIFT);
    }

    /// Drag control `id` upwards by `dy` px.
    fn drag_up(&mut self, id: &str, dy: f32) {
        let p = self.center(id);
        self.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
        self.frame(
            vec![Self::button(p, true, Modifiers::NONE)],
            Modifiers::NONE,
        );
        for i in 1..=4 {
            let q = p - egui::vec2(0.0, dy * i as f32 / 4.0);
            self.frame(vec![Event::PointerMoved(q)], Modifiers::NONE);
        }
        let q = p - egui::vec2(0.0, dy);
        self.frame(
            vec![Self::button(q, false, Modifiers::NONE)],
            Modifiers::NONE,
        );
        self.frame(Vec::new(), Modifiers::NONE);
    }

    fn fx_edits(&mut self) -> Vec<FxEdit> {
        self.rx
            .try_iter()
            .filter_map(|c| match c {
                AudioCommand::Fx(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    fn chain(&self) -> &crate::panel::ui_state::fx::ChainUi {
        let slot =
            crate::panel::ui_state::fx::slot_for(self.state.fx_focus, self.state.fx_ui.piano_b);
        &self.state.fx_ui.chains[slot]
    }
}

#[test]
fn bay_paints_many_shapes() {
    let n = shapes_painted(render_bay_effects);
    assert!(n > 150, "render_bay_effects painted only {n} shapes");
}

#[test]
fn every_inventory_id_has_a_painted_widget() {
    let s = Session::new();
    let mut missing = Vec::new();
    for id in HANDLED {
        let r = draw_rect(id, s.origin);
        let hit = s.shapes.iter().any(|cs| {
            let b = cs.shape.visual_bounding_rect();
            b.is_finite() && r.expand(1.0).contains(b.center())
        });
        if !hit {
            missing.push(id);
        }
    }
    assert!(
        missing.is_empty(),
        "ids without a painted widget: {missing:?}"
    );
}

#[test]
fn rendering_is_deterministic() {
    let dump = |s: &Session| format!("{:?}", s.shapes);
    assert_eq!(dump(&Session::new()), dump(&Session::new()));
}

mod flows;
