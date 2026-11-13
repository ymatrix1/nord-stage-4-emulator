//! Keybed (photo rows 340..571). The three split LEDs are drawn as one strip
//! along the top edge of the keys (ESTIMATED: the photo does not resolve them).

use super::{px, Rect};

pub fn rect(id: &str) -> Option<Rect> {
    Some(match id {
        "keybed.split_leds" => px(113.0, 341.0, 1892.0, 349.0),
        "keybed.keys" => px(113.0, 350.0, 1892.0, 560.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_wide_and_below_the_led_strip() {
        let k = rect("keybed.keys").unwrap();
        let l = rect("keybed.split_leds").unwrap();
        assert!(k.w > 1000.0);
        assert!(l.bottom() <= k.y + 1e-3);
        assert!(rect("master.level").is_none());
    }
}
