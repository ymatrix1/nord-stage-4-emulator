//! Cross-section checks of the inventory (`docs/specs/control-inventory.md` §4).

use super::all_controls;
use crate::types::{Binding, ControlKind, Provenance, Section};
use std::collections::HashSet;
use std::path::Path;

/// Host parameter sources; a `Bound` id must appear as a quoted literal here.
const HOST_SOURCES: [&str; 14] = [
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
    include_str!("../../../nord-plugin/src/fx_variation_params.rs"),
];
/// Source of the top-level struct that declares the nested `id_prefix` values.
const PARAMS_SRC: &str = include_str!("../../../nord-plugin/src/params.rs");
/// Engine sources an `engine:<path>` binding must be found in (a field `seg:` or `fn seg(`).
const ENGINE_SOURCES: [&str; 6] = [
    include_str!("../../../nord-core/src/engine/mod.rs"),
    include_str!("../../../nord-core/src/engine/pedals.rs"),
    include_str!("../../../nord-core/src/engine/aux_kb.rs"),
    include_str!("../../../nord-core/src/synth/sound_init.rs"),
    include_str!("../../../nord-core/src/synth/pattern.rs"),
    include_str!("../../../nord-core/src/fx/mod.rs"),
];

/// Every dot-separated segment of an `engine:` path names a field or method in the engine sources.
fn engine_path_exists(path: &str) -> bool {
    path.split('.').all(|seg| {
        !seg.is_empty()
            && ENGINE_SOURCES
                .iter()
                .any(|s| s.contains(&format!("{seg}:")) || s.contains(&format!("fn {seg}(")))
    })
}

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
            if let Some(path) = id.strip_prefix("engine:") {
                assert!(engine_path_exists(path), "{}: engine path {path}", c.id);
                continue;
            }
            assert!(
                host_id_exists(id),
                "{}: host id {id} not found in nord-plugin",
                c.id
            );
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
    assert_eq!(count(Section::Master), 10);
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
    assert_eq!(all_controls().len(), 274);
}

#[test]
fn binding_provenance_and_morph_totals_are_pinned() {
    let all = all_controls();
    let b = |f: fn(&Binding) -> bool| all.iter().filter(|c| f(&c.binding)).count();
    assert_eq!(b(|x| matches!(x, Binding::Bound(_))), 122);
    assert_eq!(b(|x| matches!(x, Binding::UiOnly(_))), 49);
    assert_eq!(b(|x| matches!(x, Binding::Unsupported(_))), 103);
    let assumed = all
        .iter()
        .filter(|c| c.provenance == Provenance::Assumed)
        .count();
    assert_eq!(assumed, 23);
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
    assert!(host_id_exists("fxz_mod1_beats_{n}"));
    assert!(host_id_exists("fxz_rev_var_{n}"));
    assert!(!host_id_exists("pianox_nope_{n}"));
    assert!(!host_id_exists("nopex_unison_{n}"));
    assert!(!host_id_exists("nothing"));
}

#[test]
fn engine_path_resolver_accepts_real_paths_only() {
    assert!(engine_path_exists("synth_a.sound_init"));
    assert!(engine_path_exists("pedals.sustped"));
    assert!(!engine_path_exists("synth_a.no_such_method"));
    assert!(!engine_path_exists("nothing.sound_init"));
    assert!(!engine_path_exists(""));
}

#[test]
fn rewired_synth_and_effects_controls_are_bound() {
    let all = all_controls();
    for id in [
        "synth.sound_init",
        "reverb.variation",
        "mod1.mst_clk",
        "mod1.variation",
        "mod2.variation",
    ] {
        let c = all.iter().find(|c| c.id == id).unwrap();
        assert!(matches!(c.binding, Binding::Bound(_)), "{id}");
    }
}
