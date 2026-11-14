//! MIDI, Extern, Output, Sound page 4 and Foot Switch SW B items that map onto the engine's menu
//! state (manual OS v1.6x p.58-64): `midi_menu`, `extern_menu`, `output_routing`,
//! `organ_trigger` and `pedals.foot_switch_b`.
//!
//! The choice indices are the model's (`nord_panel::menus`); the engine enums use the same order,
//! so most items are a direct index write. Each mapping is pinned by a test below.

use super::channels::extern_target;
use super::SettingsEdit;
use nord_core::engine::{
    foot_switch_b_func, AuxVelocityCurve, MidiMode, RoutingMode, Stage4Engine, TransposeAt,
    TriggerPoint,
};

/// Output source index (Organ, Rotary, Piano A, Piano B, Synth A, Synth B, Synth C) and whether
/// the item is the Global output (`false`: the Per Program bus) of the `output.3`..`output.5`
/// ids.
fn output_source(id: &str) -> Option<(usize, bool)> {
    Some(match id {
        "output.3.organ_out" => (0, true),
        "output.3.organ_bus" => (0, false),
        "output.3.rotary_out" => (1, true),
        "output.3.rotary_bus" => (1, false),
        "output.4.piano_a_out" => (2, true),
        "output.4.piano_a_bus" => (2, false),
        "output.4.piano_b_out" => (3, true),
        "output.4.piano_b_bus" => (3, false),
        "output.5.synth_a_out" => (4, true),
        "output.5.synth_a_bus" => (4, false),
        "output.5.synth_b_out" => (5, true),
        "output.5.synth_b_bus" => (5, false),
        "output.5.synth_c_out" => (6, true),
        "output.5.synth_c_bus" => (6, false),
        _ => return None,
    })
}

/// Handles the ids listed in the module docs. `false` when `e` is not one of them.
pub(super) fn apply(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    let idx = e.index();
    if let Some((source, global)) = output_source(e.id) {
        let r = &mut engine.output_routing;
        if global {
            r.global_out[source] = idx.min(7) as u8;
        } else {
            r.bus[source] = idx.min(4) as u8;
        }
        return true;
    }
    if e.id.starts_with("extern.") {
        return extern_item(e, engine);
    }
    let m = &mut engine.midi_menu;
    match e.id {
        "midi.1.local_control" => m.local_control = e.on(),
        "midi.3.organ_sect" => m.sect_on[0] = e.on(),
        "midi.4.piano_sect" => m.sect_on[1] = e.on(),
        "midi.5.synth_sect" => m.sect_on[2] = e.on(),
        "midi.7.type" => m.nrpn = idx == 1,
        "midi.7.ctrl" => m.ctrl = MidiMode::from_index(idx),
        "midi.7.dev" => m.dev = MidiMode::from_index(idx),
        "midi.8.pc" => m.pc = MidiMode::from_index(idx),
        "midi.8.pc_ext" => m.pc_ext = MidiMode::from_index(idx),
        "midi.9.transpose_at" => {
            m.transpose_at = if idx == 1 {
                TransposeAt::Out
            } else {
                TransposeAt::In
            }
        }
        "output.1.routing_mode" => {
            engine.output_routing.mode = if idx == 1 {
                RoutingMode::PerProgram
            } else {
                RoutingMode::Global
            }
        }
        "output.2.bus_main" => engine.output_routing.bus_outputs[0] = idx.min(6) as u8,
        "output.2.bus_sub1" => engine.output_routing.bus_outputs[1] = idx.min(6) as u8,
        "output.2.bus_sub2" => engine.output_routing.bus_outputs[2] = idx.min(6) as u8,
        "output.2.bus_sub3" => engine.output_routing.bus_outputs[3] = idx.min(6) as u8,
        "sound.4.organ_trigger_point" => {
            engine.organ_trigger.point = if idx == 1 {
                TriggerPoint::Low
            } else {
                TriggerPoint::High
            }
        }
        "pedal.6.fs_b" => engine.pedals.foot_switch_b.func = foot_switch_b_func(idx),
        _ => return false,
    }
    true
}

/// `extern.1` (Echo), `extern.2` (Keyboard Velocity) and `extern.8`..`extern.12` (Device MIDI).
fn extern_item(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    if e.id == "extern.1.echo_global" {
        engine.extern_menu.echo_global = e.on();
        return true;
    }
    let Some((i, head)) = extern_target(e.id) else {
        return false;
    };
    let on = e.on();
    let d = &mut engine.extern_menu.devices[i];
    match head {
        "extern.2.velocity" => {
            engine.extern_menu.velocity[i] = match e.index() {
                0 => AuxVelocityCurve::Soft,
                2.. => AuxVelocityCurve::Hard,
                _ => AuxVelocityCurve::Medium,
            }
        }
        "extern.8.mod_wheel" => d.mod_wheel = on,
        "extern.9.aftertouch" => d.aftertouch = on,
        "extern.10.control_pedal" => d.control_pedal = on,
        "extern.11.swell_pedal" => d.swell_pedal = on,
        "extern.12.volume_pedal" => d.volume_pedal = on,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::engine::{OutChoice, TripleFunc};
    use nord_panel::menus::{find_item, ItemKind, Value};

    fn run(id: &'static str, value: Value, e: &mut Stage4Engine) {
        assert!(SettingsEdit { id, value }.apply(e), "{id} not handled");
    }

    fn labels(id: &str) -> &'static [&'static str] {
        let ItemKind::Enum(c) = find_item(id).unwrap().kind else {
            panic!("{id} is not a choice");
        };
        c
    }

    #[test]
    fn midi_menu_items_write_the_settings() {
        let mut e = Stage4Engine::new(48_000.0);
        run("midi.1.local_control", Value::Bool(false), &mut e);
        run("midi.3.organ_sect", Value::Bool(true), &mut e);
        run("midi.5.synth_sect", Value::Bool(true), &mut e);
        run("midi.7.type", Value::Index(0), &mut e);
        run("midi.7.ctrl", Value::Index(2), &mut e);
        run("midi.7.dev", Value::Index(1), &mut e);
        run("midi.8.pc", Value::Index(0), &mut e);
        run("midi.8.pc_ext", Value::Index(3), &mut e);
        run("midi.9.transpose_at", Value::Index(1), &mut e);
        let m = e.midi_menu;
        assert!(!m.local_control && !m.nrpn);
        assert_eq!(m.sect_on, [true, false, true]);
        assert_eq!(
            (m.ctrl, m.dev, m.pc, m.pc_ext),
            (
                MidiMode::Receive,
                MidiMode::Send,
                MidiMode::Off,
                MidiMode::SendReceive
            )
        );
        assert_eq!(m.transpose_at, TransposeAt::Out);
        // The model's choice lists match the engine order.
        assert_eq!(
            labels("midi.7.ctrl"),
            ["Off", "Send", "Receive", "Send & Receive"]
        );
        assert_eq!(labels("midi.9.transpose_at"), ["In", "Out"]);
        assert_eq!(labels("midi.7.type"), ["CC", "CC & NRPN"]);
    }

    #[test]
    fn extern_menu_items_write_their_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        run("extern.1.echo_global", Value::Bool(false), &mut e);
        run("extern.2.velocity_a", Value::Index(0), &mut e);
        run("extern.2.velocity_c", Value::Index(2), &mut e);
        run("extern.8.mod_wheel_b", Value::Bool(false), &mut e);
        run("extern.9.aftertouch_a", Value::Bool(false), &mut e);
        run("extern.10.control_pedal_c", Value::Bool(false), &mut e);
        run("extern.11.swell_pedal_a", Value::Bool(false), &mut e);
        run("extern.12.volume_pedal_b", Value::Bool(false), &mut e);
        let x = e.extern_menu;
        assert!(!x.echo_global);
        assert_eq!(
            x.velocity,
            [
                AuxVelocityCurve::Soft,
                AuxVelocityCurve::Medium,
                AuxVelocityCurve::Hard
            ]
        );
        assert!(!x.devices[1].mod_wheel && x.devices[0].mod_wheel);
        assert!(!x.devices[0].aftertouch && !x.devices[2].control_pedal);
        assert!(!x.devices[0].swell_pedal && !x.devices[1].volume_pedal);
        assert_eq!(labels("extern.2.velocity_a"), ["Soft", "Mid", "Hard"]);
    }

    #[test]
    fn output_items_write_the_routing_and_resolve_as_the_menu_says() {
        let mut e = Stage4Engine::new(48_000.0);
        run("output.1.routing_mode", Value::Index(1), &mut e);
        run("output.2.bus_sub1", Value::Index(6), &mut e);
        run("output.4.piano_a_bus", Value::Index(1), &mut e);
        run("output.5.synth_a_out", Value::Index(3), &mut e);
        run("output.3.rotary_out", Value::Index(7), &mut e);
        let r = e.output_routing;
        assert_eq!(r.mode, RoutingMode::PerProgram);
        assert_eq!(
            r.destination(2, 0),
            OutChoice::Both,
            "Piano A -> Sub I -> 12+34"
        );
        assert_eq!(r.destination(3, 0), OutChoice::Both, "Piano B arrow");
        run("output.1.routing_mode", Value::Index(0), &mut e);
        assert_eq!(e.output_routing.destination(4, 0), OutChoice::Out2);
        assert_eq!(labels("output.3.organ_out")[3], "2");
        assert_eq!(labels("output.2.bus_sub1")[6], "12+34");
        assert_eq!(labels("output.5.synth_b_out")[7], "<-");
        assert_eq!(labels("output.5.synth_b_bus")[4], "<-");
    }

    #[test]
    fn trigger_point_and_switch_b_items_reach_the_engine() {
        let mut e = Stage4Engine::new(48_000.0);
        run("sound.4.organ_trigger_point", Value::Index(1), &mut e);
        assert_eq!(e.organ_trigger.point, TriggerPoint::Low);
        run("sound.4.organ_trigger_point", Value::Index(0), &mut e);
        assert_eq!(e.organ_trigger.point, TriggerPoint::High);
        run("pedal.6.fs_b", Value::Index(8), &mut e);
        assert_eq!(e.pedals.foot_switch_b.func, TripleFunc::RotorHold);
        assert_eq!(labels("pedal.6.fs_b").len(), 10);
        assert_eq!(labels("sound.4.organ_trigger_point"), ["High", "Low"]);
    }
}
