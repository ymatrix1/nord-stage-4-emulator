//! Shared constants and enums of the organ engine.
//!
//! Spec: `docs/specs/organ-engine.md`

use serde::{Deserialize, Serialize};

/// Number of physical tonewheels in a Hammond B3 generator (`1..=91`).
pub const NUM_TONEWHEELS: usize = 91;

/// Maximum delay buffer capacity in samples for Hammond scanner vibrato (~10 ms at 192 kHz).
pub(super) const SCANNER_BUFFER_SIZE: usize = 2048;

/// Fixed stereo delay line capacity in samples for Model 122 Doppler & cabinet reflections (~10 ms at 48 kHz).
pub(super) const ROTARY_BUFFER_SIZE: usize = 1024;

/// Synchronous AC drive motor reference frequency (1200 RPM = 20 rev/sec).
pub(super) const HAMMOND_MOTOR_HZ: f32 = 20.0;

/// Authentic Hammond 12-tone drive/driven integer gear tooth ratios (`C` through `B`).
///
/// Top-octave tonewheels (`tw >= 84`) use physical 192-tooth wheels paired with a 4/3 step-up gear
/// (effective `256` pulses per motor-ratio cycle).
pub const HAMMOND_GEAR_RATIOS: [(u32, u32); 12] = [
    (85, 104),  // C
    (71, 82),   // C#
    (67, 73),   // D
    (105, 108), // D# (35/36)
    (103, 100), // E
    (84, 77),   // F (12/11)
    (74, 64),   // F# (37/32)
    (98, 80),   // G (49/40)
    (96, 74),   // G# (48/37)
    (88, 64),   // A (11/8 => 440.000 Hz at A4)
    (67, 46),   // A#
    (108, 70),  // B (54/35)
];

/// Hardware 800 Hz rotary bass/horn crossover verified from NS4 OS v1.66 (`ARM0 @ 0x0db130`).
pub const ROTARY_CROSSOVER_HZ: f32 = 800.0;

/// Hardware Rotary Bass/Horn balance menu steps (`70/30 .. 30/70`) verified from OS v1.66 (`0x2c20d8`).
pub const ROTARY_BASS_HORN_BALANCES: [(f32, f32); 5] = [
    (0.70, 0.30),
    (0.60, 0.40),
    (0.50, 0.50),
    (0.40, 0.60),
    (0.30, 0.70),
];

/// Supported Organ models in the Nord Stage 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganModel {
    B3,
    Vox,
    Farfisa,
    Pipe1,
    Pipe2,
}

/// Hammond mechanical scanner vibrato and chorus modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScannerMode {
    #[default]
    Off,
    V1,
    V2,
    V3,
    C1,
    C2,
    C3,
}

/// Rotary speaker speed state (`Slow` Chorale vs `Fast` Tremolo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotarySpeed {
    Stop,
    Slow,
    Fast,
}

/// Source of the `B3` tonewheel waveform and phase arithmetic.
///
/// Default is [`ToneWheelSource::Legacy`]: the algorithm of the `Firmware` bank is VERIFIED against
/// the SHARC oracle, but its ROLE as the organ tone source is ASSUMED and disputed (about 65 %
/// piano string resonance, 35 % `B3` tonewheels; the bank is installed by the PNO Core1 piano
/// state machine, by no ORG/Core2 module id). The organ tone source is still unlocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToneWheelSource {
    /// PNO Core1 oscillator-bank parity: `u32` phase, `phase >> 20` into the 4096-point firmware
    /// `x` table, no interpolation (`docs/research/organ-tonewheel-bank.md`). Opt-in: algorithm
    /// VERIFIED, role as organ tonewheels ASSUMED/disputed.
    Firmware,
    /// Pre-bank waveform: float phase, sine plus ad hoc low-wheel harmonics (ASSUMED).
    #[default]
    Legacy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_table_and_defaults() {
        assert_eq!(HAMMOND_GEAR_RATIOS.len(), 12);
        assert_eq!(HAMMOND_GEAR_RATIOS[9], (88, 64));
        assert_eq!(ToneWheelSource::default(), ToneWheelSource::Legacy);
        assert_eq!(ScannerMode::default(), ScannerMode::Off);
        assert_eq!(ROTARY_CROSSOVER_HZ, 800.0);
        assert_eq!(ROTARY_BASS_HORN_BALANCES[0], (0.70, 0.30));
        assert_eq!(ROTARY_BASS_HORN_BALANCES[4], (0.30, 0.70));
        assert_eq!(NUM_TONEWHEELS, 91);
    }
}
