//! Headless legibility checks of the whole panel (B12).
//!
//! One frame of the real panel is run through `egui::Context::run`; every painted
//! `Shape::Text` becomes a label rectangle (its galley rect). The checks:
//!
//! * every label is at least [`MIN_FONT_PX`] tall,
//! * two labels do not overlap (ink rectangles, 1 px tolerance),
//! * a label does not overlap a control rectangle it does not belong to. A label belongs to the
//!   control whose rectangle contains the label's centre (text printed on a button or OLED);
//!   rectangles larger than [`BIG_RECT_PX2`] (OLEDs, groups, keys) are not checked.
//!
//! Remaining violations are listed in `legibility_allow.txt`; the test fails on any new one and
//! on any stale entry, so the list can only shrink. Run with `NS4_WRITE_COLLISIONS=<file>` to
//! write the current list instead of asserting (used to regenerate the allow-list).

use super::prelude::*;
use super::view::{self, FOCUSABLE, PANEL_SIZE};
use super::{render_panel_at, render_stage4_panel};
use egui::epaint::Shape;
use std::collections::BTreeSet;

/// Smallest readable label height in px at zoom 1 (`widgets::MIN_FONT_PX`).
const MIN_FONT_PX: f32 = crate::widgets::MIN_FONT_PX;
/// Control rectangles above this area are containers, not checked for foreign labels.
const BIG_RECT_PX2: f32 = 6000.0;
/// Overlap (px) in both axes needed to count as a collision.
const TOL: f32 = 1.0;
/// Button bezel and LED ring around a control rectangle: labels keep out of it too.
const BEZEL_PX: f32 = 1.5;
const ALLOW: &str = include_str!("legibility_allow.txt");

/// One painted text.
#[derive(Clone, Debug)]
pub(super) struct Lab {
    pub text: String,
    /// Ink rectangle (galley width, glyph-height band), in panel px.
    pub rect: Rect,
    /// Full galley rectangle in panel px (what the clip rectangle must contain).
    pub full: Rect,
    /// Clip rectangle the text was painted with, in panel px.
    pub clip: Rect,
    pub size: f32,
}

fn collect(shape: &Shape, clip: Rect, origin: egui::Pos2, out: &mut Vec<Lab>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, clip, origin, out)),
        Shape::Text(t) => {
            let size = t
                .galley
                .job
                .sections
                .first()
                .map_or(0.0, |s| s.format.font_id.size);
            let g = t.galley.rect.translate(t.pos.to_vec2());
            let band = size * 1.0;
            let ink =
                Rect::from_center_size(g.center(), vec2(g.width() + 2.0, band.min(g.height())));
            out.push(Lab {
                text: t.galley.text().to_string(),
                rect: ink.translate(-origin.to_vec2()),
                full: g.translate(-origin.to_vec2()),
                clip: clip.translate(-origin.to_vec2()),
                size,
            });
        }
        _ => {}
    }
}

/// Labels painted by `body` in one headless frame (panel origin at 0,0 in the result).
pub(super) fn labels_of(
    focus: Option<nord_panel::Section>,
    screen: egui::Vec2,
) -> (Vec<Lab>, Rect) {
    let a = audit_of(focus, screen);
    (a.labs, a.visible)
}

/// Labels, visible rectangle and recorded widget rectangles of one headless frame.
pub(super) struct Audit {
    pub labs: Vec<Lab>,
    pub visible: Rect,
    pub recs: Vec<crate::widgets::probe::Rec>,
}

pub(super) fn audit_of(focus: Option<nord_panel::Section>, screen: egui::Vec2) -> Audit {
    let ctx = egui::Context::default();
    let (audio, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    state.view.focus = focus;
    let mut origin = egui::Pos2::ZERO;
    let mut visible = Rect::NOTHING;
    let mut out = None;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, screen)),
            ..Default::default()
        };
        out = Some(ctx.run(input, |ctx| {
            crate::widgets::probe::clear(ctx);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    if focus.is_some() {
                        render_stage4_panel(ui, &mut state, &audio);
                        visible = ui.min_rect();
                        origin = view::panel_origin(focus, visible.min);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(PANEL_SIZE, Sense::hover());
                        let p = ui.painter().clone();
                        render_panel_at(ui, &p, rect, &mut state, &audio);
                        origin = rect.min;
                        visible = rect;
                    }
                });
        }));
    }
    let mut labs = Vec::new();
    for cs in out.map(|o| o.shapes).unwrap_or_default() {
        collect(&cs.shape, cs.clip_rect, origin, &mut labs);
    }
    let recs = crate::widgets::probe::recorded(&ctx)
        .into_iter()
        .map(|mut r| {
            r.rect = r.rect.translate(-origin.to_vec2());
            r
        })
        .collect();
    Audit {
        labs,
        visible: visible.translate(-origin.to_vec2()),
        recs,
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > TOL && i.height() > TOL
}

fn pos_key(l: &Lab) -> String {
    format!("{}@{:.0},{:.0}", l.text, l.rect.min.x, l.rect.min.y)
}

/// Every violation of the overview panel as a stable one-line key.
pub(super) fn violations(labs: &[Lab]) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    for (i, a) in labs.iter().enumerate() {
        for b in &labs[i + 1..] {
            if overlaps(a.rect, b.rect) {
                let (mut ka, mut kb) = (pos_key(a), pos_key(b));
                if ka > kb {
                    std::mem::swap(&mut ka, &mut kb);
                }
                v.insert(format!("L {ka} / {kb}"));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for c in nord_panel::all_controls() {
        let Some(r) = super::layout_px::rect_of(&c.id, egui::Pos2::ZERO) else {
            continue;
        };
        let key = (
            r.min.x as i32,
            r.min.y as i32,
            r.max.x as i32,
            r.max.y as i32,
        );
        if r.area() > BIG_RECT_PX2 || !seen.insert(key) {
            continue;
        }
        for l in labs {
            if !r.contains(l.rect.center()) && overlaps(l.rect, r.expand(BEZEL_PX)) {
                v.insert(format!("C {} / {}", pos_key(l), c.id));
            }
        }
    }
    v
}

fn allowed() -> BTreeSet<String> {
    ALLOW
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn collisions_match_the_allow_list() {
    let audit = audit_of(None, vec2(3300.0, 900.0));
    let mut found = violations(&audit.labs);
    found.extend(super::legibility_rules::extra_violations(&audit));
    let labs = audit.labs;
    println!(
        "legibility: {} labels, {} collisions",
        labs.len(),
        found.len()
    );
    if let Ok(path) = std::env::var("NS4_WRITE_COLLISIONS") {
        let body: Vec<&str> = found.iter().map(String::as_str).collect();
        std::fs::write(path, body.join("\n") + "\n").unwrap();
        return;
    }
    let allow = allowed();
    let new: Vec<_> = found.difference(&allow).collect();
    let stale: Vec<_> = allow.difference(&found).collect();
    assert!(new.is_empty(), "new collisions:\n{}", join(&new));
    assert!(
        stale.is_empty(),
        "fixed, remove from allow-list:\n{}",
        join(&stale)
    );
}

fn join(v: &[&String]) -> String {
    v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
}

#[test]
fn every_label_is_at_least_the_minimum_font() {
    let (labs, _) = labels_of(None, vec2(1700.0, 500.0));
    assert!(labs.len() > 200, "only {} labels", labs.len());
    let small: Vec<_> = labs
        .iter()
        .filter(|l| l.size < MIN_FONT_PX - 1e-3)
        .map(|l| format!("{} ({})", l.text, l.size))
        .collect();
    assert!(small.is_empty(), "labels below {MIN_FONT_PX} px: {small:?}");
}

#[test]
fn every_section_draws_its_title_and_labels_when_focused() {
    for s in FOCUSABLE {
        let (labs, visible) = labels_of(Some(s), vec2(1700.0, 900.0));
        let sec = view::section_px(s, egui::Pos2::ZERO);
        let inside = labs
            .iter()
            .filter(|l| sec.contains(l.rect.center()))
            .count();
        assert!(inside > 15, "{s:?}: only {inside} labels in the section");
        assert!(visible.width() > 100.0, "{s:?} visible {visible:?}");
    }
}

#[test]
fn audit_records_frames_knobs_and_clip_rects() {
    use crate::widgets::probe::Kind;
    let a = audit_of(None, vec2(3300.0, 900.0));
    assert!(a.recs.iter().any(|r| r.kind == Kind::GroupInside));
    assert!(a.recs.iter().any(|r| r.kind == Kind::GroupBreak));
    assert!(a.recs.iter().filter(|r| r.kind == Kind::Knob).count() > 20);
    assert!(a.labs.iter().all(|l| l.clip.is_positive()));
    // Every label of the rotary box sits inside its frame (rule G of `legibility_rules`).
    let frame = a
        .recs
        .iter()
        .find(|r| r.kind == Kind::GroupInside)
        .map(|r| r.rect)
        .unwrap();
    for t in [
        "ROTARY SPEAKER",
        "DRIVE",
        "CLOSE MIC",
        "SLOW/STOP - FAST",
        "FAST",
    ] {
        let l = a
            .labs
            .iter()
            .find(|l| l.text == t)
            .unwrap_or_else(|| panic!("{t}"));
        assert!(
            frame.contains_rect(l.full),
            "{t} {:?} outside {frame:?}",
            l.full
        );
    }
}

#[test]
fn allow_list_parser_skips_comments_and_blanks() {
    assert!(allowed()
        .iter()
        .all(|l| !l.starts_with('#') && !l.is_empty()));
}
