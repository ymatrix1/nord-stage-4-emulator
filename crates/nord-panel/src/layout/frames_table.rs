//! The table of printed frames (`FRAMES`), grouped by section; see `frames.rs` for the types.

use super::frames::{FrameDef, FrameKind};
use crate::types::Section;

const fn boxed(
    id: &'static str,
    section: Section,
    title: &'static str,
    members: &'static [&'static str],
    pad: [f32; 4],
) -> FrameDef {
    FrameDef {
        id,
        section,
        title,
        kind: FrameKind::Box,
        members,
        pad,
        title_x: 0.5,
    }
}

const fn bare(
    id: &'static str,
    section: Section,
    title: &'static str,
    members: &'static [&'static str],
    pad: [f32; 4],
) -> FrameDef {
    FrameDef {
        id,
        section,
        title,
        kind: FrameKind::Title,
        members,
        pad,
        title_x: 0.5,
    }
}

use Section::{Effects, Organ, Piano, Program, Synth};

/// Every frame, grouped by section.
pub const FRAMES: &[FrameDef] = &[
    // Organ.
    boxed(
        "organ.frame.model",
        Organ,
        "ORGAN MODEL",
        &["organ.model", "organ.model_leds"],
        [20.0, 10.0, 16.0, 6.0],
    ),
    boxed(
        "organ.frame.vib",
        Organ,
        "VIB/CHORUS",
        &[
            "organ.vibrato_selector",
            "organ.vibrato_on.a",
            "organ.vibrato_on.b",
        ],
        [3.0, 20.0, 3.0, 6.0],
    ),
    boxed(
        "organ.frame.perc",
        Organ,
        "B3 PERCUSSION",
        &[
            "organ.perc_volume",
            "organ.perc_decay",
            "organ.perc_third",
            "organ.perc_on",
        ],
        [10.0, 22.0, 7.0, 8.0],
    ),
    // Piano.
    bare(
        "piano.frame.acoustics",
        Piano,
        "ACOUSTICS",
        &["piano.soft_release"],
        [0.0, 10.0, 0.0, 0.0],
    ),
    bare(
        "piano.frame.unison",
        Piano,
        "UNISON",
        &["piano.unison"],
        [0.0, 10.0, 0.0, 0.0],
    ),
    bare(
        "piano.frame.kb_touch",
        Piano,
        "KB TOUCH",
        &["piano.kb_touch"],
        [0.0, 10.0, 0.0, 0.0],
    ),
    bare(
        "piano.frame.dyn_comp",
        Piano,
        "DYN COMP",
        &["piano.dyn_comp"],
        [0.0, 10.0, 0.0, 0.0],
    ),
    boxed(
        "piano.frame.select",
        Piano,
        "PIANO SELECT",
        &["piano.type_leds", "piano.type", "piano.select_dial"],
        [0.0, 8.0, 0.0, 12.0],
    ),
    // Synth.
    boxed(
        "synth.frame.mode",
        Synth,
        "MODE",
        &["synth.mode"],
        [4.0, 14.0, 4.0, 12.0],
    ),
    boxed(
        "synth.frame.arp",
        Synth,
        "ARPEGGIATOR / GATE",
        &[
            "synth.arp.rate",
            "synth.arp.mode",
            "synth.arp.range",
            "synth.arp.menu",
        ],
        [8.0, 14.0, 2.0, 12.0],
    ),
    boxed(
        "synth.frame.voice",
        Synth,
        "VOICE",
        &["synth.voice.mode", "synth.voice.glide"],
        [6.0, 14.0, 10.5, 8.0],
    ),
    boxed(
        "synth.frame.vibrato",
        Synth,
        "VIBRATO",
        &["synth.vibrato.selector", "synth.vibrato.menu"],
        [5.0, 14.0, 4.0, 8.0],
    ),
    boxed(
        "synth.frame.lfo",
        Synth,
        "LFO",
        &[
            "synth.lfo.waveform",
            "synth.lfo.rate",
            "synth.lfo.mod_amt",
            "synth.lfo.dest",
        ],
        [8.0, 14.0, 11.0, 12.0],
    ),
    boxed(
        "synth.frame.osc",
        Synth,
        "OSCILLATORS",
        &[
            "synth.osc.pitch_smp",
            "synth.osc.envelope",
            "synth.osc.ctrl",
            "synth.osc.env_amt",
        ],
        [3.0, 14.0, 11.0, 12.0],
    ),
    boxed(
        "synth.frame.filter",
        Synth,
        "FILTER",
        &[
            "synth.filter.type",
            "synth.filter.envelope",
            "synth.filter.env_amt",
            "synth.filter.freq",
            "synth.filter.res",
        ],
        [4.0, 14.0, 10.5, 12.0],
    ),
    boxed(
        "synth.frame.amp",
        Synth,
        "AMP",
        &["synth.amp.envelope"],
        [4.0, 14.0, 4.0, 12.0],
    ),
    boxed(
        "synth.frame.unison",
        Synth,
        "UNISON",
        &["synth.unison"],
        [4.0, 14.0, 4.0, 8.0],
    ),
    // Layer Effects.
    boxed(
        "fx.frame.mod1",
        Effects,
        "MOD 1",
        &["mod1.rate", "mod1.amount", "mod1.selector", "mod1.on"],
        [6.0, 4.6, 6.0, 7.0],
    )
    .at(0.80),
    boxed(
        "fx.frame.mod2",
        Effects,
        "MOD 2",
        &["mod2.rate", "mod2.amount", "mod2.selector", "mod2.on"],
        [6.0, 9.0, 6.0, 6.0],
    ),
    boxed(
        "fx.frame.amp",
        Effects,
        "AMP SIM/EQ",
        &[
            "amp.drive",
            "amp.mid_freq",
            "amp.model",
            "amp.on",
            "amp.bass",
            "amp.mid",
            "amp.treble",
        ],
        [12.0, 12.0, 4.0, 8.0],
    )
    .at(0.25),
    boxed(
        "fx.frame.delay",
        Effects,
        "DELAY",
        &[
            "delay.tempo",
            "delay.feedback",
            "delay.effects",
            "delay.filter",
            "delay.dry_wet",
            "delay.tap_set",
            "delay.on",
        ],
        [6.0, 8.0, 10.0, 10.0],
    ),
    boxed(
        "fx.frame.comp",
        Effects,
        "COMP",
        &["comp.amount", "comp.on"],
        [8.0, 14.0, 12.0, 12.0],
    ),
    boxed(
        "fx.frame.reverb",
        Effects,
        "REVERB",
        &[
            "reverb.bright_dark",
            "reverb.type",
            "reverb.dry_wet",
            "reverb.on",
        ],
        [0.5, 14.0, 6.0, 12.0],
    ),
    // Program area (the red central block).
    boxed(
        "program.frame.morph",
        Program,
        "MORPH ASSIGN",
        &["morph.wheel", "morph.at", "morph.ctrlped"],
        [4.0, 14.0, 4.0, 12.0],
    ),
    boxed(
        "program.frame.split",
        Program,
        "SPLIT",
        &["split.on_set", "split.leds"],
        [6.0, 12.0, 6.0, 8.0],
    ),
    boxed(
        "program.frame.mst_clk",
        Program,
        "MST CLK",
        &["mst_clk.tap_set"],
        [4.0, 14.0, 4.0, 12.0],
    ),
    boxed(
        "program.frame.transp",
        Program,
        "TRANSP",
        &["transp.on_set"],
        [4.0, 14.0, 4.0, 10.0],
    ),
    boxed(
        "program.frame.preset_library",
        Program,
        "PRESET LIBRARY",
        &[
            "preset_library.organ",
            "preset_library.piano",
            "preset_library.synth",
        ],
        [4.0, 14.0, 4.0, 12.0],
    ),
    boxed(
        "program.frame.program",
        Program,
        "PROGRAM",
        &[
            "program.oled",
            "program.dial",
            "program.button.1",
            "program.button.4",
            "program.button.5",
            "program.button.8",
            "program.page_down",
            "program.page_up",
        ],
        [0.0, 1.0, 4.0, 12.0],
    )
    .at(0.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_a_frame_for_every_framed_section() {
        for s in [
            Section::Organ,
            Section::Piano,
            Section::Synth,
            Section::Effects,
            Section::Program,
        ] {
            assert!(FRAMES.iter().any(|f| f.section == s), "{s:?}");
        }
        assert_eq!(FRAMES.len(), 29);
    }

    #[test]
    fn titles_are_printed_in_capitals() {
        for f in FRAMES {
            assert_eq!(f.title, f.title.to_uppercase(), "{}", f.id);
        }
    }
}
