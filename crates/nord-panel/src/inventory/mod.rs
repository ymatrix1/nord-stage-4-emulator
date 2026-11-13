//! Machine-checked list of front-panel controls (`docs/specs/control-inventory.md` §2-§4).
//!
//! Skeleton: the types and validator exist; the entries are filled per section by B2/B8.

mod types;

pub use types::{Binding, ControlEntry, ControlId, ControlKind, Layer, Provenance, Section};

/// Every control of the panel. Empty until B2 (`ns4-3n0d`) verifies the list against the manual.
pub const ALL: &[ControlEntry] = &[];

/// Problems found by [`validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    DuplicateId(&'static str),
    EmptySource(&'static str),
}

/// Checks the rules of spec §4 that need no layout or host data: unique ids, non-empty source.
pub fn validate(entries: &[ControlEntry]) -> Vec<InventoryError> {
    let mut errors = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if entries[..i].iter().any(|o| o.id == e.id) {
            errors.push(InventoryError::DuplicateId(e.id.0));
        }
        if e.source.trim().is_empty() {
            errors.push(InventoryError::EmptySource(e.id.0));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &'static str, source: &'static str) -> ControlEntry {
        ControlEntry {
            id: ControlId(id),
            section: Section::Organ,
            kind: ControlKind::Drawbar,
            layer: Some(Layer::A),
            manual_page: 0,
            provenance: Provenance::Assumed,
            source,
            binding: Binding::UiOnly("test"),
        }
    }

    #[test]
    fn test_skeleton_inventory_is_valid() {
        assert!(validate(ALL).is_empty());
    }

    #[test]
    fn test_validate_flags_duplicates_and_empty_sources() {
        let list = [entry("a", "p. 1"), entry("a", " ")];
        assert_eq!(
            validate(&list),
            vec![
                InventoryError::DuplicateId("a"),
                InventoryError::EmptySource("a")
            ]
        );
    }
}
