//! Geometry rules of the legibility audit beyond plain label/label and label/control overlap
//! (see `legibility_tests.rs`): clipped text, labels crossing a section or group frame, labels
//! touching a neighbour, and labels printed into a knob's tick ring.

use super::legibility_tests::{Audit, Lab};
use crate::widgets::probe::Kind;
use egui::{pos2, Pos2, Rect};
use std::collections::BTreeSet;

/// Space (px) that must stay free between the ink rectangles of two labels.
pub(super) const GAP: f32 = 0.9;
/// Text may leave its clip rectangle by this much (anti-aliasing).
const CLIP_TOL: f32 = 0.6;

fn key(l: &Lab) -> String {
    format!("{}@{:.0},{:.0}", l.text, l.rect.min.x, l.rect.min.y)
}

fn inside(outer: Rect, inner: Rect, tol: f32) -> bool {
    outer.expand(tol).contains_rect(inner)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 1.0 && i.height() > 1.0
}

/// Labels whose rectangle lies (partly) outside the clip rectangle they were painted with.
fn clipped(labs: &[Lab], out: &mut BTreeSet<String>) {
    for l in labs {
        if !inside(l.clip, l.full, CLIP_TOL) {
            out.insert(format!("K {}", key(l)));
        }
    }
}

/// Labels that cross the border of a section rectangle or a group frame.
fn frames(a: &Audit, out: &mut BTreeSet<String>) {
    use nord_panel::Section::{Effects, Master, Organ, Piano, Program, Synth};
    let secs: Vec<(String, Rect)> = [Master, Organ, Piano, Program, Synth, Effects]
        .into_iter()
        .map(|s| (format!("{s:?}"), super::view::section_px(s, Pos2::ZERO)))
        .collect();
    for l in &a.labs {
        for (name, r) in &secs {
            if overlaps(l.full, *r) && !inside(*r, l.full, 0.5) {
                out.insert(format!("S {name} {}", key(l)));
            }
        }
        for rec in &a.recs {
            let own_title = rec.kind == Kind::GroupBreak && rec.title == l.text;
            let frame = matches!(rec.kind, Kind::GroupBreak | Kind::GroupInside);
            if frame && !own_title && overlaps(l.full, rec.rect) && !inside(rec.rect, l.full, 0.5) {
                out.insert(format!("G {} {}", rec.title, key(l)));
            }
        }
    }
}

/// Does `label` touch the knob drawn in `knob` (a square of half-size radius + 5.5)? The tick
/// ring spans the top 270 degrees; the 90 degree gap below the knob only has the body and its
/// shadow (radius + 2), so a name printed there may come close.
fn hits_knob(knob: Rect, label: Rect) -> bool {
    let c = knob.center();
    let ring = knob.width() * 0.5;
    let q = pos2(
        c.x.clamp(label.min.x, label.max.x),
        c.y.clamp(label.min.y, label.max.y),
    );
    let d = q - c;
    let reach = if d.y > 0.0 && d.x.abs() < d.y {
        ring - 3.5
    } else {
        ring
    };
    d.length() < reach - 0.5
}

/// Labels printed into a knob's body or tick ring (the label of that knob sits outside it).
fn knobs(a: &Audit, out: &mut BTreeSet<String>) {
    for rec in a.recs.iter().filter(|r| r.kind == Kind::Knob) {
        for l in &a.labs {
            if hits_knob(rec.rect, l.rect) {
                out.insert(format!("N {} {}", rec.title, key(l)));
            }
        }
    }
}

/// Pairs of labels closer than [`GAP`] px (touching or overlapping).
fn touching(labs: &[Lab], out: &mut BTreeSet<String>) {
    for (i, a) in labs.iter().enumerate() {
        for b in &labs[i + 1..] {
            let i = a.rect.expand(GAP * 0.5).intersect(b.rect.expand(GAP * 0.5));
            if i.width() > 0.0 && i.height() > 0.0 {
                let (mut ka, mut kb) = (key(a), key(b));
                if ka > kb {
                    std::mem::swap(&mut ka, &mut kb);
                }
                out.insert(format!("L {ka} / {kb}"));
            }
        }
    }
}

/// All extra violations as stable one-line keys.
pub(super) fn extra_violations(a: &Audit) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    clipped(&a.labs, &mut v);
    frames(a, &mut v);
    knobs(a, &mut v);
    touching(&a.labs, &mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn lab(text: &str, x: f32, y: f32, clip: Rect) -> Lab {
        let r = Rect::from_min_size(pos2(x, y), vec2(30.0, 8.0));
        Lab {
            text: text.into(),
            rect: r,
            full: r,
            clip,
            size: 8.0,
        }
    }

    #[test]
    fn a_label_leaving_its_clip_rect_is_reported() {
        let clip = Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 20.0));
        let mut v = BTreeSet::new();
        clipped(&[lab("A", 0.0, 0.0, clip)], &mut v);
        assert_eq!(v.len(), 1);
        v.clear();
        let big = Rect::from_min_size(pos2(-5.0, -5.0), vec2(100.0, 100.0));
        clipped(&[lab("A", 0.0, 0.0, big)], &mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn knob_ring_is_open_below() {
        let knob = Rect::from_center_size(pos2(0.0, 0.0), vec2(40.0, 40.0));
        let below = Rect::from_min_size(pos2(-10.0, 18.0), vec2(20.0, 8.0));
        let above = Rect::from_min_size(pos2(-10.0, -22.0), vec2(20.0, 8.0));
        assert!(!hits_knob(knob, below));
        assert!(hits_knob(knob, above));
    }

    #[test]
    fn touching_labels_are_reported_but_spaced_ones_are_not() {
        let c = Rect::EVERYTHING;
        let mut v = BTreeSet::new();
        touching(&[lab("A", 0.0, 0.0, c), lab("B", 0.0, 8.5, c)], &mut v);
        assert_eq!(v.len(), 1);
        v.clear();
        touching(&[lab("A", 0.0, 0.0, c), lab("B", 0.0, 9.0, c)], &mut v);
        assert!(v.is_empty());
    }
}
