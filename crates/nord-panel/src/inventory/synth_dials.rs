//! What the three Synth OLED dials edit on each display page (manual p.27-38, p.46-47).
//!
//! One dial maps to many parameters: the page picks the target. The inventory entries
//! `synth.dial.left/middle/right` are `Bound("dial:<tail>")`; this table is the binding. The
//! GUI (`nord-gui` `ui_state/synth/page.rs`) owns the behaviour, its test
//! `dial_table_matches_the_behaviour` turns every dial of every page and compares the edit it
//! emits with the cell below; the tests here check that every named edit exists in the audio
//! edit enums and every host id in `nord-plugin`.
//!
//! Provenance: page and dial names `VERIFIED` against the manual text (p.28-37, p.46-47); the
//! edit names are the `SynthParam` / `SynthEdit` variants of `nord-gui/src/audio/edits/synth/`.

/// What a dial does on one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialTarget {
    /// Emits an audio edit. `edits` lists the `SynthParam` / `SynthEdit` debug prefixes that can
    /// result (`|` separates alternatives, e.g. one waveform pick sends `Waveform(` in Analog
    /// mode and `FmRatio(` in FM mode); `host` is the host parameter of the same setting.
    Edit {
        edits: &'static str,
        host: Option<&'static str>,
    },
    /// Navigates or browses; the value lives in the panel state and nothing is sent to the
    /// engine by this dial (the reason says where the choice takes effect).
    Ui(&'static str),
    /// The engine cannot honour it; the reason is the documented gap.
    Gap(&'static str),
}

/// One dial of one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialCell {
    /// Page key, see `SynthPage::dial_key` in `nord-gui`.
    pub page: &'static str,
    /// `0` left, `1` middle, `2` right.
    pub dial: u8,
    pub label: &'static str,
    pub target: DialTarget,
}

const fn edit(
    page: &'static str,
    dial: u8,
    label: &'static str,
    edits: &'static str,
    host: Option<&'static str>,
) -> DialCell {
    DialCell {
        page,
        dial,
        label,
        target: DialTarget::Edit { edits, host },
    }
}

const fn ui(page: &'static str, dial: u8, label: &'static str, why: &'static str) -> DialCell {
    DialCell {
        page,
        dial,
        label,
        target: DialTarget::Ui(why),
    }
}

const ARP_NAV: &str = "ARPEGGIATOR MENU page number (1..5); the pages are panel state";
const WAVE_ANY: &str = "Waveform(|FmRatio(|WavetableIndex(";
const SAMPLE_BROWSE: &str =
    "browses the opened backup's samples; the load is a LoadRequest::Sample, not a layer edit";

/// Every (page, dial) the display offers, in page order. The left / middle / right column of a
/// page is its cells with `dial` 0 / 1 / 2.
pub const DIAL_CELLS: &[DialCell] = &[
    edit("waveform.analog", 0, "TYPE", "OscMode(", None),
    edit("waveform.analog", 1, "CAT", WAVE_ANY, None),
    edit("waveform.analog", 2, "WAVE", WAVE_ANY, None),
    edit("waveform.sample", 0, "BRIGHT", "SampleTimbre(", None),
    ui("waveform.sample", 1, "CAT", SAMPLE_BROWSE),
    ui("waveform.sample", 2, "SAMPLE", SAMPLE_BROWSE),
    edit("filter_type", 0, "TYPE", "FilterModel(", None),
    edit(
        "filter_type",
        1,
        "KBD TRK",
        "KbTrack(",
        Some("synthx_kb_track_{n}"),
    ),
    edit(
        "filter_type",
        2,
        "DRIVE",
        "FilterDrive(",
        Some("synth_filter_drive"),
    ),
    edit("env_osc", 0, "ATTACK", "OscEnvAttack(", None),
    edit("env_osc", 1, "DECAY", "OscEnvDecay(", None),
    edit("env_osc", 2, "RELEASE", "OscEnvRelease(", None),
    edit("env_filter", 0, "ATTACK", "FilterEnvAttack(", None),
    edit("env_filter", 1, "DECAY", "FilterEnvDecay(", None),
    edit("env_filter", 2, "RELEASE", "FilterEnvRelease(", None),
    edit("env_amp", 0, "ATTACK", "AmpAttack(", None),
    edit("env_amp", 1, "DECAY", "AmpDecay(", None),
    edit("env_amp", 2, "RELEASE", "AmpRelease(", None),
    edit(
        "lfo_wave",
        0,
        "WAVE",
        "LfoShape(",
        Some("synthm_lfo_shape_{n}"),
    ),
    ui("arp.1", 0, "PAGE", ARP_NAV),
    edit(
        "arp.1",
        1,
        "DIRECTION",
        "ArpMode(",
        Some("synthm_arp_mode_{n}"),
    ),
    edit("arp.1", 2, "ZIG ZAG", "Pattern(Zigzag(", None),
    ui("arp.2", 0, "PAGE", ARP_NAV),
    edit(
        "arp.2",
        1,
        "PRESET",
        "PatternPreset(",
        Some("synthg_pat_preset_{n}"),
    ),
    edit("arp.2", 2, "LENGTH", "Pattern(Len(", None),
    ui("arp.3", 0, "PAGE", ARP_NAV),
    ui(
        "arp.3",
        1,
        "POSITION",
        "moves the pattern step cursor shown on the display",
    ),
    edit("arp.3", 2, "GATE", "Pattern(Gate", None),
    ui("arp.4", 0, "PAGE", ARP_NAV),
    ui(
        "arp.4",
        1,
        "POSITION",
        "moves the pattern step cursor shown on the display",
    ),
    edit("arp.4", 2, "ACCENT", "Pattern(Accent", None),
    ui("arp.5", 0, "PAGE", ARP_NAV),
    ui(
        "arp.5",
        1,
        "POSITION",
        "moves the pattern step cursor shown on the display",
    ),
    edit("arp.5", 2, "PAN", "Pattern(Pan", None),
    edit(
        "vibrato",
        0,
        "RATE",
        "VibratoRate(",
        Some("synthm_vib_rate_{n}"),
    ),
    edit(
        "vibrato",
        1,
        "AMOUNT",
        "VibratoDepth(",
        Some("synthm_vib_depth_{n}"),
    ),
    edit(
        "vibrato",
        2,
        "DLY TIME",
        "VibratoDelay(",
        Some("synthm_vib_delay_{n}"),
    ),
    edit("pitch_smp.analog", 0, "PITCH", "PitchCents(", None),
    edit("pitch_smp.analog", 1, "FINE", "PitchCents(", None),
    edit("pitch_smp.sample", 0, "PITCH", "PitchCents(", None),
    edit("pitch_smp.sample", 1, "FINE", "PitchCents(", None),
    DialCell {
        page: "pitch_smp.sample",
        dial: 2,
        label: "SAMPLE OPT",
        target: DialTarget::Gap(
            "Natural / No Dyn / Fast Atk need the sample preset data, not in the engine",
        ),
    },
    edit("extern", 0, "CC VAL 1", "ExternCc {", None),
    edit("extern", 1, "CC VAL 2", "ExternCc {", None),
    edit("extern", 2, "PROGRAM", "ExternProgram {", None),
];

/// Dial column for an inventory tail (`left`, `middle`, `right`).
pub fn column(tail: &str) -> Vec<&'static DialCell> {
    let dial = match tail {
        "left" => 0,
        "middle" => 1,
        "right" => 2,
        _ => return Vec::new(),
    };
    DIAL_CELLS.iter().filter(|c| c.dial == dial).collect()
}

/// Cells of the `Bound("dial:<tail>")` binding string, empty when it is not one.
pub fn cells_of(binding: &str) -> Vec<&'static DialCell> {
    binding.strip_prefix("dial:").map_or_else(Vec::new, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sources that define the audio edits: `SynthParam`, `PatternParam`, `SynthEdit`.
    const EDIT_SOURCES: [&str; 3] = [
        include_str!("../../../nord-gui/src/audio/edits/synth/param.rs"),
        include_str!("../../../nord-gui/src/audio/edits/synth/pattern.rs"),
        include_str!("../../../nord-gui/src/audio/edits/synth/mod.rs"),
    ];

    fn variant_exists(name: &str) -> bool {
        EDIT_SOURCES
            .iter()
            .any(|s| s.contains(&format!("    {name}(")) || s.contains(&format!("    {name} {{")))
    }

    /// Identifier segments of an edit prefix: `Pattern(Len(` gives `Pattern`, `Len`.
    fn segments(prefix: &str) -> Vec<&str> {
        prefix
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect()
    }

    #[test]
    fn every_named_edit_exists_in_the_audio_edit_enums() {
        for c in DIAL_CELLS {
            if let DialTarget::Edit { edits, .. } = c.target {
                for alt in edits.split('|') {
                    for seg in segments(alt) {
                        assert!(variant_exists(seg), "{}/{}: no edit {seg}", c.page, c.dial);
                    }
                }
            }
        }
        assert!(!variant_exists("NoSuchEdit"));
    }

    #[test]
    fn every_page_has_a_left_cell_and_dials_are_unique_per_page() {
        let mut seen = std::collections::HashSet::new();
        for c in DIAL_CELLS {
            assert!(c.dial < 3 && !c.label.is_empty());
            assert!(seen.insert((c.page, c.dial)), "{}/{} twice", c.page, c.dial);
        }
        let pages: std::collections::BTreeSet<_> = DIAL_CELLS.iter().map(|c| c.page).collect();
        assert_eq!(pages.len(), 16);
        for p in pages {
            assert!(seen.contains(&(p, 0)), "{p} has no left dial");
        }
    }

    #[test]
    fn reasons_are_written_out() {
        for c in DIAL_CELLS {
            match c.target {
                DialTarget::Ui(w) | DialTarget::Gap(w) => assert!(w.len() > 10, "{}", c.page),
                DialTarget::Edit { edits, .. } => assert!(!edits.is_empty(), "{}", c.page),
            }
        }
    }

    #[test]
    fn columns_group_by_dial_and_unknown_tails_are_empty() {
        let (l, m, r) = (column("left"), column("middle"), column("right"));
        assert_eq!(l.len() + m.len() + r.len(), DIAL_CELLS.len());
        assert!(l.iter().all(|c| c.dial == 0) && r.iter().all(|c| c.dial == 2));
        assert!(column("up").is_empty());
        assert_eq!(cells_of("dial:middle").len(), m.len());
        assert!(cells_of("synth_osc_mode").is_empty());
    }

    #[test]
    fn cell_kinds_are_pinned() {
        let n = |f: fn(&DialTarget) -> bool| DIAL_CELLS.iter().filter(|c| f(&c.target)).count();
        assert_eq!(n(|t| matches!(t, DialTarget::Edit { .. })), 34);
        assert_eq!(n(|t| matches!(t, DialTarget::Ui(_))), 10);
        assert_eq!(n(|t| matches!(t, DialTarget::Gap(_))), 1);
        assert_eq!(DIAL_CELLS.len(), 45);
    }

    #[test]
    fn only_the_sample_option_dial_is_a_gap() {
        let gaps: Vec<_> = DIAL_CELLS
            .iter()
            .filter(|c| matches!(c.target, DialTarget::Gap(_)))
            .map(|c| (c.page, c.dial))
            .collect();
        assert_eq!(gaps, [("pitch_smp.sample", 2)]);
    }
}
