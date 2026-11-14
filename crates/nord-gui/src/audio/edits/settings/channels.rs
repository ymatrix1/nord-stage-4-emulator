//! MIDI channel and Extern menu items (manual OS v1.6x p.61-64).
//!
//! Channels are `0 = Off`, `1..=16` as printed; the engine stores them zero-based.

use super::SettingsEdit;
use nord_core::engine::Stage4Engine;
use nord_core::morph::LayerId;

fn channel(v: i32) -> Option<u8> {
    u8::try_from(v.clamp(0, 16)).ok().filter(|&c| c > 0)
}

fn layer_of(id: &str) -> Option<LayerId> {
    Some(match id {
        "midi.3.organ_a" => LayerId::OrganA,
        "midi.3.organ_b" => LayerId::OrganB,
        "midi.4.piano_a" => LayerId::PianoA,
        "midi.4.piano_b" => LayerId::PianoB,
        "midi.5.synth_a" => LayerId::SynthA,
        "midi.5.synth_b" => LayerId::SynthB,
        "midi.5.synth_c" => LayerId::SynthC,
        _ => return None,
    })
}

/// Extern layer `0..3` and the id without its `_a|_b|_c` suffix.
pub(super) fn extern_target(id: &str) -> Option<(usize, &str)> {
    let (head, layer) = id.rsplit_once('_')?;
    let i = match layer {
        "a" => 0,
        "b" => 1,
        "c" => 2,
        _ => return None,
    };
    Some((i, head))
}

/// Handles the `midi.*` and `extern.*` ids. `false` when `e` is not one the engine backs.
pub(super) fn apply(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    let v = e.int();
    if let Some(l) = layer_of(e.id) {
        engine.set_layer_channel(l, channel(v));
        return true;
    }
    match e.id {
        "midi.2.global_channel" => engine.set_global_channel(channel(v)),
        "midi.2.aux_channel" => engine.set_aux_channel(channel(v)),
        id => {
            let Some((i, head)) = extern_target(id) else {
                return false;
            };
            let layer = &mut engine.extern_out.layers[i];
            let byte = v.clamp(0, 127) as u8;
            match head {
                "midi.6.extern" => layer.channel = channel(v).map(|c| c - 1),
                // "--" (-1) has no engine state, so it stores 0.
                "extern.3.bank_msb" => layer.bank_msb = byte,
                "extern.4.bank_lsb" => layer.bank_lsb = byte,
                "extern.5.program" => layer.program = byte,
                "extern.6.cc1" => layer.cc_numbers[0] = byte.min(119),
                "extern.7.cc2" => layer.cc_numbers[1] = byte.min(119),
                "extern.13.send_on_load" => layer.send_on_load = e.on(),
                _ => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::menus::Value;

    fn run(id: &'static str, value: Value, e: &mut Stage4Engine) {
        assert!(SettingsEdit { id, value }.apply(e), "{id} not handled");
    }

    #[test]
    fn midi_channels_are_zero_off_and_stored_zero_based() {
        let mut e = Stage4Engine::new(48_000.0);
        run("midi.2.global_channel", Value::Int(5), &mut e);
        run("midi.2.aux_channel", Value::Int(0), &mut e);
        run("midi.3.organ_a", Value::Int(16), &mut e);
        run("midi.5.synth_c", Value::Int(3), &mut e);
        assert_eq!(e.global_midi_channel, Some(4));
        assert_eq!(e.aux_kb.channel, None);
        assert_eq!(e.layer_midi_channels[0], Some(15));
        assert_eq!(e.layer_midi_channels[6], Some(2));
        run("midi.3.organ_a", Value::Int(0), &mut e);
        assert_eq!(e.layer_midi_channels[0], None);
    }

    #[test]
    fn extern_items_write_their_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        run("midi.6.extern_b", Value::Int(2), &mut e);
        run("extern.3.bank_msb_a", Value::Int(-1), &mut e);
        run("extern.4.bank_lsb_a", Value::Int(7), &mut e);
        run("extern.5.program_c", Value::Int(99), &mut e);
        run("extern.6.cc1_b", Value::Int(120), &mut e);
        run("extern.7.cc2_a", Value::Int(64), &mut e);
        run("extern.13.send_on_load_c", Value::Bool(true), &mut e);
        let l = &e.extern_out.layers;
        assert_eq!(l[1].channel, Some(1));
        assert_eq!((l[0].bank_msb, l[0].bank_lsb), (0, 7));
        assert_eq!(l[2].program, 99);
        assert_eq!(l[1].cc_numbers[0], 119, "CC numbers stop at 119");
        assert_eq!(l[0].cc_numbers[1], 64);
        assert!(l[2].send_on_load && !l[0].send_on_load);
    }
}
