//! Cross-section checks of the inventory (`docs/specs/control-inventory.md` §4).

use super::all_controls;
use crate::types::{Binding, ControlKind, Provenance, Section};
use std::collections::HashSet;
use std::path::Path;

/// Host parameter sources; a `Bound` id must appear as a quoted literal here.
const HOST_SOURCES: [&str; 4] = [
    include_str!("../../../nord-plugin/src/params.rs"),
    include_str!("../../../nord-plugin/src/fx_params.rs"),
    include_str!("../../../nord-plugin/src/fx_selectors.rs"),
    include_str!("../../../nord-plugin/src/morph_assign.rs"),
];

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
            let quoted = format!("\"{id}\"");
            assert!(
                HOST_SOURCES.iter().any(|s| s.contains(&quoted)),
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
    assert_eq!(count(Section::Effects), 50);
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
    assert_eq!(all_controls().len(), 273);
}

#[test]
fn binding_provenance_and_morph_totals_are_pinned() {
    let all = all_controls();
    let b = |f: fn(&Binding) -> bool| all.iter().filter(|c| f(&c.binding)).count();
    assert_eq!(b(|x| matches!(x, Binding::Bound(_))), 66);
    assert_eq!(b(|x| matches!(x, Binding::UiOnly(_))), 49);
    assert_eq!(b(|x| matches!(x, Binding::Unsupported(_))), 158);
    let assumed = all
        .iter()
        .filter(|c| c.provenance == Provenance::Assumed)
        .count();
    assert_eq!(assumed, 23);
    assert_eq!(all.iter().filter(|c| c.morphable).count(), 35);
}
