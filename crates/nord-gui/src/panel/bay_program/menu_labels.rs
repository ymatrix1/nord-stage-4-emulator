//! Labels and value texts of the menu pages as the real display prints them.
//!
//! VERIFIED (ARM frames, `tools/arm_menu_edit.py`, research note s12.3 / s14 / s16.2): the label
//! under the title bar of the pages whose real text differs from the model's page title, and the
//! value strings of the dB, semitone and fine-tune items. Pages not listed keep the model's page
//! title, which matches the real label.

use nord_panel::menus::{Menu, Value};

/// The real label (8 px line under the title bar) of page `page` (1-based) of `menu`, when it
/// differs from the model's page title.
pub fn page_label(menu: Menu, page: u8) -> Option<&'static str> {
    Some(match (menu, page) {
        (Menu::Sound, 1) => "Volume Level Offset",
        (Menu::Output, 2) => "Bus Output Channels",
        (Menu::Output, 3) => "Organ Output Channel",
        (Menu::Output, 4) => "Piano Output Channel",
        (Menu::Output, 5) => "Synth Output Channel",
        (Menu::Pedal, 1) => "Control Pedal Type",
        (Menu::Pedal, 2) => "Swell Pedal Type",
        (Menu::Pedal, 3) => "Sustain Pedal Type",
        (Menu::Pedal, 5) => "Rotor Pedal Type",
        (Menu::Pedal, 6) => "Foot Switch Type",
        (Menu::Midi, 2) => "Global Channel",
        (Menu::Midi, 7) => "Control Change Type",
        (Menu::System, 6) => "Version",
        _ => return None,
    })
}

/// Reference pitch in Hz of Fine Tune `cents` (`440 * 2^(cents / 1200)`).
///
/// VERIFIED: `+1 cent` reads `440.3Hz`, `+3` `440.8Hz`, `-3` `439.2Hz`, `-7` `438.2Hz`, `-12`
/// `437.0Hz` on the real display (research note s16.2, `tests::fine_tune_matches_the_frames`).
fn fine_tune_hz(cents: i32) -> f32 {
    440.0 * (cents as f32 / 1200.0).exp2()
}

/// Value text of the items whose real display differs from the generic number format, or `None`.
///
/// Fine Tune is a two-part value: the pitch and the offset in cents, joined by a tab (the painter
/// right-aligns the second part, as the real widget does).
pub fn special_value(id: &str, v: Value) -> Option<String> {
    match (id, v) {
        ("system.2.transpose", Value::Int(0)) => Some("None".into()),
        ("system.2.transpose", Value::Int(i)) => Some(format!("{i:+} semi")),
        ("system.3.fine_tune", Value::Int(c)) => {
            let cents = if c == 0 { "0".into() } else { format!("{c:+}") };
            Some(format!("{:.1}Hz\t{cents:>3} cent", fine_tune_hz(c)))
        }
        ("sound.1.program_levels", Value::Float(f)) => Some(format!("{f:+.1} dB")),
        ("sound.2.piano_pedal_noise" | "sound.3.piano_string_res", Value::Float(f)) => {
            let db = f.round() as i32;
            Some(if db == 0 {
                " 0 dB".into()
            } else {
                format!("{db:+} dB")
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_real_pages() {
        assert_eq!(page_label(Menu::Sound, 1), Some("Volume Level Offset"));
        assert_eq!(page_label(Menu::Pedal, 5), Some("Rotor Pedal Type"));
        assert_eq!(page_label(Menu::Output, 3), Some("Organ Output Channel"));
        assert_eq!(
            page_label(Menu::System, 1),
            None,
            "Memory Protect already matches"
        );
        assert_eq!(page_label(Menu::Sound, 2), None);
    }

    #[test]
    fn fine_tune_matches_the_frames() {
        let f = |c| special_value("system.3.fine_tune", Value::Int(c)).unwrap();
        // Research s16.2: 440.0Hz/0, 440.3/+1, 440.8/+3, 439.2/-3, 438.2/-7, 437.0/-12.
        assert_eq!(f(0), "440.0Hz\t  0 cent");
        assert_eq!(f(1), "440.3Hz\t +1 cent");
        assert_eq!(f(3), "440.8Hz\t +3 cent");
        assert_eq!(f(-3), "439.2Hz\t -3 cent");
        assert_eq!(f(-7), "438.2Hz\t -7 cent");
        assert_eq!(f(-12), "437.0Hz\t-12 cent");
    }

    #[test]
    fn semitone_and_db_values_read_like_the_frames() {
        let v = |id, x| special_value(id, x).unwrap();
        assert_eq!(v("system.2.transpose", Value::Int(0)), "None");
        assert_eq!(v("system.2.transpose", Value::Int(3)), "+3 semi");
        assert_eq!(v("system.2.transpose", Value::Int(-6)), "-6 semi");
        assert_eq!(v("sound.1.program_levels", Value::Float(0.0)), "+0.0 dB");
        assert_eq!(v("sound.1.program_levels", Value::Float(-7.5)), "-7.5 dB");
        assert_eq!(v("sound.2.piano_pedal_noise", Value::Float(0.0)), " 0 dB");
        assert_eq!(v("sound.3.piano_string_res", Value::Float(-3.0)), "-3 dB");
        assert_eq!(v("sound.2.piano_pedal_noise", Value::Float(1.0)), "+1 dB");
        assert!(special_value("midi.2.global_channel", Value::Int(1)).is_none());
    }
}
