//! Bay 6: Layer Effects of the front panel (Phase 6b B8f, `ns4-hcuh`).
//!
//! Every control of the inventory section `Effects` (51 ids, `ids::HANDLED`) is placed with
//! `layout_px::rect_or_empty`; Shift / hold-to-shift functions share their base control's
//! rectangle and appear as legends below it. The bay edits the chain of the focused FX slot
//! (Organ, Piano A/B, Synth A/B/C) through `AudioCommand::Fx`; the GUI view of the six chains
//! lives in `ui_state::fx`. Spec: `docs/specs/effects-and-routing.md`, manual p.48-53.

mod amp;
mod ctx;
mod delay;
mod focus;
mod ids;
mod lists;
mod modulation;
mod reverb;
#[cfg(test)]
mod tests;

use super::layout_px::to_px;
use super::prelude::*;
use super::ui_state::fx::{slot_for, StateEcho};
use ctx::Fx;
use nord_panel::{section_rect, Section};

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_effects(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let origin = panel_rect.min;
    section_frame(
        painter,
        to_px(section_rect(Section::Effects), origin),
        "LAYER EFFECTS",
    );
    super::frames::paint_frames(painter, origin, Section::Effects);

    let (now_ms, shift) = ui.input(|i| ((i.time * 1000.0) as u32, i.modifiers.shift));
    // SECTION EDIT (manual p.43): a unit edit reaches every chain of the section.
    state.fx_ui.section_edit = state.program_ui.section_edit_active();
    let trackers = std::mem::take(&mut state.fx_ui.trackers);
    let mut f = Fx {
        ui,
        p: painter,
        origin,
        audio: audio_handle,
        now_ms,
        shift,
        trackers,
    };

    // A program load writes the legacy `mod1_type`, `delay_on`, ... fields; adopt that first.
    let slot = slot_for(state.fx_focus, state.fx_ui.piano_b);
    state.fx_ui.pull_external(StateEcho::read(state), slot);

    focus::focus(&mut f, state);
    let slot = slot_for(state.fx_focus, state.fx_ui.piano_b);
    modulation::mod1(&mut f, state, slot);
    modulation::mod2(&mut f, state, slot);
    delay::delay(&mut f, state, slot);
    amp::amp(&mut f, state, slot);
    amp::comp(&mut f, state, slot);
    reverb::reverb(&mut f, state, slot);

    // Keep the legacy fields a write-through view of the focused chain.
    let echo = StateEcho::of(&state.fx_ui.chains[slot]);
    echo.write_to(state);
    state.fx_ui.set_echo(echo);
    state.fx_ui.trackers = f.trackers;
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn writing_the_legacy_view_follows_the_focused_chain() {
        let mut state = Stage4PanelState::default();
        state.fx_ui.chains[0].mod1_on = true;
        state.fx_ui.chains[0].mod1_sel = 1;
        let n = shapes_painted(|ui, p, rect, _s, audio| {
            render_bay_effects(ui, p, rect, &mut state, audio);
        });
        assert!(n > 100);
        assert_eq!(state.mod1_type, nord_core::fx::Mod1Type::Tremolo);
    }
}
