//! Amp Sim / EQ (manual p.51-52) and Compressor (manual p.52).

use super::ctx::{from_log, to_log, Fx};
use super::ids::dim_reason;
use crate::panel::param_hint::table::{EQ_DB, MID_HZ};
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{ChainUi, Unit};
use crate::style::{LED_RED, LED_RED_DIM};
use nord_core::fx::AmpCabinet;
use nord_panel::behaviour::{cycle, Trigger};

/// Amp models in selector order; the first entry is "no model" (neutral EQ and drive).
const MODELS: [AmpCabinet; 4] = [
    AmpCabinet::Off,
    AmpCabinet::Twin,
    AmpCabinet::Jc,
    AmpCabinet::Small,
];

/// Next model after `cur`; a filter or TO ROTARY selection counts as "no model".
pub(super) fn next_model(cur: AmpCabinet) -> AmpCabinet {
    let i = MODELS.iter().position(|m| *m == cur).unwrap_or(0);
    MODELS[cycle(i, MODELS.len(), true)]
}

/// Next filter after `cur`: LP24, HP24, then off.
pub(super) fn next_filter(cur: AmpCabinet) -> AmpCabinet {
    match cur {
        AmpCabinet::LpFilter => AmpCabinet::HpFilter,
        AmpCabinet::HpFilter => AmpCabinet::Off,
        _ => AmpCabinet::LpFilter,
    }
}

fn eq_knob(f: &mut Fx, id: &'static str, v: &mut f32, name: &str, tip: &str) -> bool {
    let tip = format!("{tip} ({:+.1} dB)", *v);
    let changed = f.knob(id, v, EQ_DB, &tip, None);
    f.name(id, name);
    changed
}

/// Amp Sim / EQ unit.
pub(super) fn amp(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let mut changed = false;
    {
        let c = &mut st.fx_ui.chains[slot];
        let on = f.rect("amp.on");
        f.led_above(on, c.amp_on, LED_RED, LED_RED_DIM);
        if f.button("amp.on", on, c.amp_on, false, "AMP SIM/EQ ON")
            .is_some()
        {
            c.amp_on = !c.amp_on;
            changed = true;
        }

        changed |= f.knob("amp.drive", &mut c.drive, (0.0, 1.0), "DRIVE", None);
        f.name("amp.drive", "DRIVE");
        changed |= eq_knob(f, "amp.bass", &mut c.bass, "BASS", "EQ BASS (100 Hz)");
        changed |= eq_knob(f, "amp.mid", &mut c.mid, "MID", "EQ MID");
        let mut n = to_log(c.mid_freq, MID_HZ.0, MID_HZ.1);
        let tip = format!("EQ MID FREQ ({:.0} Hz)", c.mid_freq);
        if f.knob("amp.mid_freq", &mut n, (0.0, 1.0), &tip, None) {
            c.mid_freq = from_log(n, MID_HZ.0, MID_HZ.1);
            changed = true;
        }
        // The RES knob sits under FREQ, so its caption goes above the knob.
        let mf = f.rect("amp.mid_freq");
        crate::widgets::label(
            f.p,
            egui::pos2(mf.center().x, mf.top() - 5.0),
            "FREQ",
            6.5,
            super::ctx::CAPTION,
            egui::Align2::CENTER_CENTER,
        );
        changed |= eq_knob(
            f,
            "amp.treble",
            &mut c.treble,
            "TREBLE",
            "EQ TREBLE (4 kHz)",
        );
        let tip = "FILTER RES: resonance of LP24 / HP24";
        changed |= f.knob("amp.res", &mut c.res, (0.0, 1.0), tip, None);

        changed |= model_selector(f, c, slot);
    }
    if changed {
        let edits = st.fx_ui.after_edit(slot, Unit::Amp);
        f.send(edits);
    }
}

/// Model button with its LED row, the LP24/HP24 and TO ROTARY companions and the variation legend.
fn model_selector(f: &mut Fx, c: &mut ChainUi, slot: usize) -> bool {
    let mut changed = false;
    let m = f.rect("amp.model");
    let leds = [
        ("TWIN", c.cabinet == AmpCabinet::Twin, ""),
        ("JC", c.cabinet == AmpCabinet::Jc, ""),
        ("SMALL", c.cabinet == AmpCabinet::Small, ""),
    ];
    f.led_row("amp.model", m, &leds);
    let tip = "AMP MODEL: click cycles none, Twin, JC, Small. Shift+click or hold: VARIATION";
    match f.button("amp.model", m, MODELS[1..].contains(&c.cabinet), true, tip) {
        Some(Trigger::Primary) => {
            c.cabinet = next_model(c.cabinet);
            changed = true;
        }
        Some(Trigger::Shifted) => {
            c.amp_variation = !c.amp_variation;
            changed = true;
        }
        _ => {}
    }
    let why = dim_reason("amp.variation", c, slot);
    f.legend(
        "amp.variation",
        "VAR",
        c.amp_variation,
        "AMP VARIATION (Shift + selector): Twin / JC / Small with less treble",
        why,
    );

    // LP24 / HP24 and TO ROTARY share the model button's LEDs on the instrument.
    let flt = f.rect("amp.filter");
    let is_flt = matches!(c.cabinet, AmpCabinet::LpFilter | AmpCabinet::HpFilter);
    let name = match c.cabinet {
        AmpCabinet::LpFilter => "LP24",
        AmpCabinet::HpFilter => "HP24",
        _ => "FLT",
    };
    let tip = format!("AMP FILTER: {name}. Click cycles LP24, HP24, off (replaces the amp model)");
    if f.button("amp.filter", flt, is_flt, false, &tip).is_some() {
        c.cabinet = next_filter(c.cabinet);
        changed = true;
    }
    crate::widgets::label(
        f.p,
        egui::pos2(flt.center().x, flt.top() - 4.0),
        name,
        5.5,
        super::ctx::CAPTION,
        egui::Align2::CENTER_CENTER,
    );

    let rot = f.rect("amp.to_rotary");
    let dim = dim_reason("amp.to_rotary", c, slot);
    let is_rot = c.cabinet == AmpCabinet::ToRotary;
    let tip = "TO ROTARY: send this Layer to the Rotary Speaker (last unit of its chain)";
    if f.button("amp.to_rotary", rot, is_rot, false, tip).is_some() && dim.is_none() {
        c.cabinet = if is_rot {
            AmpCabinet::Off
        } else {
            AmpCabinet::ToRotary
        };
        changed = true;
    }
    crate::widgets::label(
        f.p,
        egui::pos2(rot.center().x, rot.top() - 4.0),
        "ROT",
        5.5,
        super::ctx::CAPTION,
        egui::Align2::CENTER_CENTER,
    );
    if let Some(why) = dim {
        f.dim(rot);
        f.hover(rot, "amp.to_rotary_why", &format!("Not available: {why}"));
    }
    changed
}

/// Compressor unit: ON / GLOBAL, AMOUNT, FAST (Shift + turn of the Amount knob).
pub(super) fn comp(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let mut changed = false;
    let global = st.fx_ui.global_comp.is_some();
    let mut toggle_global = false;
    {
        let c = &mut st.fx_ui.chains[slot];
        let on = f.rect("comp.on");
        f.led_above(on, c.comp_on, LED_RED, LED_RED_DIM);
        match f.button(
            "comp.on",
            on,
            c.comp_on,
            true,
            "COMP ON. Shift+click or hold: GLOBAL",
        ) {
            Some(Trigger::Primary) => {
                c.comp_on = !c.comp_on;
                changed = true;
            }
            Some(Trigger::Shifted) => toggle_global = true,
            _ => {}
        }
        f.legend(
            "comp.global",
            "GLOBAL",
            global,
            "GLOBAL (Shift + On): this compressor applies to all Layers of all Sections",
            None,
        );

        if f.shift {
            // Shift + turn: clockwise switches FAST on, counter-clockwise off.
            let mut v = if c.comp_fast { 1.0 } else { 0.0 };
            if f.knob(
                "comp.amount",
                &mut v,
                (0.0, 1.0),
                "COMP FAST (Shift + Amount)",
                None,
            ) {
                c.comp_fast = v > 0.5;
                changed = true;
                f.hint(
                    "comp.amount",
                    if c.comp_fast { "Fast On" } else { "Fast Off" },
                );
            }
        } else {
            let tip = format!(
                "COMP AMOUNT ({:.0} %). Shift + turn: FAST",
                c.comp_amount * 100.0
            );
            changed |= f.knob("comp.amount", &mut c.comp_amount, (0.0, 1.0), &tip, None);
        }
        f.name("comp.amount", "AMOUNT");
        f.legend(
            "comp.fast",
            "FAST",
            c.comp_fast,
            "FAST (Shift + Amount knob): the compressor recovers quicker",
            None,
        );
    }
    if toggle_global {
        let mut edits = st.fx_ui.toggle_global(Unit::Comp, slot);
        edits.extend(st.fx_ui.chains[slot].edits(Unit::Comp, slot as u8));
        f.send(edits);
    } else if changed {
        let edits = st.fx_ui.after_edit(slot, Unit::Comp);
        f.send(edits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_button_cycles_none_twin_jc_small() {
        let mut m = AmpCabinet::Off;
        let mut seen = Vec::new();
        for _ in 0..4 {
            m = next_model(m);
            seen.push(m);
        }
        assert_eq!(
            seen,
            [
                AmpCabinet::Twin,
                AmpCabinet::Jc,
                AmpCabinet::Small,
                AmpCabinet::Off
            ]
        );
    }

    #[test]
    fn model_button_leaves_filter_and_rotary_selections() {
        assert_eq!(next_model(AmpCabinet::LpFilter), AmpCabinet::Twin);
        assert_eq!(next_model(AmpCabinet::ToRotary), AmpCabinet::Twin);
    }

    #[test]
    fn filter_button_cycles_lp_hp_off() {
        assert_eq!(next_filter(AmpCabinet::Off), AmpCabinet::LpFilter);
        assert_eq!(next_filter(AmpCabinet::LpFilter), AmpCabinet::HpFilter);
        assert_eq!(next_filter(AmpCabinet::HpFilter), AmpCabinet::Off);
        assert_eq!(next_filter(AmpCabinet::Twin), AmpCabinet::LpFilter);
    }
}
