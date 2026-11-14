//! Name tables of the Synth display: filter types, LFO shapes, arpeggiator (manual p.29-35).
//! The oscillator category tables live in `waves.rs`.

use nord_core::synth::FilterModel;
use nord_core::synth_types::{LfoDestination, LfoShape, NotePriority, VibratoMode};

pub use super::waves::{
    fm_cat_name, harmonic_partial, inharmonic_semitones, table_flat_index, TableCat, TableEntry,
    WaveCat, WaveEntry, ANALOG, FM_GROUPS, HARMONIC_COUNT, INHARMONIC_COUNT, WAVE_TABLES,
    WAVE_TYPES,
};

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
}
