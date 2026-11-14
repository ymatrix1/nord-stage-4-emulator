//! Organ section (photo crop x 415..709). Drawbar `n` sits at
//! `x = 486 + 25.2 * (n - 1)` photo px; its LED ladder is the 15 px column
//! just to the right of the slot.
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

const DRAWBAR_X0: f32 = 486.0;
const DRAWBAR_PITCH: f32 = 25.2;

fn drawbar(id: &str) -> Option<Rect> {
    let (n, ladder) = if let Some(n) = id.strip_prefix("organ.drawbar_ladder.") {
        (n, true)
    } else {
        (id.strip_prefix("organ.drawbar.")?, false)
    };
    let n: u32 = n.parse().ok().filter(|n| (1..=9).contains(n))?;
    let cx = DRAWBAR_X0 + DRAWBAR_PITCH * (n - 1) as f32;
    Some(if ladder {
        px(cx + 4.0, 212.0, cx + 19.0, 272.0)
    } else {
        px(cx - 4.0, 236.0, cx + 4.0, 313.0)
    })
}

pub fn rect(id: &str) -> Option<Rect> {
    if let Some(r) = drawbar(id) {
        return Some(r);
    }
    Some(match id {
        "organ.on" => px(496.0, 121.0, 519.0, 134.0),
        "organ.fx_focus_led" => px(467.0, 127.0, 472.0, 132.0),
        "organ.fader.a" => px(417.0, 137.0, 431.0, 205.0),
        "organ.fader.b" => px(448.0, 137.0, 462.0, 205.0),
        "organ.ladder.a" => px(436.0, 140.0, 445.0, 200.0),
        "organ.ladder.b" => px(466.0, 140.0, 475.0, 200.0),
        "organ.focus_led.a" => px(419.0, 209.0, 425.0, 215.0),
        "organ.aux_led.a" => px(435.0, 209.0, 441.0, 215.0),
        "organ.focus_led.b" => px(449.0, 209.0, 455.0, 215.0),
        "organ.aux_led.b" => px(465.0, 209.0, 471.0, 215.0),
        "organ.layer.a" => px(420.0, 226.0, 443.0, 238.0),
        "organ.layer.b" => px(449.0, 226.0, 473.0, 238.0),
        "organ.octave_down" => px(424.0, 291.0, 446.0, 302.0),
        "organ.octave_up" => px(447.0, 291.0, 470.0, 302.0),
        "organ.zone_leds" => px(436.0, 309.0, 460.0, 315.0),
        "organ.model" => px(500.0, 178.0, 523.0, 190.0),
        "organ.model_leds" => px(505.0, 154.0, 518.0, 174.0),
        "organ.preset" => px(436.0, 263.0, 459.0, 275.0),
        "organ.vibrato_selector" => px(546.0, 156.0, 570.0, 169.0),
        "organ.vibrato_on.a" => px(560.0, 181.0, 571.0, 194.0),
        "organ.vibrato_on.b" => px(571.0, 181.0, 582.0, 194.0),
        "organ.perc_volume" => px(614.0, 158.0, 637.0, 171.0),
        "organ.perc_decay" => px(642.0, 158.0, 665.0, 171.0),
        "organ.perc_third" => px(670.0, 158.0, 693.0, 171.0),
        "organ.perc_on" => px(670.0, 181.0, 693.0, 194.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_drawbars_in_a_row_left_to_right() {
        let xs: Vec<f32> = (1..=9)
            .map(|n| rect(&format!("organ.drawbar.{n}")).unwrap().x)
            .collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]));
        assert!(rect("organ.drawbar.0").is_none());
        assert!(rect("organ.drawbar.10").is_none());
    }

    #[test]
    fn ladder_sits_right_of_its_drawbar_slot() {
        let d = rect("organ.drawbar.4").unwrap();
        let l = rect("organ.drawbar_ladder.4").unwrap();
        assert!(l.x >= d.right() - 1e-3);
    }

    #[test]
    fn unknown_id_is_none() {
        assert!(rect("piano.on").is_none());
    }
}
