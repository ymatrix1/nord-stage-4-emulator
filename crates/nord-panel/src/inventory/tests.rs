//! Cross-section checks of the inventory (`docs/specs/control-inventory.md` §4).

use super::all_controls;
use super::synth_dials::DialTarget;
use crate::types::{Binding, ControlKind, Provenance, Section};
use std::collections::HashSet;
use std::path::Path;

/// Host parameter sources; a `Bound` id must appear as a quoted literal here.
const HOST_SOURCES: [&str; 16] = [
    include_str!("../../../nord-plugin/src/params.rs"),
    include_str!("../../../nord-plugin/src/fx_params.rs"),
    include_str!("../../../nord-plugin/src/fx_selectors.rs"),
    include_str!("../../../nord-plugin/src/morph_assign.rs"),
    include_str!("../../../nord-plugin/src/piano_params.rs"),
    include_str!("../../../nord-plugin/src/synth_params.rs"),
    include_str!("../../../nord-plugin/src/organ_params.rs"),
    include_str!("../../../nord-plugin/src/synth_mod_params.rs"),
    include_str!("../../../nord-plugin/src/fx_extra_params.rs"),
    include_str!("../../../nord-plugin/src/synth_gate_params.rs"),
    include_str!("../../../nord-plugin/src/fx_sharing_params.rs"),
    include_str!("../../../nord-plugin/src/aux_pedal_params.rs"),
    include_str!("../../../nord-plugin/src/program_params.rs"),
    include_str!("../../../nord-plugin/src/organ_live_params.rs"),
    include_str!("../../../nord-plugin/src/organ_extra_params.rs"),
    include_str!("../../../nord-plugin/src/fx_variation_params.rs"),
];
/// Engine sources an `engine:<path>` binding must be found in.
const ENGINE_SOURCES: [&str; 5] = [
    include_str!("../../../nord-core/src/engine/mod.rs"),
    include_str!("../../../nord-core/src/engine/extern_out.rs"),
    include_str!("../../../nord-core/src/engine/zone_assign.rs"),
    include_str!("../../../nord-core/src/morph.rs"),
    include_str!("../../../nord-core/src/synth/sound_init.rs"),
];
/// Source of the top-level struct that declares the nested `id_prefix` values.
const PARAMS_SRC: &str = include_str!("../../../nord-plugin/src/params.rs");

/// A bound id is either a literal host id, or `<prefix>_<name>[_{n}]` where
/// `<prefix>` is a nested `id_prefix` in `params.rs` and `<name>` is a literal `#[id]`.
fn host_id_exists(id: &str) -> bool {
    let quoted = format!("\"{id}\"");
    if HOST_SOURCES.iter().any(|s| s.contains(&quoted)) {
        return true;
    }
    let base = id.strip_suffix("_{n}").unwrap_or(id);
    let Some((prefix, name)) = base.split_once('_') else {
        return false;
    };
    PARAMS_SRC.contains(&format!("id_prefix = \"{prefix}\""))
        && HOST_SOURCES
            .iter()
            .any(|s| s.contains(&format!("#[id = \"{name}\"]")))
}

/// Every dot-separated segment of an `engine:` path is a public field (`pub seg:`) or a public
/// method (`pub fn seg(`) of an engine source.
fn engine_path_exists(path: &str) -> bool {
    path.split('.').all(|seg| {
        !seg.is_empty()
            && ENGINE_SOURCES.iter().any(|s| {
                s.contains(&format!("pub {seg}:")) || s.contains(&format!("pub fn {seg}("))
            })
    })
}

/// A `ui:<area>.<name>` binding names the GUI path that performs the control (no host parameter
/// or engine field backs it). The path is lowercase ASCII with one or more dots and an area
/// prefix that has a GUI proof test (`nord-gui`: program, synth, fx).
fn ui_action_ok(action: &str) -> bool {
    let Some((area, rest)) = action.split_once('.') else {
        return false;
    };
    ["program", "synth", "fx"].contains(&area)
        && !rest.is_empty()
        && rest
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '.')
}

fn count(section: Section) -> usize {
    all_controls()
        .iter()
        .filter(|c| c.section == section)
        .count()
}

#[test]
fn ids_are_unique_lowercase_ascii() {
    let mut seen = HashSet::new();
    for c in all_controls() {
        assert!(
            c.id.chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_'),
            "bad id {}",
            c.id
        );
        assert!(seen.insert(c.id.clone()), "duplicate id {}", c.id);
    }
}

#[test]
fn every_entry_has_page_label_and_source() {
    for c in all_controls() {
        assert!(
            (11..=53).contains(&c.manual_page),
            "{} page {}",
            c.id,
            c.manual_page
        );
        assert!(!c.label.is_empty(), "{} has no label", c.id);
        assert!(!c.source.is_empty(), "{} has no source", c.id);
    }
}

#[test]
fn every_binding_is_set() {
    for c in all_controls() {
        match c.binding {
            Binding::Bound(s) | Binding::UiOnly(s) | Binding::Unsupported(s) => {
                assert!(!s.is_empty(), "{} has an empty binding", c.id)
            }
        }
    }
}

#[test]
fn bound_ids_exist_in_the_host_parameter_list() {
    for c in all_controls() {
        if let Binding::Bound(id) = c.binding {
            if id.starts_with("dial:") {
                let cells = super::synth_dials::cells_of(id);
                assert!(!cells.is_empty(), "{}: no dial cells for {id}", c.id);
                for cell in cells {
                    if let DialTarget::Edit { host: Some(h), .. } = cell.target {
                        assert!(host_id_exists(h), "{}: dial host id {h} not found", c.id);
                    }
                }
            } else if let Some(action) = id.strip_prefix("ui:") {
                assert!(ui_action_ok(action), "{}: bad ui action {id}", c.id);
            } else if let Some(path) = id.strip_prefix("engine:") {
                assert!(
                    engine_path_exists(path),
                    "{}: engine path {path} not found in nord-core",
                    c.id
                );
            } else {
                assert!(
                    host_id_exists(id),
                    "{}: host id {id} not found in nord-plugin",
                    c.id
                );
            }
        }
    }
}

#[test]
fn unsupported_beans_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.beans");
    let mut names = Vec::new();
    for dir in [root.clone(), root.join("archive")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            names.extend(
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned()),
            );
        }
    }
    for c in all_controls() {
        if let Binding::Unsupported(bean) = c.binding {
            let prefix = format!("{bean}--");
            assert!(
                names.iter().any(|n| n.starts_with(&prefix)),
                "{}: bean {bean} not found",
                c.id
            );
        }
    }
}

#[test]
fn assumed_entries_explain_themselves() {
    for c in all_controls()
        .iter()
        .filter(|c| c.provenance == Provenance::Assumed)
    {
        assert_ne!(
            c.source, "manual text",
            "{} is Assumed but has the default source",
            c.id
        );
    }
}

#[test]
fn verified_entries_use_the_default_manual_source() {
    for c in all_controls()
        .iter()
        .filter(|c| c.provenance == Provenance::Verified)
    {
        assert_eq!(c.source, "manual text", "{}", c.id);
    }
}

#[test]
fn per_section_counts_are_pinned() {
    assert_eq!(count(Section::Master), 11);
    assert_eq!(count(Section::Organ), 50);
    assert_eq!(count(Section::Piano), 32);
    assert_eq!(count(Section::Program), 56);
    assert_eq!(count(Section::Synth), 73);
    assert_eq!(count(Section::Effects), 51);
    assert_eq!(count(Section::Keybed), 2);
}

#[test]
fn per_kind_counts_are_pinned() {
    let all = all_controls();
    let n = |k: ControlKind| all.iter().filter(|c| c.kind == k).count();
    assert_eq!(n(ControlKind::Fader), 7);
    assert_eq!(n(ControlKind::Drawbar), 9);
    assert_eq!(n(ControlKind::Oled), 2);
    assert_eq!(n(ControlKind::Wheel), 1);
    assert_eq!(n(ControlKind::Stick), 1);
}

#[test]
fn total_is_the_sum_of_sections() {
    assert_eq!(all_controls().len(), 275);
}

#[test]
fn binding_provenance_and_morph_totals_are_pinned() {
    let all = all_controls();
    let b = |f: fn(&Binding) -> bool| all.iter().filter(|c| f(&c.binding)).count();
    assert_eq!(b(|x| matches!(x, Binding::Bound(_))), 208);
    assert_eq!(b(|x| matches!(x, Binding::UiOnly(_))), 67);
    assert_eq!(b(|x| matches!(x, Binding::Unsupported(_))), 0);
    let assumed = all
        .iter()
        .filter(|c| c.provenance == Provenance::Assumed)
        .count();
    assert_eq!(assumed, 24);
    assert_eq!(all.iter().filter(|c| c.morphable).count(), 35);
}

#[test]
fn host_id_resolver_accepts_literals_and_nested_ids_only() {
    assert!(host_id_exists("master_level"));
    assert!(host_id_exists("pianox_unison_{n}"));
    assert!(host_id_exists("organx_close_mic"));
    assert!(host_id_exists("fxx_mod1_amount_{n}"));
    assert!(host_id_exists("synthm_arp_mode_{n}"));
    assert!(host_id_exists("synthg_kb_hold"));
    assert!(host_id_exists("fxy_glob_rev_{n}"));
    assert!(host_id_exists("fxsel_cabinet_{n}"));
    assert!(host_id_exists("ped_sustped_{n}"));
    assert!(host_id_exists("prog_transpose"));
    assert!(host_id_exists("organz_poly_perc_{n}"));
    assert!(host_id_exists("organz_drawbar_live_{n}"));
    assert!(host_id_exists("organz_sync_{n}"));
    assert!(host_id_exists("organy_rotary_on_{n}"));
    assert!(host_id_exists("organy_mic_angle_{n}"));
    assert!(host_id_exists("auxkb_organ"));
    assert!(host_id_exists("auxkb_synth"));
    assert!(host_id_exists("fxz_mod1_beats_{n}"));
    assert!(host_id_exists("fxz_rev_var_{n}"));
    assert!(!host_id_exists("pianox_nope_{n}"));
    assert!(!host_id_exists("organz_nope_{n}"));
    assert!(!host_id_exists("nopex_unison_{n}"));
    assert!(!host_id_exists("nothing"));
}

#[test]
fn ui_action_resolver_accepts_area_dot_name_only() {
    assert!(ui_action_ok("program.slot"));
    assert!(ui_action_ok("synth.page.env_osc"));
    assert!(ui_action_ok("fx.mod1_on"));
    assert!(!ui_action_ok("program"));
    assert!(!ui_action_ok("program."));
    assert!(!ui_action_ok("nowhere.thing"));
    assert!(!ui_action_ok("synth.Page"));
}

#[test]
fn engine_path_resolver_accepts_fields_and_rejects_unknown_ones() {
    assert!(engine_path_exists("split_router.layer_zone_masks"));
    assert!(!engine_path_exists("split_router.nonexistent_field"));
    assert!(engine_path_exists("synth_a.sound_init"), "a method segment");
    assert!(!engine_path_exists("synth_a.no_such_method"));
    assert!(!engine_path_exists(""));
    assert!(
        engine_path_exists("set_layer_extern"),
        "extern_out.rs method"
    );
}

#[test]
fn rewired_synth_and_effects_controls_are_bound() {
    let all = all_controls();
    for id in [
        "synth.sound_init",
        "synth.extern",
        "synth.on",
        "synth.kb_zone_down",
        "synth.kb_zone_up",
        "synth.zone_leds",
        "synth.aux_led.a",
        "synth.aux_led.b",
        "synth.aux_led.c",
        "reverb.variation",
        "mod1.mst_clk",
        "mod1.variation",
        "mod2.variation",
    ] {
        let c = all.iter().find(|c| c.id == id).unwrap();
        assert!(matches!(c.binding, Binding::Bound(_)), "{id}");
    }
}

#[test]
fn organ_piano_and_master_have_no_unsupported_strip_controls_left() {
    for c in all_controls() {
        let wired = matches!(c.section, Section::Organ | Section::Master)
            || (c.section == Section::Piano && c.id != "piano.select_dial");
        if wired {
            assert!(
                !matches!(c.binding, Binding::Unsupported(_)),
                "{} is still unsupported",
                c.id
            );
        }
    }
}

/// Every control that is still `Unsupported`, with its bean. A new entry here needs a reason in
/// `docs/specs/control-inventory.md` section 10 and a bean that states what data or work is missing.
#[test]
fn the_unsupported_set_is_pinned() {
    let left: Vec<(String, &str)> = all_controls()
        .into_iter()
        .filter_map(|c| match c.binding {
            Binding::Unsupported(b) => Some((c.id, b)),
            _ => None,
        })
        .collect();
    assert!(
        left.is_empty(),
        "no control is Unsupported any more: {left:?}"
    );
}

/// Controls the classification pass (`ns4-lu50`) moved out of `Unsupported`, by binding kind.
#[test]
fn classification_pass_totals_are_pinned() {
    let all = all_controls();
    let ui_bound = |prefix: &str| {
        all.iter()
            .filter(|c| matches!(c.binding, Binding::Bound(b) if b.starts_with(prefix)))
            .count()
    };
    assert_eq!(ui_bound("ui:program."), 30);
    assert_eq!(ui_bound("ui:synth."), 18);
    assert_eq!(ui_bound("ui:fx."), 5);
    assert_eq!(ui_bound("ui:"), 53);
}
