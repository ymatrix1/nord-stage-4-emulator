//! Program area, part 1 (manual p.38-46): OLED, dial, Program buttons and
//! menu shortcuts, Store, views, edit helpers, Layer Scene, status LEDs.
//! Morph, Split, clock, transpose and Preset Library are in `program_perf.rs`.

use crate::types::{ControlEntry, ControlKind as K, Section};

/// Shift+Program 1..8 menu shortcuts (p.44).
const MENUS: [&str; 8] = [
    "system", "sound", "organize", "aux_kb", "output", "pedal", "midi", "extern",
];

/// Reason shared by PAGE / BANK: they move the browse cursor of the program list only.
const BROWSE: &str =
    "moves the program-list browse cursor; the program changes only when a Program button picks a slot";

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Program;
    let e = ControlEntry::new;
    let mut v = vec![
        e("program.oled", s, K::Oled, "PROGRAM DISPLAY", 38).ui("drawn by the OLED renderer (B8d)"),
        e("program.dial", s, K::Encoder, "PROGRAM DIAL", 41).bound("ui:program.dial"),
        e("program.list", s, K::Encoder, "LIST (Shift+dial)", 41)
            .shift()
            .ui("list view of programs"),
        e("program.shift.left", s, K::Button, "SHIFT / EXIT", 44).ui("modifier key"),
        e(
            "program.shift.right",
            s,
            K::Button,
            "SHIFT (Layer Effects side)",
            44,
        )
        .ui("second modifier key"),
        e("program.page_down", s, K::Button, "PAGE -", 41).ui(BROWSE),
        e("program.page_up", s, K::Button, "PAGE +", 41).ui(BROWSE),
        e("program.bank_down", s, K::Button, "BANK - (Shift+Page)", 41)
            .shift()
            .ui(BROWSE),
        e("program.bank_up", s, K::Button, "BANK + (Shift+Page)", 41)
            .shift()
            .ui(BROWSE),
    ];
    for n in 1..=8 {
        v.push(
            e(
                &format!("program.button.{n}"),
                s,
                K::Button,
                "PROGRAM 1-8",
                44,
            )
            .bound("ui:program.slot"),
        );
    }
    for (i, m) in MENUS.iter().enumerate() {
        v.push(
            e(
                &format!("program.menu.{m}"),
                s,
                K::Button,
                "MENU (Shift+Program n)",
                44,
            )
            .shift()
            .assumed(if i < 4 {
                "menu button order taken from p.44-46 examples"
            } else {
                "menu button order inferred from the panel photo"
            })
            .ui("opens the menu screen; its items are bound in nord_panel::menus"),
        );
    }
    v.extend([
        e("program.prog_view", s, K::Button, "PROG VIEW", 42)
            .ui("cycles the OLED home view (name, category, ...); changes no sound parameter"),
        e(
            "program.preset_name",
            s,
            K::Button,
            "PRESET NAME (Shift+Prog View)",
            42,
        )
        .shift()
        .ui("shows the program name on the OLED while latched; changes no sound parameter"),
        e("program.live_mode", s, K::Button, "LIVE MODE", 44).bound("ui:program.live_mode"),
        e(
            "program.num_pad",
            s,
            K::Button,
            "NUM PAD (Shift+Live Mode)",
            44,
        )
        .shift()
        .bound("ui:program.num_pad"),
        e("program.store", s, K::Button, "STORE", 41).bound("ui:program.store"),
        e(
            "program.store_as",
            s,
            K::Button,
            "STORE AS (Shift+Store)",
            42,
        )
        .shift()
        .bound("ui:program.store_as"),
        e("program.solo", s, K::Button, "SOLO", 42).ui("monitor mode, not stored"),
        e("program.undo", s, K::Button, "UNDO (Shift+Solo)", 42)
            .shift()
            .bound("ui:program.undo"),
        e("program.section_edit", s, K::Button, "SECTION EDIT", 43)
            .bound("ui:program.section_edit"),
        e(
            "program.layer_init",
            s,
            K::Button,
            "LAYER INIT (Shift+Section Edit)",
            43,
        )
        .shift()
        .bound("ui:program.layer_init"),
        e("program.mon_copy", s, K::Button, "MON/COPY", 43).bound("ui:program.mon_copy"),
        e("program.paste", s, K::Button, "PASTE (Shift+Mon/Copy)", 43)
            .shift()
            .bound("ui:program.paste"),
        e("program.layer_scene_ii", s, K::Button, "LAYER SCENE II", 43).bound("layer_scene_ii"),
        e("program.midi_led", s, K::Led, "MIDI", 41).ui("blinks on incoming MIDI"),
        e("program.extern_led", s, K::Led, "EXTERN", 41).ui("blinks on outgoing Extern MIDI"),
    ]);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_part1_total() {
        assert_eq!(controls().len(), 40);
    }

    #[test]
    fn eight_program_buttons_and_eight_menus() {
        let c = controls();
        assert_eq!(
            c.iter()
                .filter(|x| x.id.starts_with("program.button."))
                .count(),
            8
        );
        assert_eq!(
            c.iter()
                .filter(|x| x.id.starts_with("program.menu."))
                .count(),
            8
        );
    }
}
