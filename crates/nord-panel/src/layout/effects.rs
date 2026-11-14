//! Layer Effects section (photo crop x 1401..1700): FX Focus column, Mod 1,
//! Mod 2, Amp Sim/EQ on the left; Delay, Comp and Reverb on the right.
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

pub fn rect(id: &str) -> Option<Rect> {
    Some(match id {
        // Header and FX Focus column.
        "fx.layer_effects_on" => px(1502.0, 121.0, 1525.0, 134.0),
        "fx.focus.organ" => px(1406.0, 166.0, 1429.0, 179.0),
        "fx.focus.piano" => px(1405.0, 208.0, 1428.0, 221.0),
        "fx.focus.synth" => px(1405.0, 251.0, 1428.0, 264.0),
        "fx.focus.leds" => px(1404.0, 240.0, 1427.0, 245.0),
        // Mod 1 / Mod 2.
        "mod1.rate" => px(1442.0, 147.0, 1464.0, 169.0),
        "mod1.amount" => px(1482.0, 147.0, 1504.0, 169.0),
        "mod1.selector" => px(1518.0, 166.0, 1541.0, 179.0),
        "mod1.on" => px(1553.0, 157.0, 1567.0, 182.0),
        "mod2.rate" => px(1442.0, 197.0, 1464.0, 219.0),
        "mod2.amount" => px(1482.0, 197.0, 1504.0, 219.0),
        "mod2.selector" => px(1517.0, 213.0, 1540.0, 226.0),
        "mod2.on" => px(1553.0, 205.0, 1567.0, 229.0),
        // Amp Sim / EQ.
        "amp.drive" => px(1441.0, 242.0, 1463.0, 264.0),
        "amp.mid_freq" => px(1482.0, 242.0, 1504.0, 264.0),
        "amp.model" => px(1522.0, 260.0, 1545.0, 273.0),
        "amp.on" => px(1553.0, 280.0, 1567.0, 304.0),
        "amp.bass" => px(1441.0, 283.0, 1463.0, 305.0),
        "amp.mid" => px(1482.0, 283.0, 1504.0, 305.0),
        "amp.treble" => px(1522.0, 283.0, 1544.0, 305.0),
        // Delay.
        "delay.tempo" => px(1581.0, 152.0, 1603.0, 174.0),
        "delay.feedback" => px(1658.0, 141.0, 1680.0, 163.0),
        "delay.effects" => px(1617.0, 172.0, 1640.0, 185.0),
        "delay.filter" => px(1653.0, 192.0, 1676.0, 205.0),
        "delay.dry_wet" => px(1618.0, 204.0, 1640.0, 226.0),
        "delay.tap_set" => px(1580.0, 206.0, 1603.0, 219.0),
        "delay.on" => px(1657.0, 216.0, 1680.0, 228.0),
        // Comp.
        "comp.amount" => px(1580.0, 252.0, 1602.0, 274.0),
        "comp.on" => px(1580.0, 291.0, 1603.0, 303.0),
        // Reverb.
        "reverb.bright_dark" => px(1616.0, 258.0, 1639.0, 271.0),
        "reverb.type" => px(1651.0, 266.0, 1674.0, 279.0),
        "reverb.dry_wet" => px(1619.0, 284.0, 1641.0, 306.0),
        "reverb.on" => px(1656.0, 291.0, 1679.0, 303.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod2_is_below_mod1() {
        let a = rect("mod1.rate").unwrap();
        let b = rect("mod2.rate").unwrap();
        assert!(b.y > a.bottom());
    }

    #[test]
    fn delay_is_right_of_the_mod_rockers() {
        let on = rect("mod1.on").unwrap();
        let d = rect("delay.tempo").unwrap();
        assert!(d.x > on.right());
        assert!(rect("synth.on").is_none());
    }
}
