//! Unit tests of the Program OLED painter.

use super::*;
use crate::panel::support::shapes_painted;

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
