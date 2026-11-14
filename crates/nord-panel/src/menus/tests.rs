//! Cross-menu checks of the menu model (`docs/specs/midi-and-control.md` section 5).

use super::*;
use std::collections::HashSet;
use std::path::Path;

/// Host parameter sources; a `Bound` host id is `<id_prefix>_<#[id]>`.
const HOST_SOURCES: [&str; 3] = [
    include_str!("../../../nord-plugin/src/aux_pedal_params.rs"),
    include_str!("../../../nord-plugin/src/program_params.rs"),
    include_str!("../../../nord-plugin/src/settings_params.rs"),
];
const PARAMS_SRC: &str = include_str!("../../../nord-plugin/src/params.rs");
/// Engine sources an `engine:` binding must be found in.
const ENGINE_SOURCES: [&str; 5] = [
    include_str!("../../../nord-core/src/engine/mod.rs"),
    include_str!("../../../nord-core/src/engine/extern_out.rs"),
    include_str!("../../../nord-core/src/engine/aux_kb.rs"),
    include_str!("../../../nord-core/src/engine/pedals.rs"),
    include_str!("../../../nord-core/src/engine/settings.rs"),
];

fn host_id_exists(id: &str) -> bool {
    let Some((prefix, name)) = id.split_once('_') else {
        return false;
    };
    PARAMS_SRC.contains(&format!("id_prefix = \"{prefix}\""))
        && HOST_SOURCES
            .iter()
            .any(|s| s.contains(&format!("#[id = \"{name}\"]")))
}

fn engine_path_exists(path: &str) -> bool {
    path.split('.').all(|seg| {
        !seg.is_empty()
            && ENGINE_SOURCES
                .iter()
                .any(|s| s.contains(&format!("pub {seg}:")) || s.contains(&format!("{seg}:")))
    })
}

#[test]
fn ids_are_unique_lowercase_ascii() {
    let mut seen = HashSet::new();
    for (_, i) in all_items() {
        assert!(
            i.id.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_'),
            "bad id {}",
            i.id
        );
        assert!(seen.insert(i.id), "duplicate id {}", i.id);
        assert!(
            !i.label.is_empty() && !i.note.is_empty(),
            "{} lacks text",
            i.id
        );
    }
}

#[test]
fn pages_exist_from_one_to_n_per_menu() {
    let expected = [
        (Menu::System, 6),
        (Menu::Sound, 11),
        (Menu::Organize, 2),
        (Menu::AuxKb, 3),
        (Menu::Output, 6),
        (Menu::Pedal, 7),
        (Menu::Midi, 10),
        (Menu::Extern, 13),
    ];
    for (menu, n) in expected {
        let pages: Vec<u8> = pages_of(menu).iter().map(|p| p.page).collect();
        assert_eq!(pages, (1..=n).collect::<Vec<u8>>(), "{menu:?}");
        for p in pages_of(menu) {
            assert!(
                !p.items.is_empty() && !p.title.is_empty(),
                "{menu:?} {}",
                p.page
            );
        }
    }
    assert_eq!(all_pages().len(), 58);
    assert_eq!(all_items().len(), 129);
}

#[test]
fn item_ids_name_their_menu_and_page() {
    for (p, i) in all_items() {
        let menu = match p.menu {
            Menu::System => "system",
            Menu::Sound => "sound",
            Menu::Organize => "organize",
            Menu::AuxKb => "aux_kb",
            Menu::Output => "output",
            Menu::Pedal => "pedal",
            Menu::Midi => "midi",
            Menu::Extern => "extern",
        };
        assert!(
            i.id.starts_with(&format!("{menu}.{}.", p.page)),
            "{} on {menu} page {}",
            i.id,
            p.page
        );
    }
}

#[test]
fn defaults_match_the_kind_and_lie_inside_the_range() {
    for (_, i) in all_items() {
        match (i.kind, i.default) {
            (ItemKind::Toggle, Value::Bool(_)) | (ItemKind::Action, Value::None) => {}
            (ItemKind::Enum(c), Value::Index(d)) => {
                assert!(c.len() >= 2 && d < c.len(), "{} default {d}", i.id)
            }
            (ItemKind::Int { min, max, step }, Value::Int(d)) => {
                assert!(
                    min < max && step > 0 && (min..=max).contains(&d),
                    "{}",
                    i.id
                )
            }
            (ItemKind::Float { min, max, step }, Value::Float(d)) => {
                assert!(
                    min < max && step > 0.0 && (min..=max).contains(&d),
                    "{}",
                    i.id
                )
            }
            (k, d) => panic!("{}: default {d:?} does not fit kind {k:?}", i.id),
        }
        assert!(
            (45..=64).contains(&i.manual_page),
            "{} page {}",
            i.id,
            i.manual_page
        );
    }
}

#[test]
fn enum_choices_are_distinct() {
    for (_, i) in all_items() {
        if let ItemKind::Enum(c) = i.kind {
            let set: HashSet<_> = c.iter().collect();
            assert_eq!(set.len(), c.len(), "{} has duplicate choices", i.id);
        }
    }
}

#[test]
fn bindings_are_non_empty_and_bound_ids_exist() {
    for (_, i) in all_items() {
        match i.binding {
            Binding::Bound(id) => {
                if let Some(path) = id.strip_prefix("engine:") {
                    assert!(engine_path_exists(path), "{}: engine path {path}", i.id);
                } else {
                    assert!(
                        host_id_exists(id),
                        "{}: host id {id} not in nord-plugin",
                        i.id
                    );
                }
            }
            Binding::UiOnly(s) | Binding::Unsupported(s) => {
                assert!(!s.is_empty(), "{} has an empty binding", i.id)
            }
        }
    }
}

#[test]
fn host_id_resolver_rejects_unknown_ids() {
    assert!(host_id_exists("sys_transpose"));
    assert!(host_id_exists("prog_level_db"));
    assert!(host_id_exists("ped_swell_gain"));
    assert!(!host_id_exists("sys_nope"));
    assert!(!host_id_exists("nope_transpose"));
    assert!(!host_id_exists("transpose"));
    assert!(engine_path_exists("extern_out.layers.send_on_load"));
    assert!(!engine_path_exists("extern_out.layers.nonexistent_field"));
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
    for (_, i) in all_items() {
        if let Binding::Unsupported(bean) = i.binding {
            let prefix = format!("{bean}--");
            assert!(
                names.iter().any(|n| n.starts_with(&prefix)),
                "{}: bean {bean} not found",
                i.id
            );
        }
    }
}

#[test]
fn assumed_and_empirical_items_explain_themselves() {
    for (_, i) in all_items() {
        if i.provenance != crate::types::Provenance::Verified {
            assert!(i.note.len() > 10 && i.note != MANUAL, "{}", i.id);
        }
    }
}

#[test]
fn binding_totals_are_pinned() {
    let (mut bound, mut ui, mut un) = (0, 0, 0);
    for (_, i) in all_items() {
        match i.binding {
            Binding::Bound(_) => bound += 1,
            Binding::UiOnly(_) => ui += 1,
            Binding::Unsupported(_) => un += 1,
        }
    }
    assert_eq!((bound, ui, un), (56, 12, 61));
}
