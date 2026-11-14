//! Read-back of the menu items: host parameter values to the panel's menu state, so the OLED menu
//! pages show host automation, presets and project loads (the other direction is `map_settings`).
//!
//! Driven by the same table as the write side (`menu_table`). A menu value is only replaced when
//! it no longer agrees with the host value, so a label the host cannot tell apart (a Foot Switch
//! choice the engine treats as Off) is not snapped back, and an edit made in the menu is not
//! overwritten before its write has landed.

use super::index::ParamIndex;
use super::menu_table::resolved;
use nord_gui::panel::ui_state::MenuUi;

/// Overwrite the values of `menu` that differ from their host parameters; returns how many.
pub fn sync_menu_from_params(idx: &ParamIndex, menu: &mut MenuUi) -> usize {
    sync_menu(&|family, layer| idx.plain_of(family, layer), menu)
}

/// Same, reading plain host values through `read(family, layer)` (tests pass a table).
fn sync_menu(read: &dyn Fn(&str, Option<u8>) -> Option<f32>, menu: &mut MenuUi) -> usize {
    let mut changed = 0;
    for r in resolved() {
        let Some(plain) = read(r.row.family, r.row.layer) else {
            continue;
        };
        if r.agrees(menu.value(&r.item), plain) {
            continue;
        }
        if let Some(v) = r.from_plain(plain) {
            menu.set(r.item.id, v);
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Stage4Params;
    use nord_panel::menus::{find_item, Value};
    use std::collections::HashMap;

    /// Host values: the defaults of `Stage4Params`, with `over` (full host id to plain) applied.
    fn host(over: &[(&str, f32)]) -> impl Fn(&str, Option<u8>) -> Option<f32> {
        let p = Stage4Params::default();
        let idx = ParamIndex::new(&p);
        let over: HashMap<String, f32> = over.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let mut table = HashMap::new();
        for r in resolved() {
            let id = r.row.host_id();
            let v = over.get(&id).copied().or_else(|| idx.plain(&id));
            table.insert(id, v);
        }
        move |family, layer| {
            let id = match layer {
                Some(n) => format!("{family}_{n}"),
                None => family.to_string(),
            };
            table.get(&id).copied().flatten()
        }
    }

    fn value(menu: &MenuUi, id: &str) -> Value {
        menu.value(&find_item(id).expect("item"))
    }

    /// A fresh menu differs from the host defaults in exactly one place: Sustain Pedal Type shows
    /// the hardware default (SP-1) while the engine and host default to SP-2 (continuous, so half
    /// pedalling works; see the model note). The read-back shows the real state.
    #[test]
    fn a_fresh_menu_differs_from_the_host_defaults_only_in_sustain_type() {
        let mut menu = MenuUi::default();
        assert_eq!(sync_menu(&host(&[]), &mut menu), 1);
        assert_eq!(value(&menu, "pedal.3.sustain_type"), Value::Index(1));
        assert_eq!(sync_menu(&host(&[]), &mut menu), 0);
    }

    #[test]
    fn the_param_index_reader_gives_the_same_result() {
        let p = Stage4Params::default();
        let idx = ParamIndex::new(&p);
        let mut menu = MenuUi::default();
        assert_eq!(sync_menu_from_params(&idx, &mut menu), 1);
        assert_eq!(sync_menu_from_params(&idx, &mut menu), 0);
    }

    #[test]
    fn host_automation_moves_the_menu_values() {
        let h = host(&[
            ("sys_transpose", -3.0),
            ("sys_tonewheel", 0.0),
            ("sys_mono", 1.0),
            ("auxkb_synth", 4.0), // layer C only: menu choice `--C` (3)
            ("outx_bus_6", 3.0),
            ("midix_vel_2", 2.0),
            ("pedx_tp2_mid", 6.0),
            ("ped_foot_func", 3.0), // Rotor Hold: menu choice 8
        ]);
        let mut menu = MenuUi::default();
        assert_eq!(sync_menu(&h, &mut menu), 9, "8 automated + sustain type");
        assert_eq!(value(&menu, "system.2.transpose"), Value::Int(-3));
        assert_eq!(value(&menu, "sound.5.b3_tonewheel_mode"), Value::Index(0));
        assert_eq!(value(&menu, "output.2.mono"), Value::Bool(true));
        assert_eq!(value(&menu, "aux_kb.2.synth"), Value::Index(3));
        assert_eq!(value(&menu, "output.5.synth_b_bus"), Value::Index(3));
        assert_eq!(value(&menu, "extern.2.velocity_b"), Value::Index(2));
        assert_eq!(value(&menu, "pedal.4.middle"), Value::Index(5));
        assert_eq!(value(&menu, "pedal.6.fs_a"), Value::Index(8));
        assert_eq!(sync_menu(&h, &mut menu), 0, "idempotent");
    }

    #[test]
    fn an_unsupported_foot_switch_choice_is_not_snapped_back() {
        let mut menu = MenuUi::default();
        sync_menu(&host(&[]), &mut menu);
        menu.set("pedal.6.fs_a", Value::Index(3)); // Pump: written to the host as Off
        assert_eq!(sync_menu(&host(&[]), &mut menu), 0);
        assert_eq!(value(&menu, "pedal.6.fs_a"), Value::Index(3));
    }

    #[test]
    fn a_pedal_shared_by_two_pages_keeps_both_pages_in_step() {
        let mut menu = MenuUi::default();
        sync_menu(&host(&[("ped_right", 2.0)]), &mut menu);
        assert_eq!(value(&menu, "pedal.3.right"), Value::Index(2));
        assert_eq!(value(&menu, "pedal.4.right"), Value::Index(2));
    }
}
