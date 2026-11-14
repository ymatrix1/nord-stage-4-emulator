//! Menu item writes (`ProgramEdit::Settings`) to host parameters.
//!
//! Every `Bound` item of the menus model (System, Sound, Output, Pedal, MIDI, Aux KB, Extern) that
//! has a host parameter is a row of [`menu_table::ROWS`]; this file only turns an edit into the
//! row's `ParamWrite`. The host write goes through `GuiParamWriter`, which wraps it in a
//! begin / set / end gesture, so the host records automation. The reverse direction (host value
//! to menu state) is `readback_menu`. Items without a host parameter are listed with their
//! reason in `menu_table::UI_ONLY`; they are dropped and counted as unmapped.

use super::menu_table::lookup;
use super::write::ParamWrite as W;
use nord_gui::audio::SettingsEdit;

/// Map one menu item write; returns `false` when the item has no host mapping.
pub(super) fn map_settings(e: &SettingsEdit, out: &mut Vec<W>) -> bool {
    let Some(r) = lookup(e.id) else {
        return false;
    };
    out.push(W {
        family: r.row.family,
        layer: r.row.layer,
        plain: r.to_plain(e.value),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aux_pedal_params::PedalBank;
    use crate::editor::index::ParamIndex;
    use crate::editor::menu_table::{resolved, Conv, Resolved, AUX_SYNTH_MASK};
    use crate::editor::menu_ui_only::{pattern_matches, ui_only_reason, UI_ONLY};
    use crate::params::Stage4Params;
    use crate::pedal_assign_params::PedalAssignParams;
    use nih_plug::prelude::{IntParam, IntRange};
    use nord_core::engine::Stage4Engine;
    use nord_panel::menus::{all_items, ItemKind, Value};
    use nord_panel::Binding;

    fn edit(id: &'static str, value: Value) -> SettingsEdit {
        SettingsEdit { id, value }
    }

    fn mapped(e: &SettingsEdit) -> Vec<W> {
        let mut out = Vec::new();
        assert!(map_settings(e, &mut out), "{} not mapped", e.id);
        out
    }

    /// Values worth trying for an item: every choice, both toggle states, the range ends and the
    /// default.
    fn probe_values(r: &Resolved) -> Vec<Value> {
        match r.item.kind {
            ItemKind::Toggle => vec![Value::Bool(false), Value::Bool(true)],
            ItemKind::Enum(c) => (0..c.len()).map(Value::Index).collect(),
            ItemKind::Int { min, max, .. } => {
                vec![Value::Int(min), Value::Int(max), r.item.default]
            }
            ItemKind::Float { min, max, .. } => {
                vec![Value::Float(min), Value::Float(max), r.item.default]
            }
            ItemKind::Action => vec![],
        }
    }

    #[test]
    fn every_bound_item_is_mapped_or_allow_listed() {
        let params = Stage4Params::default();
        let idx = ParamIndex::new(&params);
        let mut unmapped = Vec::new();
        for (_, item) in all_items() {
            if !matches!(item.binding, Binding::Bound(_)) {
                assert!(
                    crate::editor::menu_table::lookup(item.id).is_none(),
                    "{} is mapped but not Bound in the model",
                    item.id
                );
                continue;
            }
            let value = match item.kind {
                ItemKind::Action => Value::None,
                _ => item.default,
            };
            let mut out = Vec::new();
            let known = map_settings(&edit(item.id, value), &mut out);
            match (known, ui_only_reason(item.id)) {
                (true, None) => {
                    assert_eq!(out.len(), 1, "{}", item.id);
                    assert!(
                        idx.contains(&out[0].host_id()),
                        "{} writes unknown param {}",
                        item.id,
                        out[0].host_id()
                    );
                }
                (false, Some(why)) => assert!(!why.is_empty(), "{}", item.id),
                (true, Some(_)) => panic!("{} is mapped but still allow-listed", item.id),
                (false, None) => unmapped.push(item.id),
            }
        }
        assert!(
            unmapped.is_empty(),
            "Bound menu items with no editor mapping and no UI_ONLY reason: {unmapped:?}"
        );
    }

    #[test]
    fn allow_list_has_no_stale_patterns_and_names_only_bound_items() {
        let items = all_items();
        for (pattern, why) in UI_ONLY {
            assert!(!why.is_empty());
            let hits: Vec<_> = items
                .iter()
                .filter(|(_, i)| pattern_matches(pattern, i.id))
                .collect();
            assert!(!hits.is_empty(), "stale pattern {pattern}");
            for (_, i) in hits {
                assert!(
                    matches!(i.binding, Binding::Bound(_)),
                    "{} is not Bound; drop it from the allow-list",
                    i.id
                );
            }
        }
    }

    #[test]
    fn mapped_defaults_equal_the_host_parameter_defaults() {
        let params = Stage4Params::default();
        let idx = ParamIndex::new(&params);
        let mut bad = Vec::new();
        for r in resolved() {
            let host = idx.plain_of(r.row.family, r.row.layer).expect("param");
            // Known: the model shows the hardware default SP-1, the engine and host use SP-2
            // (continuous, half pedalling); `readback_menu` shows the host value on open.
            if r.row.id == "pedal.3.sustain_type" {
                assert_eq!((r.item.default, host), (Value::Index(0), 1.0));
                continue;
            }
            if !r.agrees(r.item.default, host) {
                bad.push((r.row.id, r.item.default, host));
            }
        }
        assert!(bad.is_empty(), "model default != host default: {bad:?}");
    }

    /// Host parameters are ranges; every menu value must fit (no silent clamping on the way in).
    #[test]
    fn host_ranges_hold_every_menu_value() {
        let params = Stage4Params::default();
        let idx = ParamIndex::new(&params);
        for r in resolved() {
            let ptr = idx.ptr(&r.row.host_id()).expect("param");
            for v in probe_values(r) {
                let plain = r.to_plain(v);
                // SAFETY: `params` outlives `ptr` within this test.
                let back = unsafe { ptr.preview_plain(ptr.preview_normalized(plain)) };
                let tol = (0.02_f32).max(0.02 * plain.abs());
                assert!(
                    (back - plain).abs() <= tol,
                    "{} = {v:?}: wrote {plain}, host holds {back}",
                    r.row.id
                );
            }
            if let (Conv::Direct, ItemKind::Enum(c)) = (r.row.conv, r.item.kind) {
                // SAFETY: as above.
                let steps = unsafe { ptr.step_count() };
                let host_len = steps.map(|n| n + 1);
                // Output choices end in the arrow / Auto entry only on the Global list.
                assert!(
                    host_len.is_some_and(|n| n >= c.len()),
                    "{}: host has {host_len:?} values, menu {}",
                    r.row.id,
                    c.len()
                );
            }
        }
    }

    fn triple_state(e: &Stage4Engine) -> String {
        format!("{:?}", e.pedals.triple)
    }

    /// Triple pedal Left / Middle lists differ from the host `FUNCS` order: decode through the
    /// host parameters and compare with what the standalone edit does.
    #[test]
    fn triple_pedal_choices_decode_to_the_engine_edit_function() {
        for (id, unit_left, unit_mid, tp1) in [
            ("pedal.3.left", true, false, true),
            ("pedal.3.middle", false, true, true),
            ("pedal.4.left", true, false, false),
            ("pedal.4.middle", false, true, false),
        ] {
            let r = crate::editor::menu_table::lookup(id).unwrap();
            let ItemKind::Enum(choices) = r.item.kind else {
                panic!()
            };
            for i in 0..choices.len() {
                let e = edit(id, Value::Index(i));
                let plain = mapped(&e)[0].plain as i32;
                let int = || IntParam::new("t", plain, IntRange::Linear { min: 0, max: 12 });
                let mut p = PedalAssignParams::default();
                match (tp1, unit_left, unit_mid) {
                    (true, true, _) => p.tp1_left = int(),
                    (true, _, true) => p.tp1_mid = int(),
                    (false, true, _) => p.tp2_left = int(),
                    _ => p.tp2_mid = int(),
                }
                let mut host = Stage4Engine::new(48_000.0);
                p.sync_to_engine(&mut host);
                let mut standalone = Stage4Engine::new(48_000.0);
                assert!(e.apply(&mut standalone));
                assert_eq!(triple_state(&host), triple_state(&standalone), "{id} #{i}");
            }
        }
    }

    #[test]
    fn foot_switch_a_choices_decode_to_the_engine_edit_function() {
        for i in 0..10 {
            let e = edit("pedal.6.fs_a", Value::Index(i));
            let plain = mapped(&e)[0].plain as i32;
            let mut p = PedalBank::default();
            p.foot_func = IntParam::new("f", plain, IntRange::Linear { min: 0, max: 4 });
            let mut host = Stage4Engine::new(48_000.0);
            p.sync_to_engine(&mut host);
            let mut standalone = Stage4Engine::new(48_000.0);
            assert!(e.apply(&mut standalone));
            assert_eq!(
                format!("{:?}", host.pedals.foot_switch),
                format!("{:?}", standalone.pedals.foot_switch),
                "SW A choice {i}"
            );
        }
    }

    #[test]
    fn aux_synth_mask_matches_the_engine_edit() {
        for (choice, &mask) in AUX_SYNTH_MASK.iter().enumerate() {
            let e = edit("aux_kb.2.synth", Value::Index(choice));
            let mut engine = Stage4Engine::new(48_000.0);
            assert!(e.apply(&mut engine));
            let l = engine.aux_kb.layers;
            let engine_mask = u8::from(l[4]) | u8::from(l[5]) << 1 | u8::from(l[6]) << 2;
            assert_eq!(
                mapped(&e),
                vec![W::global("auxkb_synth", f32::from(engine_mask))]
            );
            assert_eq!(mask, engine_mask);
        }
    }

    #[test]
    fn layer_indexed_rows_write_the_layer_suffixed_id() {
        let w = mapped(&edit("extern.9.aftertouch_c", Value::Bool(false)));
        assert_eq!(w, vec![W::layer("midix_aft", 3, 0.0)]);
        let w = mapped(&edit("output.5.synth_b_bus", Value::Index(2)));
        assert_eq!(w, vec![W::layer("outx_bus", 6, 2.0)]);
    }

    #[test]
    fn items_without_a_host_parameter_are_not_mapped() {
        let mut out = Vec::new();
        for id in [
            "midi.1.local_control",
            "midi.3.organ_a",
            "extern.4.bank_lsb_b",
        ] {
            assert!(!map_settings(&edit(id, Value::Int(3)), &mut out), "{id}");
        }
        assert!(out.is_empty());
    }
}
