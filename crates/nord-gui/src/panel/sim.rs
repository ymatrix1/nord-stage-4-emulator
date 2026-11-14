//! Headless egui driver for the bay tests: one persistent context that renders the Master and
//! Organ bays and replays pointer events (tap, hold, drag) frame by frame.

use super::layout_px::rect_or_empty;
use super::prelude::*;
use super::{bay_master::render_bay_master, bay_organ::render_bay_organ};
use crate::audio::OrganEdit;
use egui::epaint::ClippedShape;
use egui::{Event, Modifiers, PointerButton, RawInput};

/// One persistent headless egui context rendering both bays.
pub(super) struct Sim {
    pub(super) ctx: egui::Context,
    pub(super) state: Stage4PanelState,
    pub(super) audio: AudioHandle,
    pub(super) rx: crossbeam_channel::Receiver<AudioCommand>,
    pub(super) origin: egui::Pos2,
    pub(super) time: f64,
    pub(super) shapes: Vec<ClippedShape>,
    /// Render the whole panel through `render_stage4_panel` (view modes) instead of two bays.
    pub(super) full: bool,
    /// Area the detailed view occupied in the last full-panel frame.
    pub(super) visible: Rect,
}

impl Sim {
    pub(super) fn new() -> Self {
        let (audio, rx) = AudioHandle::new();
        let mut s = Self {
            ctx: egui::Context::default(),
            state: Stage4PanelState::default(),
            audio,
            rx,
            origin: pos2(0.0, 0.0),
            time: 1.0,
            shapes: Vec::new(),
            full: false,
            visible: Rect::NOTHING,
        };
        s.frame(vec![], Modifiers::NONE);
        s
    }

    /// A sim that draws the full panel at a fixed 1:1 zoom (so points equal logical px).
    pub(super) fn full_panel() -> Self {
        let mut s = Self::new();
        s.full = true;
        s.state.view.fixed_zoom = Some(1.0);
        s.frame(vec![], Modifiers::NONE);
        s.frame(vec![], Modifiers::NONE);
        s
    }

    pub(super) fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.time += 0.04;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1700.0, 600.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let (state, audio) = (&mut self.state, &self.audio);
        let mut origin = self.origin;
        let (full, mut visible) = (self.full, self.visible);
        let out = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if full {
                    let region = state.view.region();
                    let top_left = ui.cursor().min;
                    origin = super::view::region_origin(region, top_left);
                    visible = Rect::from_min_size(top_left, super::view::region_size(region));
                    super::render_stage4_panel(ui, state, audio);
                    return;
                }
                let (rect, _) = ui.allocate_exact_size(vec2(1560.0, 420.0), Sense::hover());
                origin = rect.min;
                let painter = ui.painter().clone();
                render_bay_master(ui, &painter, rect, state, audio);
                render_bay_organ(ui, &painter, rect, state, audio);
            });
        });
        self.origin = origin;
        self.visible = visible;
        self.shapes = out.shapes;
    }

    pub(super) fn center(&self, id: &str) -> egui::Pos2 {
        rect_or_empty(id, self.origin).center()
    }

    pub(super) fn press(&mut self, at: egui::Pos2, pressed: bool, m: Modifiers) {
        let ev = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: m,
        };
        self.frame(vec![ev], m);
    }

    /// Tap with optional modifiers.
    pub(super) fn tap_with(&mut self, id: &str, m: Modifiers) {
        let at = self.center(id);
        self.frame(vec![Event::PointerMoved(at)], m);
        self.press(at, true, m);
        self.press(at, false, m);
    }

    pub(super) fn click(&mut self, id: &str) {
        self.tap_with(id, Modifiers::NONE);
    }

    /// Press, keep the button down for `secs`, release.
    pub(super) fn hold(&mut self, id: &str, secs: f64) {
        let at = self.center(id);
        self.frame(vec![Event::PointerMoved(at)], Modifiers::NONE);
        self.press(at, true, Modifiers::NONE);
        self.time += secs;
        self.frame(vec![], Modifiers::NONE);
        self.press(at, false, Modifiers::NONE);
    }

    /// Drag from `a` to `b` in a few steps.
    pub(super) fn drag(&mut self, a: egui::Pos2, b: egui::Pos2) {
        self.frame(vec![Event::PointerMoved(a)], Modifiers::NONE);
        self.press(a, true, Modifiers::NONE);
        for i in 1..=4 {
            let p = a + (b - a) * (i as f32 / 4.0);
            self.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
        }
        self.press(b, false, Modifiers::NONE);
    }

    pub(super) fn cmds(&self) -> Vec<AudioCommand> {
        self.rx.try_iter().collect()
    }

    pub(super) fn organ_edits(&self) -> Vec<OrganEdit> {
        self.cmds()
            .into_iter()
            .filter_map(|c| match c {
                AudioCommand::Organ(e) => Some(e),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_renders_and_a_click_on_the_speed_button_reaches_the_audio_queue() {
        let mut sim = Sim::new();
        assert!(!sim.shapes.is_empty());
        sim.click("rotary.speed");
        assert_eq!(sim.organ_edits().len(), 1);
    }
}
