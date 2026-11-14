//! Parameter hint on the Program OLED: while a knob moves, the lower half of the home screen
//! shows `<label> <value>` (manual p.42), as `LFO Rate 0.08 Hz`. Other screens (menus, STORE,
//! LIST, split) keep their own text: a knob does not interrupt them.

use super::model::Screen;
use super::oled::OledText;

/// Replace the rows of the home screen with the hint line.
pub fn apply(text: &mut OledText, screen: Screen, hint: Option<&str>) {
    if let (Screen::Home, Some(h)) = (screen, hint) {
        text.rows = vec![h.to_string()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> OledText {
        OledText {
            big: "A:11".into(),
            sub: "Name".into(),
            rows: vec!["Org".into(), "Pno".into(), "Syn".into()],
            soft: [""; 4],
            ..Default::default()
        }
    }

    #[test]
    fn home_shows_the_hint_in_the_lower_half_and_keeps_the_title() {
        let mut t = text();
        apply(&mut t, Screen::Home, Some("LFO Rate 0.08 Hz"));
        assert_eq!(t.rows, vec!["LFO Rate 0.08 Hz".to_string()]);
        assert_eq!((t.big.as_str(), t.sub.as_str()), ("A:11", "Name"));
    }

    #[test]
    fn other_screens_and_no_hint_are_untouched() {
        let mut t = text();
        apply(&mut t, Screen::Home, None);
        assert_eq!(t, text());
        apply(&mut t, Screen::List, Some("LFO Rate 0.08 Hz"));
        assert_eq!(t, text());
        apply(&mut t, Screen::Split, Some("LFO Rate 0.08 Hz"));
        assert_eq!(t, text());
    }
}
