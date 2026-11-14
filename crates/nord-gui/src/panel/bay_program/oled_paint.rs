//! Painter of the Program OLED text (scaled to the layout rectangle).

use super::menu_screen::DIM;
use super::oled::OledText;
use super::oled_grid::{bar, grid, Line};
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
        // A two-part value (`440.8Hz<TAB>+3 cent`) puts its second part at the right edge.
        let (s, right) = match s.split_once('\t') {
            Some((l, r)) => (l, Some(r)),
            None => (s, None),
        };
        if let Some(r) = right {
            let at = pos2(rect.right() - 2.0, y);
            painter.text(at, Align2::RIGHT_TOP, r, font.clone(), color);
        }
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
        // A list bar starts with the list icon in the 8 px column; the title begins at x = 12.
        let (icon, title) = match text.big.chars().next() {
            Some(c) if is_icon(c) => (Some(c), &text.big[c.len_utf8()..]),
            _ => (None, text.big.as_str()),
        };
        let mut title_x = rect.left() + 2.0;
        if let Some(c) = icon {
            let side = h * 8.0 / 64.0;
            let cell = Rect::from_min_size(pos2(rect.left() + 0.5, top + 1.0), vec2(side, side));
            oled_icons::paint(painter, cell, c, OLED_BG);
            title_x = rect.left() + rect.width() * oled_icons::TEXT_X;
        }
        painter.text(
            pos2(title_x, top + 1.0),
            Align2::LEFT_TOP,
            title,
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
        let k = h / 64.0;
        for l in &text.lines {
            let size = if l.big { f_big } else { f_row };
            free_line(painter, rect, l, k, size, fg);
        }
        if text.lines.is_empty() && (text.sel.is_none() || text.rows.is_empty()) {
            line(top + b.label_y, &text.sub, f_row);
        }
        if let Some(sel) = text.sel {
            // Program list: rows at y = 13 + 10 i, the selected one a lit block with unlit text.
            let font = FontId::monospace(f_row.max(MIN_FONT_PX));
            for (i, r) in text.rows.iter().enumerate() {
                let y = top + b.list_y + i as f32 * g.row_pitch;
                let color = if i == sel {
                    let block = Rect::from_min_size(
                        pos2(rect.left(), y - k),
                        vec2(rect.width(), g.row_pitch),
                    );
                    painter.rect_filled(block, 0.0, fg);
                    OLED_BG
                } else {
                    fg
                };
                painter.text(
                    pos2(rect.left() + 2.0, y),
                    Align2::LEFT_TOP,
                    r,
                    font.clone(),
                    color,
                );
            }
        }
        for (i, r) in text.rows.iter().enumerate().filter(|_| text.sel.is_none()) {
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
            // The value row keeps its second part (after a tab) at the right edge.
            line(top + b.popup_y[1], value, f_row);
        }
        if let Some(g) = text.graphic {
            super::oled_popup::paint(painter, rect, g, fg);
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

/// Paint a freely placed [`Line`]; its cursor cell is a lit block with the glyph unlit.
///
/// VERIFIED (ARM frame of `STORE PROGRAM AS`, research note s16.3): the cursor block is 9 px wide
/// and 16 px tall, from 2 px above to 13 px below the top of the glyph ink.
fn free_line(painter: &Painter, rect: Rect, l: &Line, k: f32, size: f32, fg: Color32) {
    let font = FontId::monospace(size.max(MIN_FONT_PX));
    let at = pos2(
        rect.left() + 0.5 + f32::from(l.x) * rect.width() / 128.0,
        rect.top() + f32::from(l.y) * k,
    );
    painter.text(at, Align2::LEFT_TOP, &l.text, font.clone(), fg);
    let Some(c) = l.cursor else { return };
    let width = |s: String| painter.layout_no_wrap(s, font.clone(), fg).size().x;
    let prefix: String = l.text.chars().take(c).collect();
    let glyph: String = l.text.chars().nth(c).unwrap_or(' ').to_string();
    let x = at.x + width(prefix);
    let cell = Rect::from_min_size(
        pos2(x - 0.5 * k, at.y - 2.0 * k),
        vec2(width(glyph.clone()) + 1.0 * k, 16.0 * k),
    );
    painter.rect_filled(cell, 0.0, fg);
    painter.text(pos2(x, at.y), Align2::LEFT_TOP, glyph, font, OLED_BG);
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
#[path = "oled_paint_tests.rs"]
mod tests;

#[cfg(test)]
mod unit {
    use super::*;
    use crate::panel::support::shapes_painted;

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
    fn a_program_list_lights_one_row_and_a_two_part_value_adds_a_right_text() {
        let rows: Vec<String> = ["A:11 a", "A:12 b", "A:13 c", "A:14 d"]
            .map(String::from)
            .to_vec();
        let lit = OledText {
            bar: Some(String::new()),
            big: "Program".into(),
            rows,
            sel: Some(0),
            ..Default::default()
        };
        let unlit = OledText {
            sel: None,
            ..lit.clone()
        };
        assert!(painted(&lit) >= painted(&unlit));
        let one = OledText {
            bar: Some(String::new()),
            big: "SYSTEM SETTINGS".into(),
            rows: vec!["440.0Hz".into()],
            ..Default::default()
        };
        let two = OledText {
            rows: vec!["440.0Hz\t  0 cent".into()],
            ..one.clone()
        };
        assert!(
            painted(&two) > painted(&one),
            "the second part is its own text"
        );
    }

    #[test]
    fn free_lines_replace_the_label_and_a_cursor_adds_a_lit_cell() {
        let bar_only = OledText {
            bar: Some(String::new()),
            big: "STORE PROGRAM AS".into(),
            sub: "ignored label".into(),
            ..Default::default()
        };
        let with_line = OledText {
            lines: vec![Line::big(19, "Unnamed")],
            ..bar_only.clone()
        };
        // The line is painted (its text is one shape; the label `sub` is not painted any more).
        assert!(painted(&with_line) >= painted(&bar_only));
        let mut cursor = Line::big(19, "Unnamed");
        cursor.cursor = Some(0);
        let with_cursor = OledText {
            lines: vec![cursor],
            ..bar_only.clone()
        };
        assert!(
            painted(&with_cursor) >= painted(&with_line) + 2,
            "the lit cell and the unlit glyph"
        );
    }
}
