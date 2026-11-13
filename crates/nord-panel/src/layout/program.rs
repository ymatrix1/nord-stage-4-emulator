//! Central Program area (photo crop x 841..1029): OLED, Program buttons,
//! Morph Assign, Split, Mst Clk, Transpose, Preset Library, the right-hand
//! Solo / Section Edit / Mon-Copy column and the two SHIFT rockers.
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

/// Program button `n` (1..=8): two rows of four, 25 photo px pitch.
fn program_button(id: &str) -> Option<Rect> {
    let n: u32 = id
        .strip_prefix("program.button.")?
        .parse()
        .ok()
        .filter(|n| (1..=8).contains(n))?;
    let col = ((n - 1) % 4) as f32;
    let row = ((n - 1) / 4) as f32;
    let x = 888.0 + 25.0 * col;
    let y = 270.0 + 26.0 * row;
    Some(px(x, y, x + 22.0, y + 12.0))
}

pub fn rect(id: &str) -> Option<Rect> {
    if let Some(r) = program_button(id) {
        return Some(r);
    }
    Some(match id {
        "program.oled" => px(894.0, 197.0, 979.0, 239.0),
        "program.dial" => px(857.0, 199.0, 875.0, 217.0),
        "program.page_down" => px(843.0, 232.0, 865.0, 244.0),
        "program.page_up" => px(867.0, 232.0, 889.0, 244.0),
        "program.shift.left" => px(1004.0, 283.0, 1018.0, 307.0),
        "program.shift.right" => px(1409.0, 285.0, 1423.0, 309.0),
        "program.prog_view" => px(997.0, 161.0, 1020.0, 174.0),
        "program.live_mode" => px(850.0, 261.0, 872.0, 273.0),
        "program.store" => px(850.0, 168.0, 873.0, 181.0),
        "program.solo" => px(997.0, 193.0, 1020.0, 206.0),
        "program.section_edit" => px(997.0, 225.0, 1020.0, 238.0),
        "program.mon_copy" => px(997.0, 252.0, 1020.0, 265.0),
        "program.layer_scene_ii" => px(850.0, 294.0, 872.0, 306.0),
        "program.midi_led" => px(882.0, 164.0, 887.0, 169.0),
        "program.extern_led" => px(884.0, 178.0, 889.0, 183.0),
        "morph.wheel" => px(848.0, 135.0, 871.0, 147.0),
        "morph.at" => px(872.0, 135.0, 895.0, 147.0),
        "morph.ctrlped" => px(896.0, 135.0, 919.0, 147.0),
        "split.on_set" => px(931.0, 138.0, 953.0, 151.0),
        "split.leds" => px(931.0, 126.0, 954.0, 131.0),
        "mst_clk.tap_set" => px(965.0, 135.0, 988.0, 148.0),
        "transp.on_set" => px(997.0, 135.0, 1020.0, 148.0),
        "preset_library.organ" => px(902.0, 172.0, 924.0, 184.0),
        "preset_library.piano" => px(926.0, 172.0, 948.0, 184.0),
        "preset_library.synth" => px(949.0, 172.0, 971.0, 184.0),
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
