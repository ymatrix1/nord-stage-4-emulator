//! Piano section (photo crop x 709..841).
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

pub fn rect(id: &str) -> Option<Rect> {
    Some(match id {
        "piano.on" => px(792.0, 121.0, 814.0, 134.0),
        "piano.fx_focus_led" => px(764.0, 127.0, 769.0, 132.0),
        "piano.fader.a" => px(714.0, 137.0, 728.0, 205.0),
        "piano.fader.b" => px(744.0, 137.0, 758.0, 205.0),
        "piano.ladder.a" => px(732.0, 140.0, 741.0, 200.0),
        "piano.ladder.b" => px(762.0, 140.0, 771.0, 200.0),
        "piano.focus_led.a" => px(714.0, 209.0, 720.0, 215.0),
        "piano.aux_led.a" => px(730.0, 209.0, 736.0, 215.0),
        "piano.focus_led.b" => px(744.0, 209.0, 750.0, 215.0),
        "piano.aux_led.b" => px(760.0, 209.0, 766.0, 215.0),
        "piano.layer.a" => px(716.0, 225.0, 739.0, 238.0),
        "piano.layer.b" => px(746.0, 225.0, 769.0, 238.0),
        "piano.octave_down" => px(719.0, 290.0, 742.0, 302.0),
        "piano.octave_up" => px(743.0, 290.0, 765.0, 302.0),
        "piano.zone_leds" => px(731.0, 309.0, 754.0, 315.0),
        "piano.type" => px(795.0, 253.0, 818.0, 266.0),
        // Widened from 800..812 to hold the 2 x 3 LED block with the six type names printed beside it.
        "piano.type_leds" => px(778.0, 229.0, 836.0, 251.0),
        "piano.select_dial" => px(797.0, 285.0, 815.0, 303.0),
        "piano.soft_release" => px(778.0, 161.0, 801.0, 174.0),
        "piano.kb_touch" => px(778.0, 206.0, 801.0, 219.0),
        "piano.dyn_comp" => px(810.0, 206.0, 833.0, 219.0),
        "piano.unison" => px(810.0, 161.0, 833.0, 174.0),
        "piano.timbre" => px(717.0, 254.0, 731.0, 278.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_is_square() {
        let d = rect("piano.select_dial").unwrap();
        assert!((d.w - d.h).abs() < 1e-3);
    }

    #[test]
    fn unknown_id_is_none() {
        assert!(rect("organ.on").is_none());
    }
}
