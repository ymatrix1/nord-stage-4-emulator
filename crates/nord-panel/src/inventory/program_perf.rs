//! Program area, part 2 (manual p.15, p.38-42): Morph Assign, Split,
//! Master Clock, Transpose and the Preset Library buttons.

use crate::types::{ControlEntry, ControlKind as K, Section};

/// The three Morph Assign buttons share one GUI path (`MorphUi`: hold or latch, dial = depth).
const ASSIGN: &str = "ui:program.morph_assign";
/// The Preset Library buttons share the Preset browser screen.
const PRESETS: &str = "ui:program.preset_library";

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Program;
    let e = ControlEntry::new;
    vec![
        // Morph Assign (p.38-39)
        e("morph.wheel", s, K::Button, "MORPH ASSIGN WHEEL", 38).bound(ASSIGN),
        e("morph.at", s, K::Button, "MORPH ASSIGN A.T.", 38).bound(ASSIGN),
        e("morph.ctrlped", s, K::Button, "MORPH ASSIGN CTRLPED", 38).bound(ASSIGN),
        e(
            "morph.clear",
            s,
            K::Button,
            "CLEAR MORPH (Shift+Assign)",
            39,
        )
        .shift()
        .bound("ui:program.morph_clear"),
        // Split (p.15, p.39)
        e("split.on_set", s, K::Button, "SPLIT ON/SET", 39).bound("split_enabled"),
        e("split.set_key", s, K::Button, "SET KEY (Shift+Split)", 39)
            .shift()
            .bound("ui:program.split_set_key"),
        e("split.leds", s, K::Led, "SPLIT L / M / H LEDs", 15)
            .ui("show which split points are active"),
        // Master Clock (p.40)
        e("mst_clk.tap_set", s, K::Button, "MST CLK TAP/SET", 40).bound("master_bpm"),
        e(
            "mst_clk.pedal_tap",
            s,
            K::Button,
            "PEDAL TAP (Shift+Mst Clk)",
            40,
        )
        .shift()
        .bound("ui:program.pedal_tap"),
        // Transpose (p.40)
        e("transp.on_set", s, K::Button, "TRANSP ON/SET", 40).bound("prog_transpose"),
        e("transp.panic", s, K::Button, "PANIC (Shift+Transp)", 40)
            .shift()
            .bound("ui:program.panic"),
        // Preset Library (p.19, p.41-42)
        e(
            "preset_library.organ",
            s,
            K::Button,
            "PRESET LIBRARY ORGAN",
            41,
        )
        .bound(PRESETS),
        e(
            "preset_library.piano",
            s,
            K::Button,
            "PRESET LIBRARY PIANO",
            41,
        )
        .bound(PRESETS),
        e(
            "preset_library.synth",
            s,
            K::Button,
            "PRESET LIBRARY SYNTH",
            41,
        )
        .bound(PRESETS),
        e(
            "preset_library.single_piano",
            s,
            K::Button,
            "SINGLE LAYER (Shift+Piano)",
            42,
        )
        .shift()
        .bound(PRESETS),
        e(
            "preset_library.single_synth",
            s,
            K::Button,
            "SINGLE LAYER (Shift+Synth)",
            42,
        )
        .shift()
        .bound(PRESETS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_part2_total() {
        assert_eq!(controls().len(), 16);
    }

    #[test]
    fn three_morph_sources() {
        let c = controls();
        let n = c
            .iter()
            .filter(|x| x.id.starts_with("morph.") && x.id != "morph.clear")
            .count();
        assert_eq!(n, 3);
    }
}
