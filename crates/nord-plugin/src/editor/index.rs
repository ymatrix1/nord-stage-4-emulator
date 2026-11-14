//! Lookup of host parameters by id, plus resolution of inventory binding ids to param families.
//!
//! Inventory `Binding::Bound` ids (`docs/specs/control-inventory.md` section 2) are either a
//! literal host id (`master_level`), a template with the per-layer index (`ped_sustped_{n}`) or
//! a bare name of a field inside a per-layer bank (`drive` is `fx_drive_{n}`). The plugin names
//! array entries `<prefix>_<id>_<n>` (1-based), so resolving is a suffix match on the param map.

use nih_plug::prelude::{ParamPtr, Params};
use std::collections::HashMap;

/// Every host parameter of a `Params` tree by id.
pub struct ParamIndex {
    map: HashMap<String, ParamPtr>,
}

impl ParamIndex {
    /// Collect the parameters. The caller keeps `params` alive for as long as the index is used.
    pub fn new(params: &dyn Params) -> Self {
        Self {
            map: params
                .param_map()
                .into_iter()
                .map(|(id, ptr, _group)| (id, ptr))
                .collect(),
        }
    }

    pub fn ptr(&self, id: &str) -> Option<ParamPtr> {
        self.map.get(id).copied()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Plain value of a parameter (bool as 0/1, int and enum as the index), or `None`.
    pub fn plain(&self, id: &str) -> Option<f32> {
        // SAFETY: see `GuiParamWriter::write`; the params outlive the index.
        self.ptr(id).map(|p| unsafe { p.unmodulated_plain_value() })
    }

    /// Plain value of a `family` (+ optional layer) parameter.
    pub fn plain_of(&self, family: &str, layer: Option<u8>) -> Option<f32> {
        match layer {
            Some(n) => self.plain(&format!("{family}_{n}")),
            None => self.plain(family),
        }
    }

    /// Param family an inventory binding template resolves to, or `None` when no host parameter
    /// matches. The family is the id without the layer suffix: `fx_drive` for `drive`,
    /// `master_level` for `master_level`, `ped_sustped` for `ped_sustped_{n}`.
    pub fn family_of(&self, template: &str) -> Option<String> {
        if let Some(base) = template.strip_suffix("_{n}") {
            return self
                .contains(&format!("{base}_1"))
                .then(|| base.to_string());
        }
        if self.contains(template) {
            return Some(template.to_string());
        }
        // Bare name inside a per-layer bank: `<prefix>_<template>_1` with a one-word prefix.
        let tail = format!("_{template}_1");
        let mut hits = self.map.keys().filter(|id| {
            id.strip_suffix(tail.as_str())
                .is_some_and(|prefix| !prefix.is_empty() && !prefix.contains('_'))
        });
        let first = hits.next()?;
        hits.next().is_none().then(|| {
            first
                .strip_suffix("_1")
                .unwrap_or(first.as_str())
                .to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Stage4Params;

    fn index() -> (Stage4Params, ParamIndex) {
        let p = Stage4Params::default();
        let i = ParamIndex::new(&p);
        (p, i)
    }

    #[test]
    fn index_holds_every_param_and_reads_defaults() {
        let (_p, i) = index();
        assert!(i.len() > 700, "only {} params", i.len());
        assert!(!i.is_empty());
        assert!((i.plain("master_level").unwrap() - 0.8).abs() < 1e-6);
        assert!(i.plain("no_such_param").is_none());
        assert!(i.plain_of("fx_drive", Some(2)).is_some());
        assert!(i.plain_of("master_level", None).is_some());
    }

    #[test]
    fn literal_template_and_bare_bank_names_resolve_to_families() {
        let (_p, i) = index();
        assert_eq!(i.family_of("master_level").as_deref(), Some("master_level"));
        assert_eq!(
            i.family_of("ped_sustped_{n}").as_deref(),
            Some("ped_sustped")
        );
        assert_eq!(i.family_of("drive").as_deref(), Some("fx_drive"));
        assert_eq!(i.family_of("mod1_rate").as_deref(), Some("fxsel_mod1_rate"));
        assert_eq!(i.family_of("no_such_thing"), None);
        assert_eq!(i.family_of("no_such_{n}"), None);
    }
}
