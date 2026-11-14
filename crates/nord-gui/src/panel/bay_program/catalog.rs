//! The program / piano / sample names of the opened backup library as the panel sees them, and
//! the load requests the panel hands back to the app (the panel never touches the archive).

use super::model::ListMode;
use std::path::Path;

/// What the panel asks the library to load. Drained by `BackupLibraryState::sync_program_panel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadRequest {
    /// A `.ns4p` entry.
    Program(String),
    /// A `.npno` entry into Piano A (`slot_b = false`) or B.
    Piano { entry: String, slot_b: bool },
    /// A `.nsmp4` entry into Synth layer `0..3`.
    Sample { entry: String, layer: usize },
}

/// Names of the opened `.ns4b` backup (empty while none is open).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub programs: Vec<String>,
    pub pianos: Vec<String>,
    pub samples: Vec<String>,
}

/// File stem of an archive entry: `Program/Bank A/Sweet Layers.ns4p` -> `Sweet Layers`.
pub fn display_name(entry: &str) -> String {
    Path::new(entry)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(entry)
        .to_string()
}

/// Category label of a program entry: its folder in the archive.
///
/// ASSUMED: the backup stores programs as `Program/<folder>/<name>.ns4p`; the real category is
/// inside the decoded program and decoding every entry for a list would be slow, so the folder
/// stands in for it.
pub fn category_of(entry: &str) -> String {
    let mut parts = entry.split(['/', '\\']).collect::<Vec<_>>();
    parts.pop();
    parts.last().map(|s| (*s).to_string()).unwrap_or_default()
}

impl Catalog {
    /// Replaces the lists; returns `true` when anything changed.
    pub fn set(&mut self, programs: &[String], pianos: &[String], samples: &[String]) -> bool {
        let new = Catalog {
            programs: programs.to_vec(),
            pianos: pianos.to_vec(),
            samples: samples.to_vec(),
        };
        let changed = *self != new;
        if changed {
            *self = new;
        }
        changed
    }

    /// Program indices in LIST order: Num = archive order, Abc = by name, Cat = by category.
    pub fn order(&self, mode: ListMode) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.programs.len()).collect();
        match mode {
            ListMode::Num => {}
            ListMode::Abc => idx.sort_by_key(|&i| display_name(&self.programs[i]).to_lowercase()),
            ListMode::Cat => idx.sort_by_key(|&i| {
                (
                    category_of(&self.programs[i]).to_lowercase(),
                    display_name(&self.programs[i]).to_lowercase(),
                )
            }),
        }
        idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        Catalog {
            programs: vec![
                "Program/Bank B/Zebra.ns4p".into(),
                "Program/Bank A/mellow.ns4p".into(),
                "Program/Bank A/Alpha.ns4p".into(),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn names_and_categories_come_from_the_entry_path() {
        assert_eq!(
            display_name("Program/Bank A/Sweet Layers.ns4p"),
            "Sweet Layers"
        );
        assert_eq!(category_of("Program/Bank A/Sweet Layers.ns4p"), "Bank A");
        assert_eq!(category_of("loose.ns4p"), "");
    }

    #[test]
    fn list_orders_num_abc_cat() {
        let c = cat();
        assert_eq!(c.order(ListMode::Num), vec![0, 1, 2]);
        assert_eq!(
            c.order(ListMode::Abc),
            vec![2, 1, 0],
            "Alpha, mellow, Zebra"
        );
        assert_eq!(
            c.order(ListMode::Cat),
            vec![2, 1, 0],
            "Bank A (Alpha, mellow), Bank B"
        );
    }

    #[test]
    fn set_reports_changes_only() {
        let mut c = Catalog::default();
        let p = vec!["a.ns4p".to_string()];
        assert!(c.set(&p, &[], &[]));
        assert!(!c.set(&p, &[], &[]));
        assert!(c.set(&p, &["x.npno".into()], &[]));
    }
}
