//! Proof for the `ui:fx.*` bindings of the control inventory (`ns4-lu50`): each ON button is
//! clicked on the painted bay, the edits it sends are applied to a real engine and the chain of
//! the focused slot (Organ, `fx_chains[0]`) is asserted. The proven ids must be exactly the Layer
//! Effects controls bound to a `ui:` path. The host-bound `delay.tap_set` and `amp.filter` are
//! proven in `flows.rs` (`tap_tempo_sets_the_delay_time`,
//! `amp_filter_and_to_rotary_are_exclusive_cabinet_choices`).

use super::*;
use nord_core::engine::Stage4Engine;
use nord_core::fx::{AmpCabinet, Mod1Type, Mod2Type, ReverbModel};
use nord_panel::{all_controls, Binding, Section};

fn sync(s: &mut Session, e: &mut Stage4Engine) {
    for edit in s.fx_edits() {
        edit.apply(e);
    }
}

#[test]
fn ui_bound_effect_buttons_switch_their_unit_in_the_engine() {
    let mut s = Session::new();
    let mut e = Stage4Engine::new(48_000.0);
    s.fx_edits();

    s.click("mod1.on");
    sync(&mut s, &mut e);
    assert_ne!(e.fx_chains[0].mod1_type, Mod1Type::Off, "MOD 1 ON");
    s.click("mod1.on");
    sync(&mut s, &mut e);
    assert_eq!(e.fx_chains[0].mod1_type, Mod1Type::Off, "MOD 1 off");

    s.click("mod2.on");
    sync(&mut s, &mut e);
    assert_ne!(e.fx_chains[0].mod2_type, Mod2Type::Off, "MOD 2 ON");
    s.click("mod2.on");
    sync(&mut s, &mut e);
    assert_eq!(e.fx_chains[0].mod2_type, Mod2Type::Off, "MOD 2 off");

    s.state.fx_ui.chains[0].cabinet = AmpCabinet::Twin;
    s.click("amp.on");
    sync(&mut s, &mut e);
    assert_eq!(
        e.fx_chains[0].amp_cabinet,
        AmpCabinet::Twin,
        "AMP SIM/EQ ON"
    );
    s.click("amp.on");
    sync(&mut s, &mut e);
    assert_eq!(
        e.fx_chains[0].amp_cabinet,
        AmpCabinet::Off,
        "AMP SIM/EQ off"
    );

    // The reverb starts on (the engine default); the button turns it off and on again.
    s.click("reverb.on");
    sync(&mut s, &mut e);
    assert_eq!(e.fx_chains[0].reverb_model, ReverbModel::Off, "REVERB off");
    s.click("reverb.on");
    sync(&mut s, &mut e);
    assert_ne!(e.fx_chains[0].reverb_model, ReverbModel::Off, "REVERB ON");

    // LAYER EFFECTS ON gates every unit of the chain: the reverb goes silent, then returns.
    s.click("fx.layer_effects_on");
    sync(&mut s, &mut e);
    assert_eq!(
        e.fx_chains[0].reverb_model,
        ReverbModel::Off,
        "LAYER FX off"
    );
    s.click("fx.layer_effects_on");
    sync(&mut s, &mut e);
    assert_ne!(e.fx_chains[0].reverb_model, ReverbModel::Off, "LAYER FX ON");

    let mut bound: Vec<String> = all_controls()
        .into_iter()
        .filter(|c| {
            c.section == Section::Effects
                && matches!(c.binding, Binding::Bound(b) if b.starts_with("ui:"))
        })
        .map(|c| c.id)
        .collect();
    bound.sort_unstable();
    assert_eq!(
        bound,
        [
            "amp.on",
            "fx.layer_effects_on",
            "mod1.on",
            "mod2.on",
            "reverb.on"
        ],
        "every ui:fx.* control has a proof above"
    );
}

#[test]
fn fx_focus_buttons_only_change_which_chain_the_bay_addresses() {
    // The three UiOnly FX FOCUS buttons: no engine command, only the addressed slot changes.
    let mut s = Session::new();
    s.fx_edits();
    let before = s.state.fx_focus;
    for id in ["fx.focus.piano", "fx.focus.synth", "fx.focus.organ"] {
        s.click(id);
        assert!(s.fx_edits().is_empty(), "{id} sends no engine edit");
    }
    s.click("fx.focus.synth");
    assert_ne!(s.state.fx_focus, before, "FX FOCUS SYNTH moved the focus");
    assert!(s.fx_edits().is_empty());
}
