//! Menu item to host parameter table: the single source for both directions of the menu mapping.
//!
//! `map_settings` (GUI edit to host write) and `readback_menu` (host value to GUI menu state) both
//! go through [`lookup`], so a menu item is mapped in one place. Every `Bound` item of
//! `nord_panel::menus` either has a row in [`ROWS`] or is listed in `menu_ui_only::UI_ONLY` with the reason it
//! has no host parameter; `tests::every_bound_item_is_mapped_or_allow_listed` enforces that.
//!
//! The menu's choice indices are labels, not host values. Where the label order differs from the
//! host parameter's coding, a [`Conv`] converts explicitly; every other row passes the index (or
//! the int / float / on-off value) through unchanged.

use nord_panel::menus::{all_items, ItemKind, MenuItem, Value};
use std::sync::OnceLock;

/// Aux KB synth choice index (`---`, `A--`, `-B-`, `--C`, `AB-`, `A-C`, `-BC`, `ABC`, manual p.58)
/// to the host layer bitmask (bit 0 = A, 1 = B, 2 = C).
///
/// VERIFIED: equals the table the engine edit uses (`audio/edits/settings/global.rs`), checked
/// by `map_settings::tests::aux_synth_mask_matches_the_engine_edit`.
pub const AUX_SYNTH_MASK: [u8; 8] = [0, 0b001, 0b010, 0b100, 0b011, 0b101, 0b110, 0b111];

/// How a menu value converts to the host parameter's plain value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conv {
    /// Same number on both sides (index, int, float, on/off as 0/1).
    Direct,
    /// Aux KB synth choice to layer bitmask ([`AUX_SYNTH_MASK`]).
    AuxSynth,
    /// Triple pedal Left list (Una Corda, then Panel Assigned ..) to the host `FUNCS` index.
    TripleLeft,
    /// Triple pedal Middle list (Sostenuto, then User Assigned ..) to the host `FUNCS` index.
    TripleMid,
    /// Foot Switch SW A (ten choices) to the host subset the engine acts on (five).
    FootA,
}

/// One row: menu item id, host family (`layer` appends `_{n}`) and conversion.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub id: &'static str,
    pub family: &'static str,
    pub layer: Option<u8>,
    pub conv: Conv,
}

impl Row {
    /// Full host parameter id (`midix_vel_2` for family `midix_vel`, layer 2).
    pub fn host_id(&self) -> String {
        match self.layer {
            Some(n) => format!("{}_{n}", self.family),
            None => self.family.to_string(),
        }
    }
}

const fn d(id: &'static str, family: &'static str) -> Row {
    Row {
        id,
        family,
        layer: None,
        conv: Conv::Direct,
    }
}

const fn l(id: &'static str, family: &'static str, n: u8) -> Row {
    Row {
        id,
        family,
        layer: Some(n),
        conv: Conv::Direct,
    }
}

const fn c(id: &'static str, family: &'static str, conv: Conv) -> Row {
    Row {
        id,
        family,
        layer: None,
        conv,
    }
}

/// Every mapped menu item (System, Sound, Output, Pedal, MIDI, Aux KB, Extern).
pub const ROWS: &[Row] = &[
    d("system.2.transpose", "sys_transpose"),
    d("system.3.fine_tune", "sys_fine_tune"),
    d("sound.1.program_levels", "prog_level_db"),
    d("sound.2.piano_pedal_noise", "sys_piano_noise_db"),
    d("sound.3.piano_string_res", "sys_piano_res_db"),
    d("sound.4.organ_trigger_point", "menx_trigger"),
    d("sound.5.b3_tonewheel_mode", "sys_tonewheel"),
    d("sound.6.b3_click_level", "sys_click_high"),
    d("sound.7.rotary_bass_horn", "sys_rotary_balance"),
    d("sound.8.rotary_rotor_speed", "sys_rotor_speed"),
    d("sound.9.rotary_rotor_acc", "sys_rotor_acc"),
    d("sound.10.rotary_horn_speed", "sys_horn_speed"),
    d("sound.11.rotary_horn_acc", "sys_horn_acc"),
    d("output.1.routing_mode", "outx_mode"),
    l("output.2.bus_main", "outx_busout", 1),
    l("output.2.bus_sub1", "outx_busout", 2),
    l("output.2.bus_sub2", "outx_busout", 3),
    l("output.2.bus_sub3", "outx_busout", 4),
    d("output.2.mono", "sys_mono"),
    l("output.3.organ_out", "outx_global", 1),
    l("output.3.organ_bus", "outx_bus", 1),
    l("output.3.rotary_out", "outx_global", 2),
    l("output.3.rotary_bus", "outx_bus", 2),
    l("output.4.piano_a_out", "outx_global", 3),
    l("output.4.piano_a_bus", "outx_bus", 3),
    l("output.4.piano_b_out", "outx_global", 4),
    l("output.4.piano_b_bus", "outx_bus", 4),
    l("output.5.synth_a_out", "outx_global", 5),
    l("output.5.synth_a_bus", "outx_bus", 5),
    l("output.5.synth_b_out", "outx_global", 6),
    l("output.5.synth_b_bus", "outx_bus", 6),
    l("output.5.synth_c_out", "outx_global", 7),
    l("output.5.synth_c_bus", "outx_bus", 7),
    d("pedal.1.ctrl_func", "ped_ctrl_func"),
    d("pedal.1.ctrl_gain", "ped_ctrl_gain"),
    d("pedal.2.swell_func", "ped_swell_func"),
    d("pedal.2.swell_gain", "ped_swell_gain"),
    d("pedal.3.sustain_type", "pedx_sustain_type"),
    c("pedal.3.left", "pedx_tp1_left", Conv::TripleLeft),
    c("pedal.3.middle", "pedx_tp1_mid", Conv::TripleMid),
    d("pedal.3.right", "ped_right"),
    c("pedal.4.left", "pedx_tp2_left", Conv::TripleLeft),
    c("pedal.4.middle", "pedx_tp2_mid", Conv::TripleMid),
    d("pedal.4.right", "ped_right"),
    d("pedal.5.rotor_func", "ped_rotor_func"),
    c("pedal.6.fs_a", "ped_foot_func", Conv::FootA),
    d("pedal.6.fs_b", "menx_fsb_func"),
    d("midi.2.aux_channel", "auxkb_channel"),
    d("midi.3.organ_sect", "midix_sect_organ"),
    d("midi.4.piano_sect", "midix_sect_piano"),
    d("midi.5.synth_sect", "midix_sect_synth"),
    d("midi.7.type", "midix_nrpn"),
    d("midi.7.ctrl", "midix_ctrl"),
    d("midi.7.dev", "midix_dev"),
    d("midi.8.pc", "midix_pc"),
    d("midi.8.pc_ext", "midix_pc_ext"),
    d("midi.9.transpose_at", "midix_transpose_at"),
    d("aux_kb.1.enable", "auxkb_enable"),
    d("aux_kb.2.organ", "auxkb_organ"),
    d("aux_kb.2.piano", "auxkb_piano"),
    c("aux_kb.2.synth", "auxkb_synth", Conv::AuxSynth),
    d("aux_kb.3.curve", "auxkb_curve"),
    d("extern.1.echo_global", "midix_echo"),
    l("extern.2.velocity_a", "midix_vel", 1),
    l("extern.2.velocity_b", "midix_vel", 2),
    l("extern.2.velocity_c", "midix_vel", 3),
    l("extern.8.mod_wheel_a", "midix_mod", 1),
    l("extern.8.mod_wheel_b", "midix_mod", 2),
    l("extern.8.mod_wheel_c", "midix_mod", 3),
    l("extern.9.aftertouch_a", "midix_aft", 1),
    l("extern.9.aftertouch_b", "midix_aft", 2),
    l("extern.9.aftertouch_c", "midix_aft", 3),
    l("extern.10.control_pedal_a", "midix_ctl", 1),
    l("extern.10.control_pedal_b", "midix_ctl", 2),
    l("extern.10.control_pedal_c", "midix_ctl", 3),
    l("extern.11.swell_pedal_a", "midix_swl", 1),
    l("extern.11.swell_pedal_b", "midix_swl", 2),
    l("extern.11.swell_pedal_c", "midix_swl", 3),
    l("extern.12.volume_pedal_a", "midix_vol", 1),
    l("extern.12.volume_pedal_b", "midix_vol", 2),
    l("extern.12.volume_pedal_c", "midix_vol", 3),
];

/// A table row with its menu item resolved once (the model builds its pages on every lookup).
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    pub row: Row,
    pub item: MenuItem,
}

/// Every row with its model item; rows whose id is not in the model are dropped (a test fails).
pub fn resolved() -> &'static [Resolved] {
    static CELL: OnceLock<Vec<Resolved>> = OnceLock::new();
    CELL.get_or_init(|| {
        let items = all_items();
        ROWS.iter()
            .filter_map(|row| {
                items
                    .iter()
                    .find(|(_, i)| i.id == row.id)
                    .map(|(_, item)| Resolved {
                        row: *row,
                        item: *item,
                    })
            })
            .collect()
    })
}

/// The row of menu item `id`.
pub fn lookup(id: &str) -> Option<&'static Resolved> {
    resolved().iter().find(|r| r.row.id == id)
}

fn number(v: Value) -> f32 {
    match v {
        Value::Bool(b) => f32::from(u8::from(b)),
        Value::Index(i) => i as f32,
        Value::Int(i) => i as f32,
        Value::Float(f) if f.is_finite() => f,
        Value::Float(_) | Value::None => 0.0,
    }
}

impl Resolved {
    /// Host plain value for a menu value.
    pub fn to_plain(&self, value: Value) -> f32 {
        let n = number(value);
        let i = n.round().max(0.0) as usize;
        match self.row.conv {
            Conv::Direct => n,
            Conv::AuxSynth => f32::from(AUX_SYNTH_MASK[i.min(7)]),
            Conv::TripleLeft => {
                if i == 0 {
                    0.0
                } else {
                    (i + 1).min(12) as f32
                }
            }
            Conv::TripleMid => (i + 1).min(12) as f32,
            Conv::FootA => match i {
                1 => 1.0,
                2 => 2.0,
                8 => 3.0,
                9 => 4.0,
                _ => 0.0,
            },
        }
    }

    /// Menu value for a host plain value; `None` when the host value has no menu label (a
    /// triple pedal list does not offer the other list's first entry).
    pub fn from_plain(&self, plain: f32) -> Option<Value> {
        let n = plain.round().max(0.0) as usize;
        let index = match self.row.conv {
            Conv::Direct => return Some(self.direct(plain)),
            Conv::AuxSynth => AUX_SYNTH_MASK.iter().position(|&m| usize::from(m) == n)?,
            Conv::TripleLeft => match n {
                0 => 0,
                1 => return None,
                _ => n - 1,
            },
            Conv::TripleMid => n.checked_sub(1)?,
            Conv::FootA => match n {
                0..=2 => n,
                3 => 8,
                4 => 9,
                _ => return None,
            },
        };
        Some(Value::Index(index))
    }

    fn direct(&self, plain: f32) -> Value {
        match self.item.kind {
            ItemKind::Toggle => Value::Bool(plain >= 0.5),
            ItemKind::Enum(_) => Value::Index(plain.round().max(0.0) as usize),
            ItemKind::Int { .. } => Value::Int(plain.round() as i32),
            ItemKind::Float { .. } => Value::Float(plain),
            ItemKind::Action => Value::None,
        }
    }

    /// `true` when `value` and `plain` describe the same setting. A read-back then keeps the
    /// menu value instead of replacing a label the host cannot tell apart (an unsupported Foot
    /// Switch choice that the engine treats as Off, say).
    pub fn agrees(&self, value: Value, plain: f32) -> bool {
        (self.to_plain(value) - plain).abs() < 1e-3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_resolves_to_a_model_item_and_ids_are_unique() {
        assert_eq!(resolved().len(), ROWS.len(), "a row names an unknown item");
        let mut ids: Vec<_> = ROWS.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ROWS.len());
    }

    fn row(id: &str) -> &'static Resolved {
        lookup(id).unwrap_or_else(|| panic!("{id} unmapped"))
    }

    #[test]
    fn direct_rows_pass_values_through_by_kind() {
        let t = row("system.2.transpose");
        assert_eq!(t.to_plain(Value::Int(-4)), -4.0);
        assert_eq!(t.from_plain(-4.0), Some(Value::Int(-4)));
        let g = row("pedal.1.ctrl_gain");
        assert_eq!(g.from_plain(2.5), Some(Value::Float(2.5)));
        let m = row("output.2.mono");
        assert_eq!(m.to_plain(Value::Bool(true)), 1.0);
        assert_eq!(m.from_plain(1.0), Some(Value::Bool(true)));
        let s = row("sound.5.b3_tonewheel_mode");
        assert_eq!(s.from_plain(2.0), Some(Value::Index(2)));
    }

    #[test]
    fn converted_rows_round_trip_every_menu_choice() {
        for id in [
            "aux_kb.2.synth",
            "pedal.3.left",
            "pedal.3.middle",
            "pedal.4.left",
            "pedal.4.middle",
            "pedal.6.fs_a",
        ] {
            let r = row(id);
            let ItemKind::Enum(choices) = r.item.kind else {
                panic!("{id} is not a choice");
            };
            for i in 0..choices.len() {
                let plain = r.to_plain(Value::Index(i));
                // Foot Switch choices the engine cannot act on all collapse to Off (0).
                if id == "pedal.6.fs_a" && plain == 0.0 {
                    assert_eq!(r.from_plain(0.0), Some(Value::Index(0)));
                    continue;
                }
                assert_eq!(r.from_plain(plain), Some(Value::Index(i)), "{id} #{i}");
            }
        }
    }

    #[test]
    fn host_values_without_a_menu_label_read_back_as_none() {
        assert_eq!(row("pedal.3.left").from_plain(1.0), None, "Sostenuto");
        assert_eq!(row("pedal.3.middle").from_plain(0.0), None, "Una Corda");
        assert_eq!(row("pedal.6.fs_a").from_plain(7.0), None);
    }

    #[test]
    fn agreement_keeps_labels_the_host_cannot_distinguish() {
        let fs = row("pedal.6.fs_a");
        // "Pump" (3) is not supported by the host: it reads as Off, and the menu keeps "Pump".
        assert!(fs.agrees(Value::Index(3), 0.0));
        assert!(!fs.agrees(Value::Index(8), 0.0));
    }
}
