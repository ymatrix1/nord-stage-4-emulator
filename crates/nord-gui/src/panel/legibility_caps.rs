//! Second set of audit rules (B8 label legibility, round 2): fader caps, LEDs and stacked legends.
//!
//! | Rule | Meaning |
//! |---|---|
//! | `F` | a label overlaps the travel of a fader cap (the cap hides it at one end of its stroke) |
//! | `E` | an LED sits closer than [`LED_EDGE`] px to a group frame edge (or on it) |
//! | `T` | a label comes closer than [`LED_TEXT`] px to an LED bezel |
//! | `M` | a knob's caption is more than [`KNOB_CAPTION_MAX`] px from its tick ring (or missing) |
//! | `V` | two labels printed one above the other have less than [`STACK_GAP`] px between them |

use super::legibility_tests::{Audit, Lab};
use crate::widgets::probe::Kind;
use egui::Rect;
use std::collections::BTreeSet;

/// Free space (px) between an LED bezel and the group frame it sits in.
pub(super) const LED_EDGE: f32 = 4.0;
/// Free space (px) between a label's ink and an LED bezel.
pub(super) const LED_TEXT: f32 = 0.6;
/// Free space (px) between the ink rectangles of two stacked labels.
pub(super) const STACK_GAP: f32 = 2.0;
/// Farthest a knob's caption may sit from its tick ring (px).
pub(super) const KNOB_CAPTION_MAX: f32 = 12.0;

fn key(l: &Lab) -> String {
    format!("{}@{:.0},{:.0}", l.text, l.rect.min.x, l.rect.min.y)
}

fn hit(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.0 && i.height() > 0.0
}

/// Fader caps: no label may print into the area a cap sweeps over.
fn caps(a: &Audit, out: &mut BTreeSet<String>) {
    for c in a.recs.iter().filter(|r| r.kind == Kind::CapTravel) {
        for l in &a.labs {
            if hit(l.rect, c.rect) {
                out.insert(format!("F {} {}", c.title, key(l)));
            }
        }
    }
}

/// LEDs hugging (or crossing) a group frame.
fn led_edges(a: &Audit, out: &mut BTreeSet<String>) {
    let frames = a
        .recs
        .iter()
        .filter(|r| matches!(r.kind, Kind::GroupBreak | Kind::GroupInside));
    for f in frames {
        for led in a.recs.iter().filter(|r| r.kind == Kind::Led) {
            let near = hit(led.rect.expand(LED_EDGE), f.rect);
            let clear = f.rect.shrink(LED_EDGE).contains_rect(led.rect);
            // LEDs entirely outside the frame's reach are not its business.
            let reach = f.rect.expand(LED_EDGE).contains_rect(led.rect);
            if near && !clear && reach {
                out.insert(format!(
                    "E {} led@{:.0},{:.0}",
                    f.title,
                    led.rect.center().x,
                    led.rect.center().y
                ));
            }
        }
    }
}

/// Labels touching an LED bezel.
fn led_text(a: &Audit, out: &mut BTreeSet<String>) {
    for led in a.recs.iter().filter(|r| r.kind == Kind::Led) {
        for l in &a.labs {
            if hit(l.rect.expand(LED_TEXT), led.rect) {
                out.insert(format!(
                    "T led@{:.0},{:.0} {}",
                    led.rect.center().x,
                    led.rect.center().y,
                    key(l)
                ));
            }
        }
    }
}

/// Stacked labels (sharing at least a third of the narrower width) with a hairline gap.
fn stacked(labs: &[Lab], out: &mut BTreeSet<String>) {
    for (i, a) in labs.iter().enumerate() {
        for b in &labs[i + 1..] {
            let shared = a.rect.max.x.min(b.rect.max.x) - a.rect.min.x.max(b.rect.min.x);
            let need = a.rect.width().min(b.rect.width()) / 3.0;
            if shared <= need {
                continue;
            }
            let gap = (b.rect.min.y - a.rect.max.y).max(a.rect.min.y - b.rect.max.y);
            if (0.0..STACK_GAP).contains(&gap) {
                let (mut ka, mut kb) = (key(a), key(b));
                if ka > kb {
                    std::mem::swap(&mut ka, &mut kb);
                }
                out.insert(format!("V {ka} / {kb}"));
            }
        }
    }
}

/// Knob captions: the nearest label under (or over) a knob, centred on it, must sit within
/// [`KNOB_CAPTION_MAX`] px of the tick ring, so the name reads as the knob's own. The panel
/// prints knob names about 3 px under the ring (photos, `docs/specs/visual-style-and-panel.md`).
fn knob_captions(a: &Audit, out: &mut BTreeSet<String>) {
    for k in a.recs.iter().filter(|r| r.kind == Kind::Knob) {
        let c = k.rect.center();
        let ring = k.rect.width() * 0.5;
        let near = |l: &&Lab| (l.rect.center().x - c.x).abs() < ring * 0.5;
        let below = a
            .labs
            .iter()
            .filter(near)
            .filter(|l| l.rect.min.y >= c.y)
            .map(|l| l.rect.min.y - (c.y + ring - 3.5))
            .fold(f32::MAX, f32::min);
        let above = a
            .labs
            .iter()
            .filter(near)
            .filter(|l| l.rect.max.y <= c.y)
            .map(|l| (c.y - ring) - l.rect.max.y)
            .fold(f32::MAX, f32::min);
        // A caption beside the knob (small trimmers) counts too.
        let side = a
            .labs
            .iter()
            .filter(|l| (l.rect.center().y - c.y).abs() < ring * 0.5)
            .map(|l| (l.rect.min.x - (c.x + ring)).max((c.x - ring) - l.rect.max.x))
            .fold(f32::MAX, f32::min);
        let gap = below.min(above).min(side).min(99.0);
        if gap > KNOB_CAPTION_MAX {
            out.insert(format!("M {} caption {gap:.0} px away", k.title));
        }
    }
}

/// All violations of this file's rules as stable one-line keys.
pub(super) fn violations(a: &Audit) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    caps(a, &mut v);
    led_edges(a, &mut v);
    led_text(a, &mut v);
    knob_captions(a, &mut v);
    // Text lines inside one display (OLED) have their own line pitch.
    let displays: Vec<Rect> = nord_panel::all_controls()
        .iter()
        .filter_map(|c| super::layout_px::rect_of(&c.id, egui::Pos2::ZERO))
        .filter(|r| r.area() > super::legibility_tests::BIG_RECT_PX2)
        .collect();
    let outside: Vec<Lab> = a
        .labs
        .iter()
        .filter(|l| !displays.iter().any(|d| d.contains(l.rect.center())))
        .cloned()
        .collect();
    stacked(&outside, &mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::probe::Rec;
    use egui::{pos2, vec2};

    fn lab(text: &str, x: f32, y: f32) -> Lab {
        let r = Rect::from_min_size(pos2(x, y), vec2(30.0, 8.0));
        Lab {
            text: text.into(),
            rect: r,
            full: r,
            clip: Rect::EVERYTHING,
            size: 8.0,
        }
    }

    fn rec(kind: Kind, rect: Rect, title: &str) -> Rec {
        Rec {
            kind,
            rect,
            title: title.into(),
        }
    }

    #[test]
    fn stacked_labels_need_a_gap_but_side_by_side_ones_do_not() {
        let mut v = BTreeSet::new();
        stacked(&[lab("A", 0.0, 0.0), lab("B", 0.0, 9.0)], &mut v);
        assert_eq!(v.len(), 1);
        v.clear();
        stacked(&[lab("A", 0.0, 0.0), lab("B", 0.0, 10.5)], &mut v);
        assert!(v.is_empty());
        stacked(&[lab("A", 0.0, 0.0), lab("B", 60.0, 9.0)], &mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn a_label_in_a_cap_travel_and_a_led_on_a_frame_edge_are_reported() {
        let sq = |c: egui::Pos2| Rect::from_center_size(c, vec2(6.0, 6.0));
        let a = Audit {
            labs: vec![lab("8'", 0.0, 0.0)],
            visible: Rect::EVERYTHING,
            recs: vec![
                rec(
                    Kind::CapTravel,
                    Rect::from_min_size(pos2(5.0, 4.0), vec2(5.0, 20.0)),
                    "d",
                ),
                rec(
                    Kind::GroupInside,
                    Rect::from_min_size(pos2(100.0, 0.0), vec2(50.0, 50.0)),
                    "box",
                ),
                rec(Kind::Led, sq(pos2(147.0, 20.0)), ""),
                rec(Kind::Led, sq(pos2(125.0, 20.0)), ""),
            ],
        };
        let v = violations(&a);
        assert!(v.iter().any(|k| k.starts_with("F d")), "{v:?}");
        assert_eq!(
            v.iter().filter(|k| k.starts_with("E box")).count(),
            1,
            "{v:?}"
        );
    }

    #[test]
    fn a_label_touching_a_led_is_reported() {
        let a = Audit {
            labs: vec![lab("SOLO", 0.0, 0.0)],
            visible: Rect::EVERYTHING,
            recs: vec![rec(
                Kind::Led,
                Rect::from_center_size(pos2(31.0, 4.0), vec2(6.0, 6.0)),
                "",
            )],
        };
        assert_eq!(violations(&a).len(), 1);
    }
}
