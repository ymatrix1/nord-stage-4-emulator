//! Paints the Synth OLED: bezel, header, graphic area and the three soft labels above the dials.

use super::graphics::{paint_graphic, FG};
use super::oled_view::view_of;
use super::widgets::DIAL_IDS;
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::SynthUi;
use egui::{FontId, Pos2};

/// Height (px) of the soft-label strip below the layout rectangle: two monospace lines of the
/// minimum font (about 9.5 px each) and a margin.
const SOFT_H: f32 = 24.0;

/// Rectangle of the glass: the layout rectangle of `synth.oled` plus the strip above the dials
/// that holds the soft labels (the photo's OLED is only 70 px wide at this scale).
pub(super) fn glass_rect(o: Pos2) -> Rect {
    let r = rect_or_empty("synth.oled", o);
    Rect::from_min_max(r.min - vec2(6.0, 2.0), r.max + vec2(6.0, SOFT_H))
}

/// Draw the display for the current page.
pub(super) fn paint_oled(
    p: &Painter,
    o: Pos2,
    sui: &mut SynthUi,
    sample: Option<&str>,
    hint: Option<&str>,
) {
    sui.mark("synth.oled");
    let disp = glass_rect(o);
    p.rect_filled(disp.expand(2.5), 3.0, Color32::from_rgb(10, 11, 13));
    p.rect_filled(disp, 2.0, OLED_BG);
    let p = p.with_clip_rect(disp);
    let v = view_of(sui, sample);
    // The title never runs under the layer letter at the right edge.
    let title_clip = Rect::from_min_max(disp.min, pos2(disp.right() - 9.0, disp.top() + 12.0));
    p.with_clip_rect(title_clip.intersect(disp)).text(
        disp.left_top() + vec2(2.0, 1.5),
        Align2::LEFT_TOP,
        // A moving knob replaces the page title with its hint line (`LFO Rate 0.08 Hz`).
        hint.unwrap_or(v.title),
        FontId::monospace(crate::widgets::MIN_FONT_PX),
        FG,
    );
    p.text(
        pos2(disp.right() - 2.0, disp.top() + 1.5),
        Align2::RIGHT_TOP,
        v.layer.to_string(),
        FontId::monospace(crate::widgets::MIN_FONT_PX),
        FG,
    );
    let sep_y = disp.top() + 9.5;
    p.line_segment(
        [
            pos2(disp.left() + 2.0, sep_y),
            pos2(disp.right() - 2.0, sep_y),
        ],
        Stroke::new(0.8_f32, FG.gamma_multiply(0.6)),
    );
    let area = Rect::from_min_max(
        pos2(disp.left() + 2.0, sep_y + 1.5),
        pos2(disp.right() - 2.0, disp.bottom() - SOFT_H),
    );
    paint_graphic(&p, area, &v.graphic);
    for (i, s) in v.soft.iter().enumerate() {
        let Some(s) = s else { continue };
        let x = rect_or_empty(DIAL_IDS[i], o).center().x;
        let col = if s.dim { FG.gamma_multiply(0.4) } else { FG };
        soft_text(&p, disp, x, disp.bottom() - 21.5, &s.value, col);
        // VERIFIED (research note s16.4): the labels are small inverted tabs, not outlined boxes.
        let tab_col = col.gamma_multiply(0.8);
        let w = p
            .layout_no_wrap(
                s.label.to_string(),
                FontId::monospace(crate::widgets::MIN_FONT_PX),
                tab_col,
            )
            .rect
            .width();
        let cx = soft_x(disp, x, w);
        let tab = Rect::from_center_size(pos2(cx, disp.bottom() - 6.0), vec2(w + 3.0, 10.5));
        p.rect_filled(tab, 1.0, tab_col);
        p.text(
            pos2(cx, disp.bottom() - 11.0),
            Align2::CENTER_TOP,
            s.label,
            FontId::monospace(crate::widgets::MIN_FONT_PX),
            OLED_BG,
        );
    }
}

/// Centre x for text of width `w` wanted at `x`, shifted so it stays inside the glass.
fn soft_x(disp: Rect, x: f32, w: f32) -> f32 {
    let half = w * 0.5 + 1.0;
    x.clamp(
        disp.left() + half,
        (disp.right() - half).max(disp.left() + half),
    )
}

/// Soft-label text centred on `x`, shifted sideways so it stays inside the glass.
fn soft_text(p: &Painter, disp: Rect, x: f32, y: f32, text: &str, col: Color32) {
    let font = FontId::monospace(crate::widgets::MIN_FONT_PX);
    let w = p
        .layout_no_wrap(text.to_string(), font.clone(), col)
        .rect
        .width();
    p.text(
        pos2(soft_x(disp, x, w), y),
        Align2::CENTER_TOP,
        text,
        font,
        col,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;
    use crate::panel::ui_state::SynthPage;

    #[test]
    fn glass_covers_the_oled_rectangle() {
        let o = pos2(0.0, 0.0);
        assert!(glass_rect(o).contains_rect(rect_or_empty("synth.oled", o)));
        assert!(glass_rect(o).bottom() < rect_or_empty(DIAL_IDS[0], o).top());
    }

    #[test]
    fn soft_text_is_pulled_inside_the_glass() {
        let disp = Rect::from_min_size(pos2(100.0, 100.0), vec2(60.0, 40.0));
        // A dial at the glass edge: a 27 px wide name must not stick out.
        assert!((soft_x(disp, 100.0, 27.0) - 114.5).abs() < 1e-3);
        assert!((soft_x(disp, 170.0, 27.0) - 145.5).abs() < 1e-3);
        assert_eq!(soft_x(disp, 130.0, 27.0), 130.0);
    }

    #[test]
    fn oled_paints_every_page_and_marks_the_display() {
        let mut sui = SynthUi::new();
        for page in [
            SynthPage::Waveform,
            SynthPage::FilterType,
            SynthPage::EnvOsc,
            SynthPage::EnvFilter,
            SynthPage::EnvAmp,
            SynthPage::LfoWave,
            SynthPage::Arp(1),
            SynthPage::Arp(3),
            SynthPage::Vibrato,
            SynthPage::PitchSmp,
            SynthPage::Extern,
        ] {
            sui.page = page;
            let n = shapes_painted(|_ui, p, rect, _s, _a| {
                paint_oled(p, rect.min, &mut sui, None, None)
            });
            assert!(n >= 5, "{page:?} painted {n} shapes");
        }
        assert!(sui.drawn().contains(&"synth.oled"));
    }
}
