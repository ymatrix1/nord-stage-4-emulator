//! Piano bay, second half: PIANO SELECT (TYPE, dial, LIST/INFO overlays), acoustics, touch,
//! compression, timbre and unison (manual p.24-26). The shared strip is in `bay_piano.rs`.

use super::layout_px::rect_or_empty;
use super::piano_logic as logic;
use super::piano_widgets as w;
use super::prelude::*;
use super::ui_state::{slot, PianoOverlay};
use crate::audio::PianoEdit;
use nord_panel::behaviour::Trigger;

/// Type names in LED order (manual p.24).
pub const TYPE_NAMES: [&str; 6] = ["Grand", "Upright", "Electric", "Clav", "Digital", "Misc"];

const RED: (Color32, Color32) = (LED_RED, LED_RED_DIM);
const AMBER: (Color32, Color32) = (LED_AMBER, LED_AMBER_DIM);

/// Draw the Piano Select, acoustics, touch, comp, timbre and unison controls and the overlays.
pub(super) fn render_select(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    shift: bool,
    out: &mut Vec<PianoEdit>,
) {
    draw_type_and_dial(ui, p, o, state, shift, out);
    draw_acoustics(ui, p, o, state, shift, out);
    draw_option_buttons(ui, p, o, state, out);
    draw_timbre(ui, p, o, state, out);
    super::piano_overlay::draw_overlay(ui, p, o, state);
}

fn draw_type_and_dial(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    shift: bool,
    out: &mut Vec<PianoEdit>,
) {
    let r = |id: &str| rect_or_empty(id, o);

    let ty = r("piano.type");
    let pr = w::press(
        ui,
        ty,
        "piano_type",
        &mut state.piano_ui.trackers[slot::TYPE],
    );
    if let Some(g) = pr.gesture {
        match w::trigger(g, shift, true, true) {
            Trigger::Shifted => {
                state.piano_ui.shift_latch = false;
                state.piano_ui.overlay = if state.piano_ui.overlay == PianoOverlay::Info {
                    PianoOverlay::None
                } else {
                    PianoOverlay::Info
                };
            }
            _ => logic::type_cycle(state, out),
        }
    }
    w::face(p, ty, pr.down, false);
    w::inside(p, ty, "TYPE", false);
    w::legend(p, ty, "INFO", false);
    pr.resp.on_hover_text(
        "TYPE: next of Grand, Upright, Electric, Clav, Digital, Misc. Shift+tap = INFO.",
    );
    state.piano_ui.mark("piano.type");
    state.piano_ui.mark("piano.info");

    let cat = logic::category_index(state);
    w::led_strip(p, r("piano.type_leds"), 6, |i| i == cat, RED, false);
    state.piano_ui.mark("piano.type_leds");

    let dial = r("piano.select_dial");
    let n = state.piano_ui.models.len();
    let pos = if n > 1 {
        state.piano_ui.model_idx as f32 / (n - 1) as f32
    } else {
        0.0
    };
    let mut accum = state.piano_ui.dial_accum;
    let (mv, resp) = w::dial(ui, dial, "piano_dial", pos, &mut accum);
    state.piano_ui.dial_accum = accum;
    if shift && (mv.steps != 0 || mv.clicked) {
        state.piano_ui.overlay = PianoOverlay::List;
        state.piano_ui.shift_latch = false;
    }
    for _ in 0..mv.steps.unsigned_abs() {
        logic::model_step(state, mv.steps > 0);
    }
    if mv.clicked && !shift {
        logic::model_step(state, true);
    }
    w::legend(p, dial, "LIST", false);
    let tip = if n == 0 {
        "PIANO SELECT: no piano library loaded (open an .ns4b or a .npno). Turn or click to cycle models; Shift+turn = LIST."
    } else {
        "PIANO SELECT: turn (scroll or drag) or click to cycle the library models; Shift+turn = LIST."
    };
    resp.on_hover_text(tip);
    state.piano_ui.mark("piano.select_dial");
    state.piano_ui.mark("piano.list");
}

fn draw_acoustics(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    shift: bool,
    out: &mut Vec<PianoEdit>,
) {
    let l = state.piano_ui.layer();
    let clav = state.piano_cat == PianoCategory::Clav;

    // SOFT RELEASE (dimmed on Clav: manual p.25 "Clavinets and Harpsichords do not support it").
    let soft = rect_or_empty("piano.soft_release", o);
    let pr = w::press(
        ui,
        soft,
        "piano_soft",
        &mut state.piano_ui.trackers[slot::SOFT_RELEASE],
    );
    if pr.gesture.is_some() && !clav {
        logic::soft_release_toggle(state, out);
    }
    w::face(p, soft, pr.down, clav);
    w::inside(p, soft, "SOFT REL", clav);
    w::mini_led(
        p,
        pos2(soft.left() - 5.0, soft.center().y),
        state.piano_ui.soft_release[l],
        LED_RED,
        LED_RED_DIM,
        2.2,
        clav,
    );
    pr.resp.on_hover_text(if clav {
        "SOFT RELEASE is disabled for Clavinets and Harpsichords (manual p.25)"
    } else {
        "SOFT RELEASE of the focused layer"
    });
    if clav {
        state.piano_ui.mark_dim(
            "piano.soft_release",
            "SOFT RELEASE: not available for the Clav type (manual p.25)",
        );
    } else {
        state.piano_ui.mark("piano.soft_release");
    }

    // STRING RES (tap) and PED NOISE (Shift): the layout lists them as one ACOUSTICS button on the
    // SOFT RELEASE rectangle; the STRING RES button sits just below it.
    let sr = soft.translate(vec2(0.0, soft.height() + 11.0));
    let pr = w::press(
        ui,
        sr,
        "piano_stringres",
        &mut state.piano_ui.trackers[slot::STRING_RES],
    );
    if let Some(g) = pr.gesture {
        match w::trigger(g, shift, true, true) {
            Trigger::Shifted => {
                state.piano_ui.shift_latch = false;
                logic::ped_noise_toggle(state, out);
            }
            _ => logic::string_res_toggle(state, out),
        }
    }
    w::face(p, sr, pr.down, false);
    w::inside(p, sr, "STR RES", false);
    w::legend(p, sr, "PED NOISE", false);
    w::mini_led(
        p,
        pos2(sr.left() - 5.0, sr.center().y),
        state.string_res,
        LED_RED,
        LED_RED_DIM,
        2.2,
        false,
    );
    w::mini_led(
        p,
        pos2(sr.right() + 5.0, sr.center().y),
        state.pedal_noise,
        AMBER.0,
        AMBER.1,
        2.2,
        false,
    );
    pr.resp.on_hover_text(
        "STRING RES (left LED). Shift+tap = PED NOISE (right LED). Both apply to both layers.",
    );
    state.piano_ui.mark("piano.string_res");
    state.piano_ui.mark("piano.ped_noise");
}

fn draw_option_buttons(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    out: &mut Vec<PianoEdit>,
) {
    let l = state.piano_ui.layer();
    let touch = logic::touch_index(state);
    let comp = usize::from(state.piano_ui.dyn_comp[l]);
    let uni = usize::from(state.piano_ui.unison[l]);

    type Cycle = fn(&mut Stage4PanelState, &mut Vec<PianoEdit>);
    let specs: [(&'static str, &'static str, usize, Cycle, usize, &str); 3] = [
        (
            "piano.kb_touch",
            "TOUCH",
            slot::KB_TOUCH,
            logic::touch_cycle,
            touch,
            "KB TOUCH: Heavy, Medium, Light (LEDs top to bottom order left to right).",
        ),
        (
            "piano.dyn_comp",
            "DYN COMP",
            slot::DYN_COMP,
            logic::dyn_comp_cycle,
            comp.wrapping_sub(1),
            "DYN COMP: off, 1, 2, 3 for the focused layer.",
        ),
        (
            "piano.unison",
            "UNISON",
            slot::UNISON,
            logic::unison_cycle,
            uni.wrapping_sub(1),
            "UNISON: off, 1, 2, 3 for the focused layer.",
        ),
    ];
    for (id, text, tr, cycle, lit, tip) in specs {
        let rect = rect_or_empty(id, o);
        let pr = w::press(ui, rect, id, &mut state.piano_ui.trackers[tr]);
        if pr.gesture.is_some() {
            cycle(state, out);
        }
        w::face(p, rect, pr.down, false);
        w::inside(p, rect, text, false);
        let row = Rect::from_min_size(
            pos2(rect.left(), rect.bottom() + 3.0),
            vec2(rect.width(), 6.0),
        );
        w::led_strip(p, row, 3, |i| i == lit, RED, false);
        pr.resp.on_hover_text(tip);
        state.piano_ui.mark(id);
    }
}

fn draw_timbre(
    ui: &mut Ui,
    p: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    out: &mut Vec<PianoEdit>,
) {
    let clav = state.piano_cat == PianoCategory::Clav;
    let rect = rect_or_empty("piano.timbre", o);
    let pr = w::press(
        ui,
        rect,
        "piano_timbre",
        &mut state.piano_ui.trackers[slot::TIMBRE],
    );
    if pr.gesture.is_some() {
        logic::timbre_cycle(state, out);
    }
    w::face(p, rect, pr.down, clav);
    w::inside(p, rect, "TIMB", clav);

    let (names, lit): (&[&str], u8) = if clav {
        (&["SOFT", "TREBLE", "BRIL"], state.piano_ui.clav_timbre)
    } else {
        let idx = logic::timbre_index(state);
        (
            &["SOFT", "MID", "BRIGHT", "DYNO1", "DYNO2"],
            if idx == 0 { 0 } else { 1 << (idx - 1) },
        )
    };
    let step = 8.5;
    let top = rect.center().y - step * 2.0;
    for (i, name) in names.iter().enumerate() {
        let c = pos2(rect.right() + 7.0, top + i as f32 * step);
        let on = lit & (1 << i) != 0;
        w::mini_led(p, c, on, LED_RED, LED_RED_DIM, 2.2, clav);
        label(
            p,
            c + vec2(5.0, 0.0),
            name,
            5.5,
            Color32::from_gray(if clav { 110 } else { 225 }),
            Align2::LEFT_CENTER,
        );
    }
    if clav {
        pr.resp.on_hover_text(
            "TIMBRE on Clav: the seven pick-up EQ combinations (Soft, Treble, Brilliant and their mixes, manual p.26) are kept in the UI; the engine has no Clav timbre yet.",
        );
        state.piano_ui.mark_dim(
            "piano.timbre",
            "TIMBRE (Clav): engine has no Clav pick-up EQ timbres",
        );
    } else {
        pr.resp.on_hover_text(
            "TIMBRE: off, Soft, Mid, Bright, Dyno 1, Dyno 2 (applies to both layers).",
        );
        state.piano_ui.mark("piano.timbre");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    fn render(
        state_setup: impl FnOnce(&mut Stage4PanelState),
    ) -> (usize, Vec<&'static str>, Vec<(&'static str, &'static str)>) {
        let mut setup = Some(state_setup);
        let mut ids = vec![];
        let mut dims = vec![];
        let n = shapes_painted(|ui, p, rect, s, _a| {
            if let Some(f) = setup.take() {
                f(s);
            }
            s.piano_ui.begin_frame();
            let mut out = vec![];
            render_select(ui, p, rect.min, s, false, &mut out);
            ids = s.piano_ui.drawn.clone();
            dims = s.piano_ui.dimmed.clone();
        });
        (n, ids, dims)
    }

    #[test]
    fn select_group_paints_its_twelve_ids() {
        let (n, ids, dims) = render(|_| {});
        assert!(n > 60, "{n}");
        for id in [
            "piano.type",
            "piano.type_leds",
            "piano.select_dial",
            "piano.list",
            "piano.info",
            "piano.soft_release",
            "piano.string_res",
            "piano.ped_noise",
            "piano.kb_touch",
            "piano.dyn_comp",
            "piano.timbre",
            "piano.unison",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
        assert!(
            dims.is_empty(),
            "nothing dimmed for the Grand type: {dims:?}"
        );
    }

    #[test]
    fn clav_type_dims_timbre_and_soft_release_with_a_reason() {
        let (_, ids, dims) = render(|s| s.piano_cat = PianoCategory::Clav);
        assert_eq!(ids.len(), 12);
        let d: Vec<_> = dims.iter().map(|d| d.0).collect();
        assert_eq!(d, ["piano.soft_release", "piano.timbre"]);
        assert!(dims[1].1.contains("Clav"));
    }

    #[test]
    fn overlays_paint_more_than_the_closed_group() {
        let (closed, _, _) = render(|_| {});
        let (list, _, _) = render(|s| {
            s.piano_ui.models = vec!["x/A.npno".into(), "x/B.npno".into()];
            s.piano_ui.overlay = PianoOverlay::List;
        });
        let (info, _, _) = render(|s| s.piano_ui.overlay = PianoOverlay::Info);
        assert!(
            list > closed + 5 && info > closed + 3,
            "{closed} {list} {info}"
        );
    }

    #[test]
    fn type_names_match_the_category_list() {
        assert_eq!(TYPE_NAMES.len(), logic::CATEGORIES.len());
    }
}
