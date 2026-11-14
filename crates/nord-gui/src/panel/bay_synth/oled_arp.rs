//! The Arpeggiator MENU pages of the Synth OLED (manual p.36): direction and Zig Zag, preset and
//! length, and the per-step Gate / Accent / Pan edit pages.

use super::oled_types::Graphic;
use crate::panel::ui_state::{SynthLayerUi, SynthUi, ARP_DIRECTIONS};

/// Cursor step index of the pattern edit pages.
fn cursor(l: &SynthLayerUi) -> usize {
    usize::from(l.pattern_cursor).min(15)
}

/// Cursor step as a 1-based label.
fn step_no(l: &SynthLayerUi) -> String {
    (cursor(l) + 1).to_string()
}

/// Title, graphic and the three soft values of ARP page `n` (`1..=5`).
pub(super) fn arp_page(sui: &SynthUi, n: u8) -> (&'static str, Graphic, [String; 3]) {
    let l = sui.layer();
    let pages = format!("{n}/5");
    let gfx = match n {
        1 => Graphic::Direction(l.arp_dir),
        2 => Graphic::Pattern {
            steps: l.step_gate,
            len: l.pattern_len,
            cursor: None,
        },
        _ => Graphic::Pattern {
            steps: l.step_gate,
            len: l.pattern_len,
            cursor: Some(l.pattern_cursor),
        },
    };
    let (v1, v2) = match n {
        1 => (
            ARP_DIRECTIONS[usize::from(l.arp_dir) % 4].to_string(),
            if l.zigzag { "ON" } else { "OFF" }.to_string(),
        ),
        2 => (format!("{}", l.pattern_preset), l.pattern_len.to_string()),
        3 => (step_no(l), format!("{}/8", l.step_gate[cursor(l)])),
        4 => (
            step_no(l),
            if l.step_accent[cursor(l)] {
                "ON"
            } else {
                "OFF"
            }
            .to_string(),
        ),
        _ => (
            step_no(l),
            ["LEFT", "CENTER", "RIGHT"][usize::from(l.step_pan[cursor(l)]) % 3].to_string(),
        ),
    };
    (sui.page.title(), gfx, [pages, v1, v2])
}

#[cfg(test)]
mod tests {
    use super::super::oled_view::view_of;
    use super::*;
    use crate::panel::ui_state::SynthPage;

    #[test]
    fn arp_menu_has_five_pages_and_no_dimmed_dial() {
        let mut s = SynthUi::new();
        for n in 1..=5u8 {
            s.page = SynthPage::Arp(n);
            let v = view_of(&s, None);
            assert_eq!(v.soft[0].as_ref().unwrap().value, format!("{n}/5"));
            assert_eq!(v.soft[0].as_ref().unwrap().label, "PAGE");
        }
        for n in 1..=5u8 {
            s.page = SynthPage::Arp(n);
            let v = view_of(&s, None);
            assert!(v.soft.iter().flatten().all(|x| !x.dim), "page {n}");
        }
    }

    #[test]
    fn pattern_pages_show_the_cursor_step_values() {
        let mut s = SynthUi::new();
        s.open(SynthPage::Arp(3));
        s.turn_dial(1, 1, false);
        s.turn_dial(2, 3, false);
        let v = view_of(&s, None);
        assert_eq!(v.soft[1].as_ref().unwrap().value, "2");
        assert_eq!(v.soft[2].as_ref().unwrap().value, "7/8");
        assert!(matches!(
            v.graphic,
            Graphic::Pattern {
                cursor: Some(1),
                ..
            }
        ));
        s.page = SynthPage::Arp(4);
        s.turn_dial(2, 1, false);
        assert_eq!(view_of(&s, None).soft[2].as_ref().unwrap().value, "ON");
        s.page = SynthPage::Arp(5);
        s.turn_dial(2, 1, false);
        assert_eq!(view_of(&s, None).soft[2].as_ref().unwrap().value, "RIGHT");
    }

    #[test]
    fn page_one_shows_direction_and_zig_zag() {
        let mut s = SynthUi::new();
        s.open(SynthPage::Arp(1));
        s.turn_dial(1, 1, false);
        s.turn_dial(2, 1, false);
        let (title, gfx, v) = arp_page(&s, 1);
        assert_eq!(title, "Arpeggiator");
        assert!(matches!(gfx, Graphic::Direction(1)));
        assert_eq!(v, ["1/5", "DOWN", "ON"]);
        s.open(SynthPage::Arp(2));
        let (_, _, v) = arp_page(&s, 2);
        assert_eq!(v, ["2/5", "0", "8"], "preset 0, 8 steps");
    }
}
