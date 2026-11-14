//! Layout invariants over the whole inventory.

use super::*;
use crate::inventory::all_controls;
use crate::types::{Access, ControlEntry};

/// Controls whose inventory section differs from the area they physically
/// sit in; they only have to lie inside the panel. `program.shift.right` is
/// the SHIFT rocker beside the Layer Effects block.
const OUTSIDE_OWN_SECTION: &[&str] = &["program.shift.right"];

fn root(id: &str) -> &str {
    shared_base(id).unwrap_or(id)
}

fn placed() -> Vec<(ControlEntry, Rect)> {
    all_controls()
        .into_iter()
        .map(|c| {
            let r = rect_for(&c.id).unwrap_or_else(|| panic!("no rect for {}", c.id));
            (c, r)
        })
        .collect()
}

#[test]
fn every_inventory_id_has_a_rect() {
    assert!(placed().len() > 250);
}

#[test]
fn unknown_id_has_no_rect() {
    assert!(rect_for("nope.nothing").is_none());
}

#[test]
fn rect_sizes_are_positive_and_finite() {
    for (c, r) in placed() {
        assert!(r.w > 0.0 && r.h > 0.0, "{}: {r:?}", c.id);
        assert!(r.x.is_finite() && r.y.is_finite(), "{}", c.id);
    }
}

#[test]
fn rects_lie_inside_their_section_and_the_panel() {
    let panel = panel_rect();
    for (c, r) in placed() {
        assert!(panel.contains(&r), "{} outside panel: {r:?}", c.id);
        if !OUTSIDE_OWN_SECTION.contains(&c.id.as_str()) {
            let s = section_rect(c.section);
            assert!(
                s.contains(&r),
                "{} outside {:?}: {r:?} vs {s:?}",
                c.id,
                c.section
            );
        }
    }
}

#[test]
fn section_rects_do_not_overlap_and_fit_the_panel() {
    use Section::*;
    let all = [Master, Organ, Piano, Program, Synth, Effects, Keybed];
    let panel = panel_rect();
    for (i, a) in all.iter().enumerate() {
        assert!(panel.contains(&section_rect(*a)), "{a:?}");
        for b in &all[i + 1..] {
            assert!(
                !section_rect(*a).overlaps(&section_rect(*b)),
                "{a:?} overlaps {b:?}"
            );
        }
    }
}

#[test]
fn no_overlaps_except_shared_by_shift() {
    let p = placed();
    for (i, (a, ra)) in p.iter().enumerate() {
        for (b, rb) in &p[i + 1..] {
            if ra.overlaps(rb) {
                assert_eq!(
                    root(&a.id),
                    root(&b.id),
                    "{} overlaps {} but they do not share a base",
                    a.id,
                    b.id
                );
            }
        }
    }
}

#[test]
fn shared_ids_are_inventory_ids_with_own_rects_for_their_base() {
    let ids: Vec<String> = all_controls().into_iter().map(|c| c.id).collect();
    for (a, b) in SHARED {
        assert!(ids.iter().any(|i| i == a), "{a} not in inventory");
        assert!(ids.iter().any(|i| i == b), "{b} not in inventory");
        assert_eq!(rect_for(a), rect_for(b), "{a} vs {b}");
    }
}

#[test]
fn every_shift_and_hold_control_shares_a_base() {
    for c in all_controls() {
        if matches!(c.access, Access::Shift | Access::Hold) {
            assert!(shared_base(&c.id).is_some(), "{} has no shared base", c.id);
        }
    }
}

#[test]
fn estimated_ids_are_inventory_ids() {
    let ids: Vec<String> = all_controls().into_iter().map(|c| c.id).collect();
    for e in ESTIMATED {
        assert!(ids.iter().any(|i| i == e), "{e} not in inventory");
    }
}
