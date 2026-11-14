//! Name tables of the Synth display: waveform categories, filter types, LFO shapes (manual p.29-35).
//!
//! Names are VERIFIED against the manual text; which engine waveform each name drives is ASSUMED
//! (the engine has five virtual-analog shapes), so inexact entries carry `exact = false` and the
//! display marks them with `~`.

use nord_core::synth::{FilterModel, VaWaveform};
use nord_core::synth_types::{LfoDestination, LfoShape, NotePriority, VibratoMode};

/// One waveform of the ANALOG type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveEntry {
    pub name: &'static str,
    /// Nearest engine waveform.
    pub wave: VaWaveform,
    /// Fixed pulse duty (Pulse waveforms), if any.
    pub pulse: Option<f32>,
    /// The engine reproduces this waveform; `false` = nearest-shape stand-in.
    pub exact: bool,
}

/// A waveform category (middle dial).
#[derive(Debug, Clone, Copy)]
pub struct WaveCat {
    pub name: &'static str,
    pub waves: &'static [WaveEntry],
}

const fn w(name: &'static str, wave: VaWaveform, pulse: Option<f32>, exact: bool) -> WaveEntry {
    WaveEntry {
        name,
        wave,
        pulse,
        exact,
    }
}

use VaWaveform::{Pulse, Saw, Sine, SuperSaw, Triangle};

/// ANALOG type categories (manual p.29-30: Pure, Sub Osc, Shape, Shape Sine, Multi, Super, Misc).
pub const ANALOG: [WaveCat; 7] = [
    WaveCat {
        name: "PURE",
        waves: &[
            w("Sine", Sine, None, true),
            w("Triangle", Triangle, None, true),
            w("Saw", Saw, None, true),
            w("Square", Pulse, Some(0.5), true),
            w("Pulse 33", Pulse, Some(0.33), true),
            w("Pulse 10", Pulse, Some(0.10), true),
            w("White Noise", Saw, None, false),
        ],
    },
    WaveCat {
        name: "SUB OSC",
        waves: &[
            w("Sub Square Saw", Saw, None, false),
            w("Sub Pulse Saw", Saw, None, false),
            w("Sub Sine Saw", Saw, None, false),
        ],
    },
    WaveCat {
        name: "SHAPE",
        waves: &[
            w("Triangle", Triangle, None, false),
            w("Saw", Saw, None, false),
            w("Square", Pulse, None, false),
            w("Half Sine", Sine, None, false),
            w("Parabolic", Triangle, None, false),
        ],
    },
    WaveCat {
        name: "SHAPE SINE",
        waves: &[
            w("Pulse", Pulse, None, false),
            w("Saw", Saw, None, false),
            w("Square", Pulse, None, false),
            w("Squeeze", Sine, None, false),
            w("Fold", Sine, None, false),
            w("ESaw", Saw, None, false),
            w("ESquare", Pulse, None, false),
        ],
    },
    WaveCat {
        name: "MULTI",
        waves: &[
            w("Multi Saw", SuperSaw, None, false),
            w("Multi Saw 8ve", SuperSaw, None, false),
            w("Multi Saw 5th", SuperSaw, None, false),
            w("Multi Saw 5th+", SuperSaw, None, false),
            w("Multi Saw 8ve 8ve+", SuperSaw, None, false),
            w("Multi Saw 8ve 5th+", SuperSaw, None, false),
        ],
    },
    WaveCat {
        name: "SUPER",
        waves: &[
            w("Super Saw", SuperSaw, None, true),
            w("Super Square", SuperSaw, None, false),
            w("Super Bright", SuperSaw, None, false),
            w("Super Organ", SuperSaw, None, false),
        ],
    },
    WaveCat {
        name: "MISC",
        waves: &[
            w("Pink Noise", Saw, None, false),
            w("Red Noise", Saw, None, false),
            w("Bell", Sine, None, false),
        ],
    },
];

/// Oscillator TYPE dial entries (manual p.29). `FM-H` and `FM-I` both drive the engine FM mode.
pub const WAVE_TYPES: [&str; 4] = ["ANALOG", "FM-H", "FM-I", "WAVE"];

/// FM algorithm categories A-E (manual p.31).
pub const FM_ALGOS: [&str; 5] = ["ALGO A", "ALGO B", "ALGO C", "ALGO D", "ALGO E"];

/// Wavetables per WAVE category.
/// ASSUMED: the engine has 64 tables; the panel's category names are not decoded, so they are
/// grouped as eight categories of eight tables.
pub const TABLES_PER_CAT: u8 = 8;
/// Number of WAVE categories (`64 / TABLES_PER_CAT`).
pub const WAVE_CATS: u8 = 8;

/// Harmonic FM partial for index `i` (manual p.31: 0.5 to 24).
pub fn harmonic_partial(i: u8) -> f32 {
    if i == 0 {
        0.5
    } else {
        f32::from(i.min(24))
    }
}

/// Number of harmonic partials (0.5, 1 .. 24).
pub const HARMONIC_COUNT: u8 = 25;

/// Inharmonic FM pitch in semitones for index `i` (manual p.31: -12 to 48).
pub fn inharmonic_semitones(i: u8) -> i8 {
    i.min(60) as i8 - 12
}

/// Number of inharmonic pitches (-12 .. 48).
pub const INHARMONIC_COUNT: u8 = 61;

/// One filter TYPE entry.
#[derive(Debug, Clone, Copy)]
pub struct FilterType {
    pub name: &'static str,
    /// Engine model; `None` for LP+HP, which the engine does not have.
    pub model: Option<FilterModel>,
}

/// Filter types in panel order (manual p.31-32).
pub const FILTER_TYPES: [FilterType; 6] = [
    FilterType {
        name: "LP M",
        model: Some(FilterModel::Ladder24),
    },
    FilterType {
        name: "LP 24",
        model: Some(FilterModel::Lp24),
    },
    FilterType {
        name: "LP 12",
        model: Some(FilterModel::Lp12),
    },
    FilterType {
        name: "LP+HP",
        model: None,
    },
    FilterType {
        name: "HP",
        model: Some(FilterModel::HighPass),
    },
    FilterType {
        name: "BP",
        model: Some(FilterModel::BandPass),
    },
];

/// Index into [`FILTER_TYPES`] of an engine filter model.
pub fn filter_type_of(m: FilterModel) -> u8 {
    FILTER_TYPES
        .iter()
        .position(|t| t.model == Some(m))
        .unwrap_or(0) as u8
}

/// LFO waveforms in panel order: (name, engine shape). `Sawtooth 1` is the ramp down.
pub const LFO_SHAPES: [(&str, LfoShape); 5] = [
    ("Triangle", LfoShape::Triangle),
    ("Sawtooth 1", LfoShape::SawDown),
    ("Sawtooth 2", LfoShape::Saw),
    ("Square", LfoShape::Square),
    ("Sample&Hold", LfoShape::SampleAndHold),
];

/// LFO destinations in LED order (manual p.34).
pub const LFO_DESTS: [(&str, LfoDestination); 4] = [
    ("FILTER", LfoDestination::FilterCutoff),
    ("PITCH", LfoDestination::OscPitch),
    ("OSC CTRL", LfoDestination::OscCtrl),
    ("OFF", LfoDestination::Off),
];

/// Vibrato selector positions (manual p.37).
pub const VIBRATO_MODES: [(&str, VibratoMode); 6] = [
    ("OFF", VibratoMode::Off),
    ("ON", VibratoMode::On),
    ("DLY", VibratoMode::Delay),
    ("WHL", VibratoMode::Wheel),
    ("A.T.", VibratoMode::Aftertouch),
    ("PED", VibratoMode::Pedal),
];

/// Vibrato delay choices in seconds (manual p.37).
pub const VIBRATO_DELAYS: [f32; 4] = [0.2, 0.7, 1.2, 2.0];

/// Note priority positions: Last, Lo, Hi.
pub const PRIORITIES: [(&str, NotePriority); 3] = [
    ("LAST", NotePriority::Off),
    ("LO", NotePriority::Lo),
    ("HI", NotePriority::Hi),
];

/// Master Clock subdivision names for `ClockDivision` index `0..=9`.
pub const DIVISIONS: [&str; 10] = [
    "1/2", "1/2T", "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T",
];

/// Arpeggio direction names.
pub const ARP_DIRECTIONS: [&str; 4] = ["UP", "DOWN", "UP/DOWN", "RANDOM"];

/// Arpeggiator / Poly / Gate mode names.
pub const ARP_KINDS: [&str; 3] = ["ARP", "POLY", "GATE"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analog_categories_are_the_seven_of_the_manual() {
        let names: Vec<_> = ANALOG.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            [
                "PURE",
                "SUB OSC",
                "SHAPE",
                "SHAPE SINE",
                "MULTI",
                "SUPER",
                "MISC"
            ]
        );
        assert!(ANALOG.iter().all(|c| !c.waves.is_empty()));
    }

    #[test]
    fn pure_category_lists_the_seven_waveforms_of_manual_p29() {
        let n: Vec<_> = ANALOG[0].waves.iter().map(|e| e.name).collect();
        assert_eq!(
            n,
            [
                "Sine",
                "Triangle",
                "Saw",
                "Square",
                "Pulse 33",
                "Pulse 10",
                "White Noise"
            ]
        );
    }

    #[test]
    fn pulse_entries_carry_their_duty() {
        let p33 = ANALOG[0].waves[4];
        assert_eq!(p33.wave, VaWaveform::Pulse);
        assert_eq!(p33.pulse, Some(0.33));
    }

    #[test]
    fn fm_partials_and_pitches_span_the_manual_ranges() {
        assert_eq!(harmonic_partial(0), 0.5);
        assert_eq!(harmonic_partial(HARMONIC_COUNT - 1), 24.0);
        assert_eq!(inharmonic_semitones(0), -12);
        assert_eq!(inharmonic_semitones(INHARMONIC_COUNT - 1), 48);
    }

    #[test]
    fn filter_type_lookup_round_trips_supported_models() {
        for (i, t) in FILTER_TYPES.iter().enumerate() {
            if let Some(m) = t.model {
                assert_eq!(filter_type_of(m) as usize, i);
            }
        }
        assert!(FILTER_TYPES[3].model.is_none());
    }

    #[test]
    fn lfo_dest_and_vibrato_tables_cover_all_engine_values() {
        assert_eq!(LFO_DESTS.len(), 4);
        assert_eq!(VIBRATO_MODES.len(), 6);
        assert_eq!(LFO_SHAPES[1].1, LfoShape::SawDown);
        assert_eq!(DIVISIONS.len(), 10);
    }

    /// VERIFIED: the three SUB OSC names occur in `bank0_CODE.bin@0x2D7DEC..0x2D7E1C` (OS v1.66),
    /// checked by `tools/tests/test_ns4_oled_strings.py::test_fixed_wording_is_in_the_firmware`.
    #[test]
    fn sub_osc_names_use_the_firmware_word_order() {
        let sub = ANALOG.iter().find(|c| c.name == "SUB OSC").unwrap();
        let names: Vec<_> = sub.waves.iter().map(|w| w.name).collect();
        assert_eq!(names, ["Sub Square Saw", "Sub Pulse Saw", "Sub Sine Saw"]);
    }
}
