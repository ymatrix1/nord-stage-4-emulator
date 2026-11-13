//! Host parameter-ID stability contract (Phase 6b B3, `ns4-xhbf`).
//!
//! `param-ids.snapshot` lists every shipped host parameter as
//! `id|step_count|default_normalized`. Removing an id, or changing its step
//! count (type/range class) or default, fails the test. Adding ids passes.
//! Regenerate after an intentional additive change with:
//! `UPDATE_PARAM_SNAPSHOT=1 cargo test -p nord-plugin param_snapshot`.

use std::collections::BTreeMap;

/// One snapshot row.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamRow {
    pub step_count: Option<usize>,
    pub default_normalized: String,
}

/// Render rows as snapshot text (sorted by id).
pub fn render(rows: &BTreeMap<String, ParamRow>) -> String {
    let mut out = String::new();
    for (id, r) in rows {
        let steps = r.step_count.map_or("cont".to_string(), |s| s.to_string());
        out.push_str(&format!("{id}|{steps}|{}\n", r.default_normalized));
    }
    out
}

/// Parse snapshot text; malformed lines are an error.
pub fn parse(text: &str) -> Result<BTreeMap<String, ParamRow>, String> {
    let mut rows = BTreeMap::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 3 {
            return Err(format!("line {}: expected 3 fields", n + 1));
        }
        let step_count = match parts[1] {
            "cont" => None,
            s => Some(
                s.parse::<usize>()
                    .map_err(|e| format!("line {}: {e}", n + 1))?,
            ),
        };
        rows.insert(
            parts[0].to_string(),
            ParamRow {
                step_count,
                default_normalized: parts[2].to_string(),
            },
        );
    }
    Ok(rows)
}

/// Contract violations of `current` against the committed `snapshot`
/// (removed ids and changed step count or default). Added ids are allowed.
pub fn violations(
    snapshot: &BTreeMap<String, ParamRow>,
    current: &BTreeMap<String, ParamRow>,
) -> Vec<String> {
    let mut v = Vec::new();
    for (id, old) in snapshot {
        match current.get(id) {
            None => v.push(format!("removed or renamed: {id}")),
            Some(new) if new != old => v.push(format!("changed: {id}: {old:?} -> {new:?}")),
            Some(_) => {}
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Stage4Params;
    use nih_plug::prelude::Params;

    fn current_rows() -> BTreeMap<String, ParamRow> {
        let params = Stage4Params::default();
        let mut rows = BTreeMap::new();
        for (id, ptr, _) in params.param_map() {
            // SAFETY: `params` outlives `ptr`; both calls only read metadata.
            let (steps, default) = unsafe { (ptr.step_count(), ptr.default_normalized_value()) };
            let row = ParamRow {
                step_count: steps,
                default_normalized: format!("{default:.6}"),
            };
            assert!(rows.insert(id.clone(), row).is_none(), "duplicate id {id}");
        }
        rows
    }

    fn row(steps: Option<usize>, d: &str) -> ParamRow {
        ParamRow {
            step_count: steps,
            default_normalized: d.to_string(),
        }
    }

    const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/param-ids.snapshot");

    #[test]
    fn shipped_ids_are_stable() {
        let current = current_rows();
        if std::env::var("UPDATE_PARAM_SNAPSHOT").is_ok() {
            std::fs::write(SNAPSHOT_PATH, render(&current)).unwrap();
        }
        let text = std::fs::read_to_string(SNAPSHOT_PATH).expect("param-ids.snapshot missing");
        let snapshot = parse(&text).unwrap();
        assert!(!snapshot.is_empty());
        let bad = violations(&snapshot, &current);
        assert!(
            bad.is_empty(),
            "host parameter contract broken:\n{}",
            bad.join("\n")
        );
    }

    #[test]
    fn removal_is_a_violation() {
        let snap = BTreeMap::from([("a".to_string(), row(None, "0.5"))]);
        let cur = BTreeMap::new();
        assert_eq!(
            violations(&snap, &cur),
            vec!["removed or renamed: a".to_string()]
        );
    }

    #[test]
    fn rename_is_a_violation() {
        let snap = BTreeMap::from([("a".to_string(), row(None, "0.5"))]);
        let cur = BTreeMap::from([("b".to_string(), row(None, "0.5"))]);
        assert_eq!(violations(&snap, &cur).len(), 1);
    }

    #[test]
    fn type_or_default_change_is_a_violation() {
        let snap = BTreeMap::from([("a".to_string(), row(Some(3), "0.5"))]);
        let steps = BTreeMap::from([("a".to_string(), row(Some(4), "0.5"))]);
        let def = BTreeMap::from([("a".to_string(), row(Some(3), "0.6"))]);
        assert_eq!(violations(&snap, &steps).len(), 1);
        assert_eq!(violations(&snap, &def).len(), 1);
    }

    #[test]
    fn additive_change_passes() {
        let snap = BTreeMap::from([("a".to_string(), row(None, "0.5"))]);
        let mut cur = snap.clone();
        cur.insert("new".to_string(), row(Some(1), "0.0"));
        assert!(violations(&snap, &cur).is_empty());
    }

    #[test]
    fn render_parse_round_trip() {
        let rows = BTreeMap::from([
            ("a".to_string(), row(None, "0.500000")),
            ("b".to_string(), row(Some(7), "1.000000")),
        ]);
        assert_eq!(parse(&render(&rows)).unwrap(), rows);
        assert!(parse("bad line").is_err());
    }
}
