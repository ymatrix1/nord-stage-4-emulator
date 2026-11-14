//! Pedal menu items (manual OS v1.6x p.60-61): `pedals` and `pedal_assign` state.

use super::SettingsEdit;
use nord_core::engine::{
    ControlFunc, FootSwitchFunc, RightPedal, RotorFunc, Stage4Engine, SustainPedalType, SwellFunc,
    TripleFunc, TriplePedal, TripleUnit,
};

/// Function order of the Left / Middle choice lists after their first entry (Una Corda /
/// Sostenuto): Panel Assigned .. Control. VERIFIED against `menus/pedal.rs` (`LEFT`, `MIDDLE`),
/// checked by `triple_pedal_choices_cover_the_whole_list`.
const TRIPLE_TAIL: [TripleFunc; 11] = [
    TripleFunc::PanelAssigned,
    TripleFunc::LayerSceneHold,
    TripleFunc::LayerSceneToggle,
    TripleFunc::Pump,
    TripleFunc::MstClkTap,
    TripleFunc::SynthVibrato,
    TripleFunc::ProgramUp,
    TripleFunc::ProgramDown,
    TripleFunc::RotorHold,
    TripleFunc::RotorToggle,
    TripleFunc::Control,
];

fn triple_func(first: TripleFunc, idx: usize) -> TripleFunc {
    match idx.checked_sub(1) {
        None => first,
        Some(i) => TRIPLE_TAIL[i.min(TRIPLE_TAIL.len() - 1)],
    }
}

/// Foot switch SW A choice (10 entries, `menus/pedal.rs` `FS_FUNC`) to the engine function; the
/// choices the engine cannot act on map to Off (the model documents that gap).
fn foot_switch(idx: usize) -> FootSwitchFunc {
    match idx {
        1 => FootSwitchFunc::LayerSceneHold,
        2 => FootSwitchFunc::LayerSceneToggle,
        8 => FootSwitchFunc::RotorHold,
        9 => FootSwitchFunc::RotorToggle,
        _ => FootSwitchFunc::Off,
    }
}

/// Handles the `pedal.*` ids. `false` when `e` is not one the engine backs.
pub(super) fn apply(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    let p = &mut engine.pedals;
    let idx = e.index();
    match e.id {
        "pedal.1.ctrl_func" => {
            p.control_func = if idx == 1 {
                ControlFunc::CtrlSwell
            } else {
                ControlFunc::Control
            }
        }
        "pedal.1.ctrl_gain" => p.control_gain = e.float().clamp(1.0, 10.0),
        "pedal.2.swell_func" => {
            p.swell_func = match idx {
                1 => SwellFunc::Volume,
                2.. => SwellFunc::WheelMorph,
                _ => SwellFunc::Swell,
            }
        }
        "pedal.2.swell_gain" => p.swell_gain = e.float().clamp(1.0, 10.0),
        "pedal.3.right" | "pedal.4.right" => {
            p.right_pedal = match idx {
                1 => RightPedal::SusRotorHold,
                2.. => RightPedal::SusRotorToggle,
                _ => RightPedal::Sustain,
            }
        }
        "pedal.5.rotor_func" => {
            p.rotor_func = if idx == 1 {
                RotorFunc::Hold
            } else {
                RotorFunc::Toggle
            }
        }
        "pedal.6.fs_a" => p.foot_switch = foot_switch(idx),
        "pedal.3.sustain_type" => {
            p.triple.sustain_type = match idx {
                0 => SustainPedalType::Sp1,
                2 => SustainPedalType::Tp1,
                3 => SustainPedalType::NormallyOpen,
                4.. => SustainPedalType::NormallyClosed,
                _ => SustainPedalType::Sp2,
            }
        }
        "pedal.3.left" => p.triple.set_func(
            TripleUnit::Tp1,
            TriplePedal::Left,
            triple_func(TripleFunc::UnaCorda, idx),
        ),
        "pedal.3.middle" => p.triple.set_func(
            TripleUnit::Tp1,
            TriplePedal::Middle,
            triple_func(TripleFunc::Sostenuto, idx),
        ),
        "pedal.4.left" => p.triple.set_func(
            TripleUnit::Tp2,
            TriplePedal::Left,
            triple_func(TripleFunc::UnaCorda, idx),
        ),
        "pedal.4.middle" => p.triple.set_func(
            TripleUnit::Tp2,
            TriplePedal::Middle,
            triple_func(TripleFunc::Sostenuto, idx),
        ),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::menus::{find_item, ItemKind, Value};

    fn run(id: &'static str, value: Value, e: &mut Stage4Engine) {
        assert!(SettingsEdit { id, value }.apply(e), "{id} not handled");
    }

    #[test]
    fn pedal_items_write_the_pedal_state() {
        let mut e = Stage4Engine::new(48_000.0);
        run("pedal.1.ctrl_func", Value::Index(1), &mut e);
        run("pedal.1.ctrl_gain", Value::Float(99.0), &mut e);
        run("pedal.2.swell_func", Value::Index(2), &mut e);
        run("pedal.3.right", Value::Index(2), &mut e);
        run("pedal.5.rotor_func", Value::Index(1), &mut e);
        run("pedal.6.fs_a", Value::Index(9), &mut e);
        let p = &e.pedals;
        assert_eq!(p.control_func, ControlFunc::CtrlSwell);
        assert_eq!(p.control_gain, 10.0, "gain clamps to 1..10");
        assert_eq!(p.swell_func, SwellFunc::WheelMorph);
        assert_eq!(p.right_pedal, RightPedal::SusRotorToggle);
        assert_eq!(p.rotor_func, RotorFunc::Hold);
        assert_eq!(p.foot_switch, FootSwitchFunc::RotorToggle);
        run("pedal.6.fs_a", Value::Index(5), &mut e);
        assert_eq!(
            e.pedals.foot_switch,
            FootSwitchFunc::Off,
            "unsupported choice"
        );
    }

    #[test]
    fn triple_pedal_choices_cover_the_whole_list() {
        let mut e = Stage4Engine::new(48_000.0);
        let ItemKind::Enum(left) = find_item("pedal.4.left").unwrap().kind else {
            panic!("enum");
        };
        assert_eq!(left.len(), TRIPLE_TAIL.len() + 1);
        run("pedal.4.left", Value::Index(10), &mut e);
        run("pedal.4.middle", Value::Index(0), &mut e);
        run("pedal.3.left", Value::Index(11), &mut e);
        let t = &e.pedals.triple;
        assert_eq!(
            t.func(TripleUnit::Tp2, TriplePedal::Left),
            TripleFunc::RotorToggle
        );
        assert_eq!(
            t.func(TripleUnit::Tp2, TriplePedal::Middle),
            TripleFunc::Sostenuto
        );
        // Control is TP-2 only: the engine ignores it on the TP-1.
        assert_eq!(
            t.func(TripleUnit::Tp1, TriplePedal::Left),
            TripleFunc::UnaCorda
        );
        run("pedal.3.sustain_type", Value::Index(4), &mut e);
        assert_eq!(
            e.pedals.triple.sustain_type,
            SustainPedalType::NormallyClosed
        );
    }
}
