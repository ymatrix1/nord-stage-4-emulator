//! FX focus column (manual p.48): LAYER EFFECTS ON, ORGAN / PIANO / SYNTH focus buttons with
//! GROUP on Shift + Piano / Synth, and the focus LEDs.

use super::ctx::{Fx, CAPTION};
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{section_of_slot, section_slots, slot_for, FocusSection, Unit};
use crate::style::{LED_AMBER, LED_AMBER_DIM, LED_RED, LED_RED_DIM};
use egui::{pos2, Color32, Stroke};
use nord_panel::behaviour::Trigger;

/// Units re-sent when LAYER EFFECTS ON flips (the engine gets neutral values while it is off).
const ALL_UNITS: [Unit; 6] = [
    Unit::Mod1,
    Unit::Mod2,
    Unit::Amp,
    Unit::Comp,
    Unit::Delay,
    Unit::Reverb,
];

/// Draw and operate the focus column; may change `st.fx_focus` (callers re-derive the slot).
pub(super) fn focus(f: &mut Fx, st: &mut Stage4PanelState) {
    let slot = slot_for(st.fx_focus, st.fx_ui.piano_b);

    // LAYER EFFECTS ON: gates every unit of the focused chain at once.
    let on = f.rect("fx.layer_effects_on");
    let all_on = st.fx_ui.chains[slot].all_on;
    f.led_above(on, all_on, LED_RED, LED_RED_DIM);
    if f.button(
        "fx.layer_effects_on",
        on,
        all_on,
        false,
        "LAYER EFFECTS ON: all effects of the focused Layer on or off",
    )
    .is_some()
    {
        st.fx_ui.chains[slot].all_on = !all_on;
        let mut edits = Vec::new();
        for u in ALL_UNITS {
            edits.extend(st.fx_ui.after_edit(slot, u));
        }
        f.send(edits);
    }
    f.caption(on, 0, "LAYER FX", CAPTION);

    for (id, group_id, section, name) in [
        ("fx.focus.organ", "", FocusSection::Organ, "ORGAN"),
        (
            "fx.focus.piano",
            "fx.focus.group_piano",
            FocusSection::Piano,
            "PIANO",
        ),
        (
            "fx.focus.synth",
            "fx.focus.group_synth",
            FocusSection::Synth,
            "SYNTH",
        ),
    ] {
        let r = f.rect(id);
        let lit = section_of_slot(slot_for(st.fx_focus, st.fx_ui.piano_b)) == section;
        let has_group = section != FocusSection::Organ;
        f.led_above(r, lit, LED_AMBER, LED_AMBER_DIM);
        let tip = if has_group {
            format!("FX FOCUS {name}: press again for the next Layer. Shift+click or hold: GROUP")
        } else {
            "FX FOCUS ORGAN (Organ A and B share one effects chain)".to_string()
        };
        match f.button(id, r, lit, has_group, &tip) {
            Some(Trigger::Primary) => st.fx_ui.focus_press(&mut st.fx_focus, section),
            Some(Trigger::Shifted) => {
                let s = slot_for(st.fx_focus, st.fx_ui.piano_b);
                let edits = st.fx_ui.toggle_group(section, s);
                f.send(edits);
            }
            _ => {}
        }
        f.caption(r, 0, name, CAPTION);
        if has_group {
            let on = st.fx_ui.group_of(section).is_some();
            f.legend(
                group_id,
                "GROUP",
                on,
                "GROUP (Shift + FX Focus): all Layers of the Section share one set of effects",
                None,
            );
        }
    }

    // FX FOCUS LEDs: Organ, Piano A/B, Synth A/B/C; GROUP lights the whole section.
    let slot = slot_for(st.fx_focus, st.fx_ui.piano_b);
    let r = f.rect("fx.focus.leds");
    let sec = section_of_slot(slot);
    let group = st.fx_ui.group_of(sec).is_some();
    for i in 0..6 {
        let lit = i == slot || (group && section_slots(sec).contains(&i));
        let c = pos2(r.left() + r.width() * (i as f32 + 0.5) / 6.0, r.center().y);
        let col = if lit {
            LED_AMBER
        } else {
            Color32::from_gray(60)
        };
        f.p.circle_filled(c, 1.7, col);
        f.p.circle_stroke(c, 1.7, Stroke::new(0.8_f32, Color32::from_black_alpha(140)));
    }
    let names = [
        "ORGAN", "PIANO A", "PIANO B", "SYNTH A", "SYNTH B", "SYNTH C",
    ];
    f.hover(r, "fx.focus.leds", &format!("FX FOCUS: {}", names[slot]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_units_are_listed_once() {
        for (i, a) in ALL_UNITS.iter().enumerate() {
            assert!(ALL_UNITS[i + 1..].iter().all(|b| b != a));
        }
        assert_eq!(ALL_UNITS.len(), 6);
    }
}
