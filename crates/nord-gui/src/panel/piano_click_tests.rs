//! Pointer-driven tests of the Piano bay: real egui press/release events through
//! `render_bay_piano`, checking panel state and the `PianoEdit`s sent to the audio thread.

use super::bay_piano::render_bay_piano;
use super::layout_px::rect_or_empty;
use super::prelude::*;
use crate::audio::PianoEdit;
use egui::{Event, Modifiers, PointerButton, RawInput};

struct Rig {
    ctx: egui::Context,
    handle: AudioHandle,
    rx: crossbeam_channel::Receiver<AudioCommand>,
    origin: egui::Pos2,
    clock: f64,
}

impl Rig {
    fn new(state: &mut Stage4PanelState) -> Self {
        let (handle, rx) = AudioHandle::new();
        let mut rig = Self {
            ctx: egui::Context::default(),
            handle,
            rx,
            origin: pos2(0.0, 0.0),
            clock: 0.0,
        };
        rig.frame(state, vec![], 0.0, false);
        rig
    }

    fn frame(&mut self, state: &mut Stage4PanelState, events: Vec<Event>, time: f64, shift: bool) {
        let mut raw = RawInput {
            events,
            time: Some(time),
            ..RawInput::default()
        };
        if shift {
            raw.modifiers = Modifiers::SHIFT;
        }
        let mut origin = self.origin;
        let handle = &self.handle;
        let _ = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(1560.0, 420.0), Sense::hover());
                origin = rect.min;
                let painter = ui.painter().clone();
                render_bay_piano(ui, &painter, rect, state, handle);
            });
        });
        self.origin = origin;
    }

    /// Press on the centre of `id`, keep it down for `hold` seconds, release; return the edits.
    fn press(
        &mut self,
        state: &mut Stage4PanelState,
        at: egui::Pos2,
        hold: f64,
        shift: bool,
    ) -> Vec<PianoEdit> {
        let mods = if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        let btn = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: mods,
        };
        let t0 = self.clock + 10.0;
        self.frame(state, vec![Event::PointerMoved(at)], t0, shift);
        self.frame(state, vec![btn(true)], t0, shift);
        if hold > 0.2 {
            self.frame(state, vec![], t0 + hold, shift);
        }
        self.clock = t0 + hold + 0.05;
        self.frame(state, vec![btn(false)], self.clock, shift);
        self.rx
            .try_iter()
            .filter_map(|c| match c {
                AudioCommand::Piano(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    fn centre(&self, id: &str) -> egui::Pos2 {
        rect_or_empty(id, self.origin).center()
    }
}

fn tap(state: &mut Stage4PanelState, id: &str) -> Vec<PianoEdit> {
    let mut rig = Rig::new(state);
    let at = rig.centre(id);
    rig.press(state, at, 0.05, false)
}

#[test]
fn on_tap_switches_the_section_off() {
    let mut s = Stage4PanelState::default();
    let edits = tap(&mut s, "piano.on");
    assert!(!s.piano_a_active);
    assert_eq!(
        edits,
        [PianoEdit::LayerActive {
            layer: 0,
            on: false
        }]
    );
}

#[test]
fn on_hold_is_solo_and_release_restores_the_scene() {
    let mut s = Stage4PanelState {
        organ_a_active: true,
        ..Stage4PanelState::default()
    };
    let mut rig = Rig::new(&mut s);
    let at = rig.centre("piano.on");
    let edits = rig.press(&mut s, at, 0.7, false);
    assert_eq!(
        edits,
        [PianoEdit::SceneMask(0b100), PianoEdit::SceneMask(0b101)]
    );
    assert!(s.piano_a_active, "solo is temporary and not stored");
    assert!(s.piano_ui.solo_saved.is_none());
}

#[test]
fn layer_b_tap_replaces_layer_a() {
    let mut s = Stage4PanelState::default();
    let edits = tap(&mut s, "piano.layer.b");
    assert!(s.piano_b_active && !s.piano_a_active);
    assert_eq!(s.piano_ui.focus, 1);
    assert_eq!(edits.len(), 2);
}

#[test]
fn layer_hold_turns_a_layer_off_when_both_are_on() {
    let mut s = Stage4PanelState {
        piano_b_active: true,
        ..Stage4PanelState::default()
    };
    let mut rig = Rig::new(&mut s);
    let at = rig.centre("piano.layer.a");
    let edits = rig.press(&mut s, at, 0.7, false);
    assert!(!s.piano_a_active && s.piano_b_active);
    assert_eq!(
        edits,
        [PianoEdit::LayerActive {
            layer: 0,
            on: false
        }]
    );
}

#[test]
fn shift_tap_on_layer_a_toggles_sustped_ui_state() {
    let mut s = Stage4PanelState::default();
    let mut rig = Rig::new(&mut s);
    let at = rig.centre("piano.layer.a");
    let edits = rig.press(&mut s, at, 0.05, true);
    assert!(s.piano_ui.sustped);
    assert!(edits.is_empty(), "SUSTPED has no engine effect");
}

#[test]
fn fader_click_sets_the_level() {
    let mut s = Stage4PanelState::default();
    let mut rig = Rig::new(&mut s);
    let r = rect_or_empty("piano.fader.b", rig.origin);
    let at = pos2(r.center().x, r.top() + 1.0);
    let edits = rig.press(&mut s, at, 0.05, false);
    assert!(s.piano_b_level > 0.9);
    assert!(matches!(edits[0], PianoEdit::Level { layer: 1, .. }));
}

#[test]
fn octave_buttons_step_and_shift_moves_the_zone_led_only() {
    let mut s = Stage4PanelState::default();
    let edits = tap(&mut s, "piano.octave_up");
    assert_eq!(s.piano_ui.octave[0], 1);
    assert_eq!(
        edits,
        [PianoEdit::OctaveShift {
            layer: 0,
            octaves: 1
        }]
    );
    let mut rig = Rig::new(&mut s);
    let at = rig.centre("piano.octave_down");
    let edits = rig.press(&mut s, at, 0.05, true);
    assert!(edits.is_empty());
    assert_eq!(s.piano_ui.octave[0], 1);
    assert_eq!(s.piano_ui.kb_zone[0], 3);
}

#[test]
fn type_tap_cycles_and_shift_tap_opens_info() {
    let mut s = Stage4PanelState::default();
    let edits = tap(&mut s, "piano.type");
    assert_eq!(edits, [PianoEdit::Category(PianoCategory::Upright)]);
    let mut rig = Rig::new(&mut s);
    rig.clock = 100.0; // a fresh clock must not look like a double tap of the first press
    let at = rig.centre("piano.info");
    rig.press(&mut s, at, 0.05, true);
    assert_eq!(s.piano_ui.overlay, super::ui_state::PianoOverlay::Info);
}

#[test]
fn acoustics_buttons_send_their_edits() {
    let mut s = Stage4PanelState::default();
    assert_eq!(
        tap(&mut s, "piano.soft_release"),
        [PianoEdit::SoftRelease { layer: 0, on: true }]
    );
    let mut s = Stage4PanelState::default();
    let mut rig = Rig::new(&mut s);
    let sr = rect_or_empty("piano.soft_release", rig.origin);
    let at = sr.translate(vec2(0.0, sr.height() + 11.0)).center();
    assert_eq!(
        rig.press(&mut s, at, 0.05, false),
        [PianoEdit::StringRes(false)]
    );
    assert_eq!(
        rig.press(&mut s, at, 0.05, true),
        [PianoEdit::PedalNoise(false)]
    );
}

#[test]
fn touch_comp_unison_timbre_buttons_send_their_edits() {
    let mut s = Stage4PanelState::default();
    assert_eq!(
        tap(&mut s, "piano.kb_touch"),
        [PianoEdit::Touch(VelocityTouch::Light)]
    );
    assert_eq!(
        tap(&mut s, "piano.dyn_comp"),
        [PianoEdit::DynComp {
            layer: 0,
            amount: 1
        }]
    );
    assert_eq!(
        tap(&mut s, "piano.unison"),
        [PianoEdit::Unison {
            layer: 0,
            amount: 1
        }]
    );
    assert_eq!(
        tap(&mut s, "piano.timbre"),
        [PianoEdit::Timbre(PianoTimbre::Soft)]
    );
}

#[test]
fn clav_timbre_press_sends_nothing_to_the_engine() {
    let mut s = Stage4PanelState {
        piano_cat: PianoCategory::Clav,
        ..Stage4PanelState::default()
    };
    assert!(tap(&mut s, "piano.timbre").is_empty());
    assert_eq!(s.piano_ui.clav_timbre, 1);
}

#[test]
fn dial_click_requests_the_next_library_model_and_shift_opens_the_list() {
    let mut s = Stage4PanelState::default();
    s.piano_ui.models = vec!["a.npno".into(), "b.npno".into()];
    let mut rig = Rig::new(&mut s);
    let at = rig.centre("piano.select_dial");
    rig.press(&mut s, at, 0.05, false);
    assert_eq!(s.piano_ui.model_request.as_deref(), Some("b.npno"));
    rig.press(&mut s, at, 0.05, true);
    assert_eq!(s.piano_ui.overlay, super::ui_state::PianoOverlay::List);
}
