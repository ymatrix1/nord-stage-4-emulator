//! Dropdown choice tables of the firmware bar: value, entry name, per-entry status hint.

use nord_core::fx::RingAdvance;
use nord_core::organ::{OutputLevel, ToneWheelSource};
use nord_core::render_opts::RotaryDrive;
use nord_core::synth::{FilterEngine, PSpec, PTableSpec, SynthChain};

/// Dropdown entry: value, name, per-choice status hint shown when hovering the entry.
pub(super) type Choice<T> = (T, &'static str, &'static str);

pub(super) const TONES: [Choice<Option<ToneWheelSource>>; 6] = [
    (None, "Model default", "Per-model default source"),
    (Some(ToneWheelSource::Legacy), "Legacy", "ASSUMED"),
    (
        Some(ToneWheelSource::Firmware),
        "Pipe (firmware)",
        "VERIFIED",
    ),
    (
        Some(ToneWheelSource::B3Wheels),
        "B3 wheels (firmware)",
        "VERIFIED ladder, per-key weights not modelled",
    ),
    (
        Some(ToneWheelSource::Vox),
        "Vox bank (firmware)",
        "partial, processors not modelled",
    ),
    (
        Some(ToneWheelSource::FarfisaWheels),
        "Farfisa (firmware)",
        "partial, processors not modelled",
    ),
];
pub(super) const TAILS: [Choice<Option<bool>>; 2] = [
    (None, "On (default)", "EMPIRICAL"),
    (Some(false), "Off", "EMPIRICAL"),
];
pub(super) const ROTARIES: [Choice<RotaryDrive>; 3] = [
    (
        RotaryDrive::Engine,
        "Engine",
        "tanh(g x), program drive as stored",
    ),
    (RotaryDrive::Unity, "Unity", "tanh(g x) / g, EMPIRICAL"),
    (RotaryDrive::Scaled, "Scaled /10", "drive / 10, EMPIRICAL"),
];
pub(super) const ORGAN_LEVELS: [Choice<OutputLevel>; 3] = [
    (
        OutputLevel::ModelDefault,
        "Model default",
        "firmware for Pipe, legacy otherwise",
    ),
    (OutputLevel::Legacy, "Legacy", "bus scale 0.08"),
    (
        OutputLevel::Firmware,
        "Firmware",
        "swell word, EMPIRICAL gain K",
    ),
];
pub(super) const CHAINS: [Choice<Option<SynthChain>>; 2] = [
    (Some(SynthChain::Legacy), "Classic", "engine chain"),
    (
        Some(SynthChain::FirmwareStrip),
        "Firmware strip (default)",
        "partly VERIFIED, P table EMPIRICAL",
    ),
];
pub(super) const P_TABLES: [Choice<Option<PSpec>>; 3] = [
    (
        Some(PSpec::Table(PTableSpec::BUILTIN)),
        "Per-wave table (default)",
        "EMPIRICAL, docs/research/synth-validation.md",
    ),
    (
        Some(PSpec::Fitted),
        "Smooth Lead fit",
        "EMPIRICAL, one program",
    ),
    (None, "Off (unit P)", "no level or cutoff correction"),
];
pub(super) const FILTERS: [Choice<Option<FilterEngine>>; 2] = [
    (None, "ZDF (default)", "engine filter"),
    (Some(FilterEngine::Firmware), "Firmware", "VERIFIED"),
];
pub(super) const LEVELS: [Choice<bool>; 2] = [
    (false, "Legacy", "touch curve"),
    (true, "Firmware", "VERIFIED level model for .npno"),
];
pub(super) const AMPS: [Choice<Option<bool>>; 3] = [
    (None, "Default (firmware)", "VERIFIED drive stage"),
    (Some(false), "Engine", "tanh drive and RBJ cabinet"),
    (Some(true), "Firmware", "VERIFIED drive stage"),
];
pub(super) const LESLIES: [Choice<bool>; 2] = [
    (false, "Engine rotary", "engine rotary speaker"),
    (true, "Firmware", "ported firmware Leslie, EMPIRICAL ring"),
];
pub(super) const RINGS: [Choice<RingAdvance>; 3] = [
    (
        RingAdvance::ADVANCE,
        "Advance (default)",
        "4 words per call, never re-based: fits the B3 clip",
    ),
    (
        RingAdvance::PASS_RESET8,
        "Firmware",
        "re-based every 8 calls like the booted pass routine",
    ),
    (
        RingAdvance::FROZEN,
        "Frozen",
        "first oracle state, about 50 dB low",
    ),
];
pub(super) const KERNELS: [Choice<bool>; 2] = [
    (false, "Ratio (default)", "ASSUMED ratio rule"),
    (true, "Key", "firmware key rule, key field ASSUMED"),
];
pub(super) const PITCHES: [Choice<bool>; 2] = [
    (false, "Equal (default)", "exact 2^(n/12)"),
    (
        true,
        "Firmware",
        "ARM exponent tables, root and rate ASSUMED",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leslie_ring_choices_cover_the_three_ring_behaviours_once() {
        let rings: Vec<RingAdvance> = RINGS.iter().map(|c| c.0).collect();
        for r in [
            RingAdvance::ADVANCE,
            RingAdvance::PASS_RESET8,
            RingAdvance::FROZEN,
        ] {
            assert_eq!(rings.iter().filter(|x| **x == r).count(), 1);
        }
        assert_eq!(LESLIES.len() + KERNELS.len() + PITCHES.len(), 6);
        assert!(TONES.iter().all(|c| !c.1.is_empty() && !c.2.is_empty()));
    }
}
