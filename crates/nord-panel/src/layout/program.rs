//! Central Program area (left crop x 2663..3754): OLED, Program buttons,
//! Morph Assign, Split, Mst Clk, Transpose, Preset Library, the right-hand
//! Solo / Section Edit / Mon-Copy column and the SHIFT rocker.
//!
//! VERIFIED (centres): every control listed in `data/photo_anchors_program.csv` (crop
//! `NS4_Panel-crop-left.jpg`, mapped affinely from the crop's section outline onto
//! [`super::section_rect`] like the Organ pass), checked by
//! `python tools/layout_overlay.py program`, max centre error below 0.1 mm before nudges (the
//! tables below are the mapped centres converted back to hero-photo px). Sizes keep the old reads.
//! ASSUMED: the crop's section is 1.31x taller in mm than the real one would be at the
//! horizontal scale (see the spec); `program.shift.right` (right crop) is placed from the
//! hero photo only.

use super::{px, Rect};

/// Rectangle `w` x `h` photo px centred on `(cx, cy)` photo px.
fn at(cx: f32, cy: f32, w: f32, h: f32) -> Rect {
    px(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0)
}

/// Program button `n` (1..=8): two rows of four, 25.1 photo px pitch.
fn program_button(id: &str) -> Option<Rect> {
    let n: u32 = id
        .strip_prefix("program.button.")?
        .parse()
        .ok()
        .filter(|n| (1..=8).contains(n))?;
    let col = ((n - 1) % 4) as f32;
    let row = ((n - 1) / 4) as f32;
    Some(at(904.6 + 25.1 * col, 286.2 + 35.3 * row, 22.0, 12.0))
}

pub fn rect(id: &str) -> Option<Rect> {
    if let Some(r) = program_button(id) {
        return Some(r);
    }
    Some(match id {
        "program.oled" => at(942.2, 210.3, 85.0, 42.0),
        "program.dial" => at(871.8, 196.9, 18.0, 18.0),
        "program.page_down" => at(860.0, 236.7, 22.0, 12.0),
        "program.page_up" => at(883.4, 236.7, 22.0, 12.0),
        "program.shift.left" => at(1016.9, 313.8, 14.0, 24.0),
        "program.shift.right" => px(1409.0, 285.0, 1423.0, 309.0),
        "program.prog_view" => at(1014.4, 145.0, 23.0, 13.0),
        "program.live_mode" => at(866.5, 274.0, 22.0, 12.0),
        "program.store" => at(867.5, 153.3, 23.0, 13.0),
        "program.solo" => at(1014.4, 186.8, 23.0, 13.0),
        "program.section_edit" => at(1014.4, 227.7, 23.0, 13.0),
        "program.mon_copy" => at(1014.4, 264.5, 23.0, 13.0),
        "program.layer_scene_ii" => at(866.3, 319.2, 22.0, 12.0),
        "program.midi_led" => at(891.7, 150.6, 5.0, 5.0),
        "program.extern_led" => at(891.7, 167.3, 5.0, 5.0),
        "morph.wheel" => at(866.3, 109.0, 23.0, 12.0),
        "morph.at" => at(889.8, 109.0, 23.0, 12.0),
        "morph.ctrlped" => at(913.2, 109.0, 23.0, 12.0),
        "split.on_set" => at(948.7, 113.8, 22.0, 13.0),
        "split.leds" => at(948.7, 94.3, 23.0, 5.0),
        "mst_clk.tap_set" => at(981.6, 109.3, 23.0, 13.0),
        "transp.on_set" => at(1014.5, 109.3, 23.0, 13.0),
        "preset_library.organ" => at(918.9, 158.3, 22.0, 12.0),
        "preset_library.piano" => at(942.3, 158.3, 22.0, 12.0),
        "preset_library.synth" => at(965.6, 158.3, 22.0, 12.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_buttons_form_two_rows_of_four() {
        let b: Vec<Rect> = (1..=8)
            .map(|n| rect(&format!("program.button.{n}")).unwrap())
            .collect();
        assert!(b[0].x < b[1].x && (b[0].y - b[3].y).abs() < 1e-3);
        assert!(b[4].y > b[0].y && (b[0].x - b[4].x).abs() < 1e-3);
        assert!(rect("program.button.9").is_none());
        assert!(rect("program.button.0").is_none());
    }

    #[test]
    fn oled_is_wider_than_tall() {
        let o = rect("program.oled").unwrap();
        assert!(o.w > o.h);
        assert!(rect("synth.oled").is_none());
    }
}
