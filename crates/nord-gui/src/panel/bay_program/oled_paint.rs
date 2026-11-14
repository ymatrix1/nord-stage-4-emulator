//! Painter of the Program OLED text (scaled to the layout rectangle).

use super::menu_screen::DIM;
use super::oled::OledText;
use super::oled_home::EDITED;
use super::oled_icons::{self, is_icon};
use crate::panel::prelude::*;
use egui::{FontId, Pos2};

/// Draw the boxed `E` of an edited program with its top-left at `at`; returns the right edge.
fn boxed_e(painter: &Painter, at: Pos2, font: &FontId, color: Color32) -> f32 {
    let glyph = painter.text(
        at + vec2(1.0, 0.0),
        Align2::LEFT_TOP,
        "E",
        font.clone(),
        color,
    );
    let frame = glyph.expand2(vec2(1.0, 0.0));
    painter.rect_stroke(frame, 0.0, Stroke::new(0.7_f32, color), StrokeKind::Inside);
    frame.right() + 2.0
}

/// Row geometry of the real Program home screen on its 128 x 64 pixel grid.
///
/// VERIFIED (frames sent by the ARM emulator, `tools/arm_screens.py`, measured in
/// `docs/research/arm-oled-render.md` section 9): the location title and the name line are both
/// 13 px proportional text at the left edge, text boxes at y = 3 and y = 18 (15 px pitch); a 1 px
/// rule at y = 33 spans the full width; the section rows below use the 8 px font from y = 36 every
/// 10 px. The sizes here are those pixels divided by 64, so they scale with the rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Top of the title text box.
    pub title_y: f32,
    /// Top of the name line text box.
    pub name_y: f32,
    /// The full-width rule between the header and the rows.
    pub rule_y: f32,
    /// Top of the first row.
    pub rows_y: f32,
    /// Distance between rows.
    pub row_pitch: f32,
}

/// Real pixel rows of [`Grid`] for a display `height` (in the unit of the caller).
pub fn grid(height: f32) -> Grid {
    let k = height / 64.0;
    Grid {
        title_y: 3.0 * k,
        name_y: 18.0 * k,
        rule_y: 33.0 * k,
        rows_y: 36.0 * k,
        row_pitch: 10.0 * k,
    }
}

/// Rows of the other layouts on the 128 x 64 grid.
///
/// VERIFIED (frames of `tools/arm_screens.py`, `docs/research/arm-oled-render.md` sections 9 and
/// 12): title-bar screens (menus, Preset, Layer Init, Morph assign, Section Edit) light rows 0 to
/// 10 and print the title unlit at x = 1, the page counter right-aligned; the label starts at
/// y = 15 and the value at y = 26; soft buttons are outlined boxes in rows 53 to 63 and the active
/// one is lit; parameter popups (Transpose, Master Clock Tempo, Nord `Nothing to Paste`) put label
/// and value at y = 45 and y = 55 under the home header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// Height of the inverted title bar.
    pub bar_h: f32,
    /// Top of the label under the bar.
    pub label_y: f32,
    /// Top of the first body row (the value).
    pub value_y: f32,
    /// Top of the soft buttons.
    pub soft_y: f32,
    /// Tops of the popup label and value.
    pub popup_y: [f32; 2],
}

/// Real pixel rows of [`Bar`] for a display `height`.
pub fn bar(height: f32) -> Bar {
    let k = height / 64.0;
    Bar {
        bar_h: 11.0 * k,
        label_y: 15.0 * k,
        value_y: 26.0 * k,
        soft_y: 53.0 * k,
        popup_y: [45.0 * k, 55.0 * k],
    }
}

/// Paint the display text into `rect`, scaling the type to the rectangle.
pub fn paint(painter: &Painter, rect: Rect, text: &OledText, dim: bool) {
    let fg = if dim {
        Color32::from_rgb(80, 100, 120)
    } else {
        Color32::from_rgb(150, 190, 230)
    };
    painter.rect_filled(rect.expand(1.5), 2.0, Color32::from_rgb(10, 11, 13));
    painter.rect_filled(rect, 1.5, OLED_BG);
    let h = rect.height();
    // The name line is the same 13 px font as the title on the real unit (not a smaller one).
    let (f_big, f_sub, f_row) = (
        (h * 0.19).max(5.0),
        (h * 0.19).max(5.0),
        (h * 0.12).max(4.0),
    );
    let (g, b) = (grid(h), bar(h));
    let line = |y: f32, s: &str, size: f32| {
        // A row that starts with the DIM mark is an unsupported menu item: drawn faded.
        let (s, color) = match s.strip_prefix(DIM) {
            Some(rest) => (rest, fg.gamma_multiply(0.45)),
            None => (s, fg),
        };
        let font = FontId::monospace(size.max(MIN_FONT_PX));
        let mut x = rect.left() + 2.0;
        // A section row starts with an icon in the 8 px column; its text begins at x = 12.
        let (icon, s) = match s.chars().next() {
            Some(c) if is_icon(c) => (Some(c), &s[c.len_utf8()..]),
            _ => (None, s),
        };
        if let Some(c) = icon {
            let side = h * 8.0 / 64.0;
            let cell = Rect::from_min_size(pos2(rect.left() + 0.5, y + 0.5), vec2(side, side));
            oled_icons::paint(painter, cell, c, color);
            x = rect.left() + rect.width() * oled_icons::TEXT_X;
        }
        let mut segments = s.split(EDITED);
        let first = segments.next().unwrap_or_default();
        x = painter
            .text(pos2(x, y), Align2::LEFT_TOP, first, font.clone(), color)
            .right();
        for rest in segments {
            x = boxed_e(painter, pos2(x + 1.0, y), &font, color);
            x = painter
                .text(pos2(x, y), Align2::LEFT_TOP, rest, font.clone(), color)
                .right();
        }
    };
    let top = rect.top();
    if let Some(counter) = &text.bar {
        // Title bar: lit rows 0 to 10, unlit title, right-aligned counter.
        let strip = Rect::from_min_size(rect.min, vec2(rect.width(), b.bar_h));
        painter.rect_filled(strip, 1.5, fg);
        let font = FontId::monospace(f_row.max(MIN_FONT_PX));
        painter.text(
            pos2(rect.left() + 2.0, top + 1.0),
            Align2::LEFT_TOP,
            &text.big,
            font.clone(),
            OLED_BG,
        );
        painter.text(
            pos2(rect.right() - 2.0, top + 1.0),
            Align2::RIGHT_TOP,
            counter,
            font,
            OLED_BG,
        );
        line(top + b.label_y, &text.sub, f_row);
        for (i, r) in text.rows.iter().enumerate() {
            let y = top + b.value_y + i as f32 * g.row_pitch;
            if y + f_row > rect.bottom() - b.bar_h {
                break;
            }
            // A lone value (menu page with one item) is drawn in the 13 px font.
            line(
                y,
                r,
                if text.body_big && i == 0 {
                    f_big
                } else {
                    f_row
                },
            );
        }
    } else {
        line(top + g.title_y, &text.big, f_big);
        line(top + g.name_y, &text.sub, f_sub);
        painter.line_segment(
            [
                pos2(rect.left() + 1.0, top + g.rule_y),
                pos2(rect.right() - 1.0, top + g.rule_y),
            ],
            Stroke::new(0.5_f32, fg),
        );
        let soft_top = top + b.soft_y;
        if let Some((label, value)) = &text.popup {
            line(top + b.popup_y[0], label, f_row);
            line(top + b.popup_y[1], value, f_row);
        }
        for (i, r) in text.rows.iter().enumerate() {
            let y = top + g.rows_y + i as f32 * g.row_pitch;
            if y + f_row > soft_top && text.soft.iter().any(|l| !l.is_empty()) {
                break;
            }
            if y + f_row > rect.bottom() {
                break;
            }
            line(y, r, f_row);
        }
    }
    soft_buttons(painter, rect, text, fg, f_row);
}

/// Outlined soft-button boxes in rows 53 to 63; the active one is lit with unlit text.
fn soft_buttons(painter: &Painter, rect: Rect, text: &OledText, fg: Color32, size: f32) {
    if text.soft.iter().all(|l| l.is_empty()) {
        return;
    }
    let b = bar(rect.height());
    let w = rect.width() / 4.0;
    for (i, l) in text.soft.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let r = Rect::from_min_size(
            pos2(rect.left() + w * i as f32 + 0.5, rect.top() + b.soft_y),
            vec2(w - 1.0, rect.bottom() - rect.top() - b.soft_y - 0.5),
        );
        let on = text.soft_on == Some(i);
        if on {
            painter.rect_filled(r, 1.5, fg);
        }
        painter.rect_stroke(r, 1.5, Stroke::new(0.7_f32, fg), StrokeKind::Inside);
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            *l,
            FontId::monospace(size.max(MIN_FONT_PX)),
            if on { OLED_BG } else { fg },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn grid_is_the_measured_pixel_rows_of_the_real_home_screen() {
        let g = grid(64.0);
        assert_eq!(
            (g.title_y, g.name_y, g.rule_y, g.rows_y, g.row_pitch),
            (3.0, 18.0, 33.0, 36.0, 10.0)
        );
        // The title and the name line are one 15 px pitch apart; the rule is two pitches down.
        assert_eq!(g.name_y - g.title_y, 15.0);
        assert_eq!(g.rule_y - g.title_y, 30.0);
        // It scales with the display height.
        let half = grid(32.0);
        assert_eq!((half.name_y, half.rule_y), (9.0, 16.5));
    }

    #[test]
    fn the_edited_mark_adds_a_box_and_the_e() {
        let paint_big = |big: &str| {
            shapes_painted(|_ui, p, rect, _s, _a| {
                let t = OledText {
                    big: big.into(),
                    ..Default::default()
                };
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(80.0, 40.0)),
                    &t,
                    false,
                );
            })
        };
        let plain = paint_big("A:11");
        let edited = paint_big(&format!("A:11{EDITED}"));
        assert!(edited >= plain + 2, "box and E: {plain} -> {edited}");
    }

    #[test]
    fn painter_draws_text_rows_and_only_the_labelled_soft_buttons() {
        let paint_with = |soft: [&'static str; 4]| {
            shapes_painted(|_ui, p, rect, _s, _a| {
                let t = OledText {
                    big: "A".into(),
                    sub: "b".into(),
                    rows: vec!["row".into()],
                    soft,
                    ..Default::default()
                };
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(80.0, 40.0)),
                    &t,
                    false,
                );
            })
        };
        let none = paint_with(["", "", "", ""]);
        let two = paint_with(["a", "", "c", ""]);
        assert!(none >= 5, "painted {none}");
        assert!(two > none, "soft buttons add shapes");
    }

    #[test]
    fn a_lone_menu_value_paints_like_any_body_row() {
        let paint_with = |body_big: bool| {
            shapes_painted(|_ui, p, rect, _s, _a| {
                let t = OledText {
                    big: "SYSTEM SETTINGS".into(),
                    sub: "Memory Protect".into(),
                    rows: vec!["On".into()],
                    bar: Some("1/6".into()),
                    body_big,
                    ..Default::default()
                };
                paint(
                    p,
                    Rect::from_min_size(rect.min, vec2(80.0, 40.0)),
                    &t,
                    false,
                );
            })
        };
        // The same shapes either way: only the type size of the value row differs.
        assert_eq!(paint_with(true), paint_with(false));
        assert!(paint_with(true) >= 5);
    }

    #[test]
    fn bar_rows_are_the_measured_pixel_rows_of_the_real_title_bar_screens() {
        let b = bar(64.0);
        assert_eq!(
            (b.bar_h, b.label_y, b.value_y, b.soft_y, b.popup_y),
            (11.0, 15.0, 26.0, 53.0, [45.0, 55.0])
        );
        let half = bar(32.0);
        assert_eq!(
            (half.bar_h, half.soft_y, half.popup_y[1]),
            (5.5, 26.5, 27.5)
        );
    }

    fn painted(t: &OledText) -> usize {
        shapes_painted(|_ui, p, rect, _s, _a| {
            paint(
                p,
                Rect::from_min_size(rect.min, vec2(128.0, 64.0)),
                t,
                false,
            );
        })
    }

    #[test]
    fn a_title_bar_adds_the_inverted_strip_and_the_counter() {
        let plain = OledText {
            big: "SYSTEM SETTINGS".into(),
            ..Default::default()
        };
        let bar_only = OledText {
            bar: Some(String::new()),
            ..plain.clone()
        };
        let counted = OledText {
            bar: Some("1/6".into()),
            ..plain.clone()
        };
        // The strip is one more shape; the counter text is painted too.
        assert!(painted(&bar_only) > painted(&plain));
        assert!(painted(&counted) >= painted(&bar_only));
    }

    #[test]
    fn a_popup_adds_its_label_and_value_and_the_lit_soft_button_a_fill() {
        let base = OledText {
            big: "A:11".into(),
            sub: "Name".into(),
            ..Default::default()
        };
        let popup = OledText {
            popup: Some(("Transpose".into(), "+1 semi".into())),
            ..base.clone()
        };
        assert!(painted(&popup) >= painted(&base) + 2);
        let soft = OledText {
            soft: ["Num", "Cat", "", "Cancel"],
            ..base.clone()
        };
        let lit = OledText {
            soft_on: Some(1),
            ..soft.clone()
        };
        assert!(
            painted(&lit) > painted(&soft),
            "the active button is filled"
        );
    }

    #[test]
    fn section_row_icons_are_drawn_as_shapes_and_text_starts_after_them() {
        let with_icon = OledText {
            rows: vec![format!("{}B3", super::oled_icons::ICON_ORGAN)],
            ..Default::default()
        };
        let without = OledText {
            rows: vec!["B3".into()],
            ..Default::default()
        };
        assert!(painted(&with_icon) > painted(&without));
    }
}
