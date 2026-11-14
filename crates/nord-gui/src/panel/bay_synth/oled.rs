//! Paints the Synth OLED: bezel, header, graphic area and the three soft labels above the dials.

use super::graphics::{paint_graphic, FG};
use super::oled_view::view_of;
use super::widgets::DIAL_IDS;
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::SynthUi;
use egui::{FontId, Pos2};

/// Rectangle of the glass: the layout rectangle of `synth.oled` plus the strip above the dials
/// that holds the soft labels (the photo's OLED is only 70 px wide at this scale).
pub(super) fn glass_rect(o: Pos2) -> Rect {
    let r = rect_or_empty("synth.oled", o);
    Rect::from_min_max(r.min - vec2(6.0, 2.0), r.max + vec2(6.0, 15.0))
}

/// Draw the display for the current page.
pub(super) fn paint_oled(p: &Painter, o: Pos2, sui: &mut SynthUi, sample: Option<&str>) {
    sui.mark("synth.oled");
    let disp = glass_rect(o);
    p.rect_filled(disp.expand(2.5), 3.0, Color32::from_rgb(10, 11, 13));
    p.rect_filled(disp, 2.0, OLED_BG);
    let p = p.with_clip_rect(disp);
    let v = view_of(sui, sample);
    p.text(
        disp.left_top() + vec2(2.0, 1.5),
        Align2::LEFT_TOP,
        v.title,
        FontId::monospace(6.5),
        FG,
    );
    p.text(
        pos2(disp.right() - 2.0, disp.top() + 1.5),
        Align2::RIGHT_TOP,
        v.layer.to_string(),
        FontId::monospace(6.5),
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
        pos2(disp.right() - 2.0, disp.bottom() - 15.0),
    );
    paint_graphic(&p, area, &v.graphic);
    for (i, s) in v.soft.iter().enumerate() {
        let Some(s) = s else { continue };
        let x = rect_or_empty(DIAL_IDS[i], o).center().x;
        let col = if s.dim { FG.gamma_multiply(0.4) } else { FG };
        p.text(
            pos2(x, disp.bottom() - 13.0),
            Align2::CENTER_TOP,
            &s.value,
            FontId::monospace(6.0),
            col,
        );
        p.text(
            pos2(x, disp.bottom() - 6.0),
            Align2::CENTER_TOP,
            s.label,
            FontId::monospace(5.5),
            col.gamma_multiply(0.8),
        );
    }
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
            let n = shapes_painted(|_ui, p, rect, _s, _a| paint_oled(p, rect.min, &mut sui, None));
            assert!(n >= 5, "{page:?} painted {n} shapes");
        }
        assert!(sui.drawn().contains(&"synth.oled"));
    }
}
