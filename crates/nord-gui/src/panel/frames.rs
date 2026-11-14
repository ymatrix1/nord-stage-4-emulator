//! Printed group frames and titles (`nord_panel::layout::frames`): `MOD 1`, `OSCILLATORS`,
//! `PIANO SELECT`, `ACOUSTICS`, ...
//!
//! A frame is the union of the (nudged) member rectangles grown by the table's pad, so a control
//! moved by `layout_tweaks` carries its frame along. Frames are painted before the controls of
//! their bay. Every box registers with the test-only widget probe, so the legibility audit's rule
//! `G` keeps every label either inside a frame or clear of it.

use super::layout_px::PX_PER_MM;
use super::prelude::*;
use nord_panel::layout::{frames_of, FrameDef, FrameKind, MM_PER_PX};
use nord_panel::Section;

/// Size of a frame title (the 7.5 px legend minimum plus one px: titles outrank legends).
const TITLE_PX: f32 = 8.5;
/// Height of the strip the title blanks out of the border.
const TITLE_BAND: f32 = 11.0;
/// Side room (px) the title keeps from the frame corners.
const TITLE_SIDE: f32 = 6.0;
/// Printed title colour (silkscreen white).
const TITLE_COL: Color32 = Color32::from_gray(238);

/// Colour of the panel behind a section (what a title breaks the border against).
fn backdrop(section: Section) -> Color32 {
    match section {
        Section::Program | Section::Master => PROGRAM_RED,
        _ => MODULE_SLATE,
    }
}

/// Pixel rectangle of `def` with panel origin `o`: member union (nudged) plus the pad.
pub(super) fn frame_px(def: &FrameDef, o: egui::Pos2) -> Rect {
    let mut r = Rect::NOTHING;
    for m in def.members {
        r = r.union(super::layout_px::rect_or_empty(m, o));
    }
    let [l, t, rt, b] = def.pad.map(|v| v * MM_PER_PX * PX_PER_MM);
    Rect::from_min_max(r.min - vec2(l, t), r.max + vec2(rt, b))
}

/// Paint every frame and title of `section`.
pub(super) fn paint_frames(p: &Painter, o: egui::Pos2, section: Section) {
    for def in frames_of(section) {
        let r = frame_px(def, o);
        match def.kind {
            FrameKind::Box => paint_box(p, r, def, backdrop(section)),
            FrameKind::Title => {
                label(
                    p,
                    pos2(r.center().x, r.top()),
                    def.title,
                    TITLE_PX,
                    TITLE_COL,
                    Align2::CENTER_CENTER,
                );
            }
        }
    }
}

/// Outline with the title breaking the top border.
fn paint_box(p: &Painter, r: Rect, def: &FrameDef, bg: Color32) {
    let title = def.title;
    #[cfg(test)]
    crate::widgets::probe::record(p.ctx(), crate::widgets::probe::Kind::GroupBreak, r, title);
    p.rect_stroke(
        r,
        5.0,
        Stroke::new(1.2_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    let w = (text_width(p, title, TITLE_PX) + 2.0 * TITLE_SIDE).min(r.width());
    let cx = (r.left() + def.title_x * r.width()).clamp(r.left() + w * 0.5, r.right() - w * 0.5);
    let band = Rect::from_center_size(pos2(cx, r.top()), vec2(w, TITLE_BAND));
    p.rect_filled(band, 0.0, bg);
    label(
        p,
        band.center(),
        title,
        TITLE_PX,
        TITLE_COL,
        Align2::CENTER_CENTER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;
    use nord_panel::layout::{frame_def, FRAMES};

    #[test]
    fn frames_paint_shapes_for_every_framed_section() {
        for s in [
            Section::Organ,
            Section::Piano,
            Section::Synth,
            Section::Effects,
            Section::Program,
        ] {
            let n = shapes_painted(|_ui, p, rect, _s, _a| paint_frames(p, rect.min, s));
            assert!(n > 1, "{s:?}: {n} shapes");
        }
    }

    #[test]
    fn pixel_frame_encloses_its_members() {
        let o = pos2(10.0, 20.0);
        for f in FRAMES {
            let r = frame_px(f, o);
            for m in f.members {
                let mr = super::super::layout_px::rect_or_empty(m, o);
                assert!(r.contains_rect(mr), "{}: {m}", f.id);
            }
        }
    }

    #[test]
    fn boxes_of_one_section_do_not_overlap_and_stay_in_the_section() {
        let o = pos2(0.0, 0.0);
        for (i, a) in FRAMES.iter().enumerate() {
            let ra = frame_px(a, o);
            let sec = super::super::view::section_px(a.section, o);
            assert!(
                sec.contains_rect(ra),
                "{} leaves {:?}: {ra:?}",
                a.id,
                a.section
            );
            for b in &FRAMES[i + 1..] {
                if a.section == b.section && a.kind == FrameKind::Box && b.kind == FrameKind::Box {
                    let rb = frame_px(b, o);
                    assert!(!ra.intersects(rb), "{} overlaps {}", a.id, b.id);
                }
            }
        }
    }

    #[test]
    fn pad_converts_photo_px_to_panel_px() {
        let f = frame_def("synth.frame.amp").unwrap();
        let o = pos2(0.0, 0.0);
        let m = super::super::layout_px::rect_or_empty("synth.amp.envelope", o);
        let r = frame_px(f, o);
        let k = MM_PER_PX * PX_PER_MM;
        assert!((m.left() - r.left() - f.pad[0] * k).abs() < 1e-3);
        assert!((r.bottom() - m.bottom() - f.pad[3] * k).abs() < 1e-3);
    }

    #[test]
    fn backdrop_of_the_program_block_is_red() {
        assert_ne!(backdrop(Section::Program), backdrop(Section::Synth));
    }
}
