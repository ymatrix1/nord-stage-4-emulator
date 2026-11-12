//! Synth engine mode enums (oscillator, filter, LFO, voice priority, vibrato, arpeggiator).

use serde::{Deserialize, Serialize};

/// Oscillator synthesis modes in the Nord Stage 4 Synth Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthOscMode {
    Analog,
    Wavetable,
    Fm,
    Sample,
}

/// Virtual Analog waveform shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaWaveform {
    Saw,
    SuperSaw,
    Pulse,
    Triangle,
    Sine,
}

/// Zero-Delay Feedback filter topologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterModel {
    Lp12,
    Lp24,
    Ladder24,
    BandPass,
    HighPass,
}

/// Low Frequency Oscillator (LFO) waveform shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LfoShape {
    #[default]
    Triangle,
    Saw,
    Square,
    SampleAndHold,
    /// Ramp down (`Sawtooth 1` on the panel); `Saw` is the ramp up (`Sawtooth 2`).
    SawDown,
}

/// Destination parameter for LFO modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LfoDestination {
    #[default]
    FilterCutoff,
    OscPitch,
    OscCtrl,
    /// LFO off (no destination LED lit).
    Off,
}

/// Mono/Legato note priority (`voice priority` in the program file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotePriority {
    /// Last note priority.
    #[default]
    Off,
    /// Lowest held key sounds; higher keys are ignored.
    Lo,
    /// Highest held key sounds; lower keys are ignored.
    Hi,
}

/// Vibrato source (`vibrato mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VibratoMode {
    #[default]
    Off,
    On,
    /// Fades in after `vibrato_delay_sec` per voice.
    Delay,
    Wheel,
    Aftertouch,
    Pedal,
}

/// Arpeggiator & Gate play patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArpMode {
    #[default]
    Off,
    Up,
    Down,
    UpDown,
    Random,
    Poly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_neutral_panel_state() {
        assert_eq!(LfoShape::default(), LfoShape::Triangle);
        assert_eq!(LfoDestination::default(), LfoDestination::FilterCutoff);
        assert_eq!(NotePriority::default(), NotePriority::Off);
        assert_eq!(VibratoMode::default(), VibratoMode::Off);
        assert_eq!(ArpMode::default(), ArpMode::Off);
    }

    #[test]
    fn enums_roundtrip_through_json() {
        let json = serde_json::to_string(&(LfoShape::SawDown, LfoDestination::Off)).unwrap();
        let back: (LfoShape, LfoDestination) = serde_json::from_str(&json).unwrap();
        assert_eq!(back, (LfoShape::SawDown, LfoDestination::Off));
        let v: VibratoMode = serde_json::from_str("\"Delay\"").unwrap();
        assert_eq!(v, VibratoMode::Delay);
    }
}
