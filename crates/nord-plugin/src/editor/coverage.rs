//! Which inventory controls are live in the plugin editor and which are dimmed.
//!
//! Every control of `nord_panel::all_controls()` gets one [`Fate`]:
//!
//! | Fate | Meaning |
//! |---|---|
//! | `Live` | `Binding::Bound` to a host parameter the editor sink writes |
//! | `Passive` | `UiOnly` / `Unsupported` in the inventory: drawn as in the standalone, no host parameter by design |
//! | `Dimmed` | `Bound`, but the host parameter is missing or the sink does not write it yet |
//! | `Shared` | would be dimmed but shares its rectangle with a live or passive control (a Shift function of a live knob), so it stays undimmed |
//!
//! Dimmed controls get a translucent overlay and the tooltip [`DIM_TOOLTIP`].

use super::index::ParamIndex;
use super::probe::mapped_families;
use nord_panel::types::{Binding, ControlEntry};

/// Hover text of a dimmed control.
pub const DIM_TOOLTIP: &str = "not automatable in the plugin yet";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Live,
    Passive,
    Dimmed,
    Shared,
}

/// Classification of the whole inventory.
#[derive(Debug, Clone)]
pub struct Coverage {
    pub entries: Vec<(String, Fate)>,
}

impl Coverage {
    pub fn count(&self, fate: Fate) -> usize {
        self.entries.iter().filter(|(_, f)| *f == fate).count()
    }

    /// Ids to overlay.
    pub fn dimmed_ids(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|(_, f)| *f == Fate::Dimmed)
            .map(|(id, _)| id.as_str())
    }
}

fn first_pass(c: &ControlEntry, index: &ParamIndex, mapped: &[&str]) -> Fate {
    match c.binding {
        Binding::Bound(template) => match index.family_of(template) {
            Some(fam) if mapped.contains(&fam.as_str()) => Fate::Live,
            _ => Fate::Dimmed,
        },
        Binding::UiOnly(_) | Binding::Unsupported(_) => Fate::Passive,
    }
}

/// Classify every inventory control against the plugin's parameters and the sink's mapping.
pub fn classify(index: &ParamIndex) -> Coverage {
    let mapped: Vec<&str> = mapped_families().into_iter().collect();
    let all = nord_panel::all_controls();
    let mut fates: Vec<Fate> = all.iter().map(|c| first_pass(c, index, &mapped)).collect();
    let rects: Vec<_> = all
        .iter()
        .map(|c| nord_panel::layout::rect_for(&c.id))
        .collect();
    for i in 0..all.len() {
        if fates[i] != Fate::Dimmed {
            continue;
        }
        let shared = (0..all.len())
            .any(|j| matches!(fates[j], Fate::Live | Fate::Passive) && rects[j] == rects[i]);
        if shared {
            fates[i] = Fate::Shared;
        }
    }
    Coverage {
        entries: all.iter().map(|c| c.id.clone()).zip(fates).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Stage4Params;

    fn coverage() -> Coverage {
        let p = Stage4Params::default();
        classify(&ParamIndex::new(&p))
    }

    #[test]
    fn every_inventory_control_gets_exactly_one_fate() {
        let c = coverage();
        let all = nord_panel::all_controls();
        assert_eq!(c.entries.len(), all.len());
        for (entry, (id, _)) in all.iter().zip(&c.entries) {
            assert_eq!(&entry.id, id);
        }
        let total = [Fate::Live, Fate::Passive, Fate::Dimmed, Fate::Shared]
            .iter()
            .map(|f| c.count(*f))
            .sum::<usize>();
        assert_eq!(total, all.len());
    }

    #[test]
    fn live_and_dimmed_partition_the_bound_controls() {
        let c = coverage();
        let bound = nord_panel::all_controls()
            .iter()
            .filter(|e| matches!(e.binding, Binding::Bound(_)))
            .count();
        assert_eq!(
            c.count(Fate::Live) + c.count(Fate::Dimmed) + c.count(Fate::Shared),
            bound
        );
        assert!(
            c.count(Fate::Live) > 40,
            "only {} live",
            c.count(Fate::Live)
        );
        assert!(c.count(Fate::Passive) > 100);
    }

    #[test]
    fn dimmed_controls_have_pixel_rects_and_known_examples_are_live() {
        let c = coverage();
        for id in c.dimmed_ids() {
            assert!(nord_panel::layout::rect_for(id).is_some(), "{id}");
        }
        let fate = |id: &str| c.entries.iter().find(|(i, _)| i == id).map(|(_, f)| *f);
        assert_eq!(fate("master.level"), Some(Fate::Live));
        assert_eq!(fate("no.such.control"), None);
        assert_eq!(DIM_TOOLTIP, "not automatable in the plugin yet");
    }

    #[test]
    fn unresolvable_host_ids_are_dimmed() {
        let p = Stage4Params::default();
        let idx = ParamIndex::new(&p);
        let c = ControlEntry::new(
            "x.y",
            nord_panel::types::Section::Master,
            nord_panel::types::ControlKind::Knob,
            "X",
            12,
        );
        let mut c = c;
        c.binding = Binding::Bound("no_such_param_{n}");
        assert_eq!(first_pass(&c, &idx, &[]), Fate::Dimmed);
        c.binding = Binding::Bound("master_level");
        assert_eq!(first_pass(&c, &idx, &["master_level"]), Fate::Live);
        assert_eq!(first_pass(&c, &idx, &[]), Fate::Dimmed);
    }
}
