//! Menu item writes (`ProgramEdit::Settings`) to host parameters.
//!
//! Only the Aux KB menu is mapped so far. The menu's choice indices are labels, not host
//! values: the Aux KB synth choice order (`---`, `A--`, `-B-`, `--C`, `AB-`, `A-C`, `-BC`,
//! `ABC`, manual p.58) differs from the plain layer bitmask the host parameter `auxkb_synth`
//! stores, so [`AUX_SYNTH_MASK`] converts explicitly. Organ and piano choices (`--`, `A-`, `-B`,
//! `AB`) are already bitmask-ordered. Other menu items (System, Sound, Output, Pedal, MIDI) have
//! host parameters but are not mapped yet; they are dropped and counted as unmapped.

use super::map::flag;
use super::write::ParamWrite as W;
use nord_gui::audio::SettingsEdit;
use nord_panel::menus::Value;

/// Aux KB synth choice index to the host layer bitmask (bit 0 = A, 1 = B, 2 = C).
///
/// VERIFIED: equals the table the engine edit uses (`audio/edits/settings/global.rs`), checked
/// by `aux_synth_mask_matches_the_engine_edit`.
pub const AUX_SYNTH_MASK: [u8; 8] = [0, 0b001, 0b010, 0b100, 0b011, 0b101, 0b110, 0b111];

fn index(v: Value) -> usize {
    match v {
        Value::Index(i) => i,
        Value::Bool(b) => usize::from(b),
        Value::Int(i) => usize::try_from(i).unwrap_or(0),
        _ => 0,
    }
}

/// Map one menu item write; returns `false` when the item has no host mapping yet.
pub(super) fn map_settings(e: &SettingsEdit, out: &mut Vec<W>) -> bool {
    let i = index(e.value);
    match e.id {
        "aux_kb.1.enable" => out.push(W::global("auxkb_enable", flag(i != 0))),
        "aux_kb.2.organ" => out.push(W::global("auxkb_organ", (i & 3) as f32)),
        "aux_kb.2.piano" => out.push(W::global("auxkb_piano", (i & 3) as f32)),
        "aux_kb.2.synth" => out.push(W::global(
            "auxkb_synth",
            f32::from(AUX_SYNTH_MASK[i.min(7)]),
        )),
        "aux_kb.3.curve" => out.push(W::global("auxkb_curve", i.min(2) as f32)),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::engine::Stage4Engine;

    fn edit(id: &'static str, value: Value) -> SettingsEdit {
        SettingsEdit { id, value }
    }

    #[test]
    fn aux_synth_mask_matches_the_engine_edit() {
        for choice in 0..8 {
            let e = edit("aux_kb.2.synth", Value::Index(choice));
            let mut engine = Stage4Engine::new(48_000.0);
            assert!(e.apply(&mut engine));
            let l = engine.aux_kb.layers;
            let engine_mask = u8::from(l[4]) | u8::from(l[5]) << 1 | u8::from(l[6]) << 2;
            let mut out = Vec::new();
            assert!(map_settings(&e, &mut out));
            assert_eq!(out, vec![W::global("auxkb_synth", f32::from(engine_mask))]);
        }
    }

    #[test]
    fn labels_that_differ_from_the_bitmask_convert_explicitly() {
        // `--C` is menu index 3 but bitmask 0b100; `AB-` is index 4 but 0b011.
        assert_eq!(AUX_SYNTH_MASK[3], 0b100);
        assert_eq!(AUX_SYNTH_MASK[4], 0b011);
        assert_eq!(AUX_SYNTH_MASK[5], 0b101);
        assert_eq!(AUX_SYNTH_MASK[6], 0b110);
    }

    #[test]
    fn organ_piano_curve_and_enable_map_directly() {
        let mut out = Vec::new();
        assert!(map_settings(
            &edit("aux_kb.2.organ", Value::Index(3)),
            &mut out
        ));
        assert!(map_settings(
            &edit("aux_kb.2.piano", Value::Index(2)),
            &mut out
        ));
        assert!(map_settings(
            &edit("aux_kb.3.curve", Value::Index(2)),
            &mut out
        ));
        assert!(map_settings(
            &edit("aux_kb.1.enable", Value::Bool(true)),
            &mut out
        ));
        assert_eq!(
            out,
            vec![
                W::global("auxkb_organ", 3.0),
                W::global("auxkb_piano", 2.0),
                W::global("auxkb_curve", 2.0),
                W::global("auxkb_enable", 1.0),
            ]
        );
    }

    #[test]
    fn other_menu_items_are_not_mapped_yet() {
        let mut out = Vec::new();
        assert!(!map_settings(
            &edit("system.2.transpose", Value::Int(3)),
            &mut out
        ));
        assert!(out.is_empty());
    }
}
