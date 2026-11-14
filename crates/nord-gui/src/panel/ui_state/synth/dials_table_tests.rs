//! The inventory's dial table (`nord_panel::inventory::synth_dials`) against the behaviour: every
//! dial of every display page is turned and the edit it emits is compared with its cell.

use super::page::{dial_defs, ALL_PAGES_FOR_TESTS};
use super::SynthUi;
use crate::audio::SynthEdit;
use nord_core::synth::SynthOscMode;
use nord_panel::inventory::synth_dials::{DialCell, DialTarget, DIAL_CELLS};

fn cell(key: &str, dial: usize) -> Option<&'static DialCell> {
    DIAL_CELLS
        .iter()
        .find(|c| c.page == key && usize::from(c.dial) == dial)
}

/// Debug text of every audio edit the dial queued (`Param` edits by their parameter).
fn emitted(page_idx: usize, dial: usize, steps: i32) -> Vec<String> {
    let (page, mode) = ALL_PAGES_FOR_TESTS[page_idx];
    let mut s = SynthUi::new();
    s.layer_mut().sample_mode = mode == SynthOscMode::Sample;
    s.layer_mut().arp_run = true;
    s.page = page;
    s.turn_dial(dial, steps, false);
    s.take_edits()
        .into_iter()
        .map(|e| match e {
            SynthEdit::Param(_, p) => format!("{p:?}"),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn every_defined_dial_has_one_cell_and_unused_dials_have_none() {
    let mut keys = std::collections::HashSet::new();
    for (p, mode) in ALL_PAGES_FOR_TESTS {
        let key = p.dial_key(mode);
        if !keys.insert(key) {
            continue; // Extern repeats in Sample mode
        }
        for (i, def) in dial_defs(p, mode).iter().enumerate() {
            match (def, cell(key, i)) {
                (Some(d), Some(c)) => {
                    assert_eq!(d.label, c.label, "{key}/{i} label");
                    assert_eq!(
                        d.unsupported.is_some(),
                        matches!(c.target, DialTarget::Gap(_)),
                        "{key}/{i} gap"
                    );
                }
                (None, None) => {}
                (d, c) => panic!("{key}/{i}: dial_defs {d:?} vs table {c:?}"),
            }
        }
    }
    let used: usize = DIAL_CELLS.iter().filter(|c| keys.contains(c.page)).count();
    assert_eq!(used, DIAL_CELLS.len(), "a table page no display page uses");
}

#[test]
fn dial_table_matches_the_behaviour() {
    for (idx, (p, mode)) in ALL_PAGES_FOR_TESTS.iter().enumerate() {
        let key = p.dial_key(*mode);
        for dial in 0..3 {
            let Some(c) = cell(key, dial) else { continue };
            let mut got = emitted(idx, dial, 1);
            if got.is_empty() {
                got = emitted(idx, dial, -1); // a value already at its upper end
            }
            match c.target {
                DialTarget::Edit { edits, .. } => assert!(
                    got.iter()
                        .any(|g| edits.split('|').any(|alt| g.starts_with(alt))),
                    "{key}/{dial} {}: wanted {edits}, got {got:?}",
                    c.label
                ),
                DialTarget::Ui(_) | DialTarget::Gap(_) => assert!(
                    got.is_empty(),
                    "{key}/{dial} {}: expected no audio edit, got {got:?}",
                    c.label
                ),
            }
        }
    }
}
