//! Bay 4: Program & Morph of the front panel (Phase 6b B8d, `ns4-z6fi`).
//!
//! Every control of the Program area is placed with `layout_px::rect_or_empty(id, origin)`:
//! OLED, Program dial (+ LIST), Program buttons 1-8 (+ Shift menus), PAGE/BANK, PROG VIEW,
//! LIVE MODE / NUM PAD, STORE / STORE AS, SOLO / UNDO, SECTION EDIT / LAYER INIT, MON/COPY /
//! PASTE, LAYER SCENE II, MORPH ASSIGN, SPLIT, MST CLK, TRANSP / PANIC, PRESET LIBRARY and the
//! MIDI / EXTERN / split LEDs. Behaviour lives in the submodules; this file paints the bay and
//! feeds the pointer into them.

#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

pub mod actions;
#[cfg(test)]
mod binding_tests;
pub mod catalog;
pub mod clock;
pub mod controls;
#[cfg(test)]
mod coverage_tests;
pub mod edited;
pub mod frame;
pub mod fw_messages;
pub mod interact;
pub mod list;
pub mod menu_actions;
pub mod menu_screen;
#[cfg(test)]
mod menu_tests;
pub mod model;
pub mod morph;
pub mod name;
pub mod nav;
pub mod oled;
pub mod oled_hint;
pub mod oled_home;
pub mod oled_paint;
pub mod organize;
pub mod organize_presets;
pub mod preset_browse;
pub mod preset_store;
pub mod preset_user;
pub mod soft;
pub mod split;
pub mod store;

use super::layout_px::{rect_or_empty, to_px};
use super::prelude::*;
use controls::{BUTTONS, MORPH_BUTTONS, PROGRAM_BUTTONS};
use nord_core::organ::OrganModel;
use nord_core::piano::label::PianoLabel;
use nord_panel::{layout, Section};

/// What the display needs from the rest of the panel.
fn summary(state: &Stage4PanelState) -> oled::Summary {
    // The loaded name is the `.npno` header name (`Pearl Upright#GRT 120   Lrg`): the model and
    // the size letter are what the display shows.
    let label = state.loaded_piano_name.as_deref().map(PianoLabel::parse);
    let piano_size = label.as_ref().and_then(|l| l.size);
    let piano = label.map_or_else(|| format!("{:?}", state.piano_cat), |l| l.model);
    let synth = if state.synth_osc_mode == SynthOscMode::Sample {
        state
            .loaded_sample_name
            .clone()
            .unwrap_or_else(|| "Sample".to_string())
    } else {
        format!("{:?}", state.synth_waveform)
    };
    oled::Summary {
        organ: format!("{:?}", state.organ_model),
        piano,
        synth,
        piano_size,
        organ_bars: (state.organ_model == OrganModel::B3).then_some(state.organ_drawbars),
        layers_on: [
            state.organ_a_active,
            state.organ_b_active,
            state.piano_a_active,
            state.piano_b_active,
            state.synth_a_active,
            state.synth_b_active,
            state.synth_c_active,
        ],
    }
}

fn led_dot(painter: &Painter, rect: Rect, on: bool, col: Color32, dim: Color32) {
    let r = (rect.width().max(rect.height()) * 0.5).max(1.5);
    #[cfg(test)]
    crate::widgets::probe::led(painter, rect.center(), r + 1.0);
    painter.circle_filled(rect.center(), r + 1.0, Color32::from_rgb(14, 16, 19));
    painter.circle_filled(rect.center(), r, if on { col } else { dim });
}

fn dial(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    pu: &mut super::ui_state::ProgramUi,
    shift_down: bool,
) -> i32 {
    let resp = ui.interact(rect, ui.id().with("program_dial"), Sense::click_and_drag());
    if resp.dragged() {
        pu.dial_accum -= resp.drag_delta().y;
    }
    if resp.hovered() {
        pu.dial_accum += ui.input(|i| i.raw_scroll_delta.y) * 0.25;
    }
    let steps = (pu.dial_accum / 5.0).trunc() as i32;
    pu.dial_accum -= steps as f32 * 5.0;
    pu.dial_pos += steps;
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    painter.circle_filled(c, r + 1.5, BUTTON_BEZEL);
    painter.circle_filled(c, r, Color32::from_gray(150));
    painter.circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_gray(60)));
    let a = pu.dial_pos as f32 * 0.35;
    painter.line_segment(
        [c, c + vec2(a.sin(), -a.cos()) * (r * 0.9)],
        Stroke::new(1.5_f32, Color32::BLACK),
    );
    label(
        painter,
        pos2(c.x, rect.bottom() + 3.0),
        "LIST",
        4.5,
        if shift_down {
            LED_AMBER
        } else {
            Color32::from_rgb(235, 190, 190)
        },
        Align2::CENTER_TOP,
    );
    resp.on_hover_text("PROGRAM dial: drag or scroll. Shift + dial = LIST (Num / Abc / Cat)");
    steps
}

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_program(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let origin = panel_rect.min;
    let now = (ui.input(|i| i.time) * 1000.0) as u32;
    let kb_shift = ui.input(|i| i.modifiers.shift);
    let sum = summary(state);
    let keys = state.midi_active_keys;
    state
        .program_ui
        .begin_frame(now, &keys, &state.prog_id, &state.prog_title);
    state.program_ui.sync_edits(audio_handle.edit_count());
    state.program_ui.undo_blocked = state.telemetry.undo_blocked();
    state.program_ui.redo_available = state.telemetry.redo_available();

    let area = to_px(layout::section_rect(Section::Program), origin);
    painter.rect_filled(area, 5.0, PROGRAM_RED);
    painter.rect_stroke(
        area,
        5.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    super::frames::paint_frames(painter, origin, Section::Program);

    let pu = &mut state.program_ui;
    let mut cmds = Vec::new();

    // Display: painted text plus a tooltip with the full text (the real OLED is tiny at 1:1).
    let mut text = oled::compose(pu, &sum);
    oled_hint::apply(
        &mut text,
        pu.screen,
        super::param_hint::active(ui.ctx()).as_deref(),
    );
    let oled_rect = rect_or_empty("program.oled", origin);
    oled::paint(painter, oled_rect, &text, false);
    ui.interact(oled_rect, ui.id().with("program_oled"), Sense::hover())
        .on_hover_text(text.plain());
    pu.mark("program.oled");

    // SHIFT buttons first: their state decides how every other button resolves.
    let mut shift_down = kb_shift;
    for spec in &BUTTONS[..2] {
        interact::handle_button(ui, painter, origin, pu, spec, now, shift_down, &mut cmds);
        shift_down |= pu.trackers.get(spec.id).is_some_and(|t| t.is_down());
    }
    if !shift_down {
        pu.shift_used = false;
    }

    let steps = dial(
        ui,
        painter,
        rect_or_empty("program.dial", origin),
        pu,
        shift_down,
    );
    if steps != 0 {
        if shift_down {
            pu.shift_used = true;
        }
        cmds.extend(pu.dial(steps, shift_down));
    }
    pu.mark("program.dial");
    pu.mark("program.list");

    for spec in PROGRAM_BUTTONS
        .iter()
        .chain(&BUTTONS[2..])
        .chain(&MORPH_BUTTONS)
    {
        interact::handle_button(ui, painter, origin, pu, spec, now, shift_down, &mut cmds);
    }

    // LEDs: MIDI / EXTERN activity and the L / M / H split LEDs.
    led_dot(
        painter,
        rect_or_empty("program.midi_led", origin),
        pu.midi_led_on() || state.leds.midi_on(),
        LED_GREEN,
        LED_GREEN_DIM,
    );
    led_dot(
        painter,
        rect_or_empty("program.extern_led", origin),
        pu.extern_led_on() || state.leds.extern_on(),
        LED_AMBER,
        LED_AMBER_DIM,
    );
    pu.mark("program.midi_led");
    pu.mark("program.extern_led");
    let strip = rect_or_empty("split.leds", origin);
    for (i, on) in state
        .telemetry
        .split_leds(pu.split.leds())
        .into_iter()
        .enumerate()
    {
        let cx = strip.left() + strip.width() * (i as f32 + 0.5) / 3.0;
        led_dot(
            painter,
            Rect::from_center_size(pos2(cx, strip.center().y), vec2(3.0, 3.0)),
            on,
            LED_GREEN,
            LED_GREEN_DIM,
        );
        label(
            painter,
            pos2(cx, strip.top() - 1.0),
            ["L", "M", "H"][i],
            4.0,
            Color32::WHITE,
            Align2::CENTER_BOTTOM,
        );
    }
    pu.mark("split.leds");

    if pu.needs_repaint() || state.leds.needs_repaint() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(40));
    }

    // Mirror the area's state into the shared panel fields and send the commands.
    if let Some(m) = pu.organ_request.take() {
        state.organ_model = m;
    }
    let pu = &mut state.program_ui;
    state.split_active = pu.split.on;
    state.morph_wheel_active = pu.morph.assigning() == Some(morph::SOURCES[0]);
    state.morph_at_active = pu.morph.assigning() == Some(morph::SOURCES[1]);
    state.morph_ctrl_active = pu.morph.assigning() == Some(morph::SOURCES[2]);
    pu.write_back(
        &mut state.prog_id,
        &mut state.prog_title,
        &mut state.active_program_idx,
    );
    for c in cmds {
        audio_handle.send_command(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;
    use nord_core::piano::label::PianoSize;

    #[test]
    fn render_bay_program_paints_shapes() {
        let n = shapes_painted(render_bay_program);
        assert!(n > 100, "render_bay_program painted only {n} shapes");
    }

    #[test]
    fn summary_reads_the_layer_flags_and_sound_names() {
        let mut s = Stage4PanelState::default();
        s.loaded_piano_name = Some("Astoria".into());
        let sum = summary(&s);
        assert_eq!(sum.piano, "Astoria");
        assert_eq!(sum.piano_size, None);
        assert!(sum.layers_on[0] && !sum.layers_on[6]);
        assert_eq!(sum.organ, "B3");
        assert_eq!(sum.organ_bars, Some([8, 8, 8, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn summary_splits_the_header_name_and_hides_bars_off_the_b3() {
        let mut s = Stage4PanelState::default();
        s.loaded_piano_name = Some("Astoria Grand#Stw O     Lrg".into());
        s.organ_model = OrganModel::Vox;
        let sum = summary(&s);
        assert_eq!(sum.piano, "Astoria Grand");
        assert_eq!(sum.piano_size, Some(PianoSize::Large));
        assert_eq!(sum.organ_bars, None, "only the B3 has drawbars");
    }
}
