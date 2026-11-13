//! Master level, wheels and the Rotary Speaker box (photo crop x 67..415).
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

pub fn rect(id: &str) -> Option<Rect> {
    Some(match id {
        "master.level" => px(383.0, 136.0, 406.0, 159.0),
        "master.pitch_stick" => px(256.0, 150.0, 300.0, 175.0),
        "master.mod_wheel" => px(314.0, 176.0, 341.0, 248.0),
        "rotary.drive" => px(382.0, 191.0, 404.0, 213.0),
        "rotary.organ" => px(385.0, 229.0, 407.0, 241.0),
        "rotary.stop_mode" => px(385.0, 262.0, 407.0, 274.0),
        "rotary.speed_leds" => px(386.0, 285.0, 406.0, 291.0),
        "rotary.speed" => px(385.0, 294.0, 407.0, 306.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_ids() {
        assert!(rect("master.level").is_some());
        assert!(rect("organ.on").is_none());
    }

    #[test]
    fn wheel_is_taller_than_wide() {
        let w = rect("master.mod_wheel").unwrap();
        assert!(w.h > w.w);
    }
}
