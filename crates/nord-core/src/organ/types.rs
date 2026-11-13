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

impl OrganModel {
    /// Value of `Edt_Org_kModel` in the firmware: `B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass` = 0..5
    /// (the B3 Bass model is [`OrganLayer::b3_bass_mode`](super::OrganLayer) on `B3` here).
    ///
    /// VERIFIED: the model to routine map (string pool `0xC02C75F4`, chain step `0xC022F060`,
    /// `docs/research/organ-model-paths.md`). ASSUMED: the value order matches the UI order.
    pub const fn firmware_value(self) -> u8 {
        match self {
            Self::B3 => 0,
            Self::Vox => 1,
            Self::Farfisa => 2,
            Self::Pipe1 => 3,
            Self::Pipe2 => 4,
        }
    }

    /// Tone source this model uses when [`ToneWheelSource::ModelDefault`] is selected.
    ///
    /// VERIFIED (model to routine map, `docs/research/organ-model-paths.md`): only the firmware
    /// models Pipe1 and Pipe2 install the 92-wheel bank (`0x122620`) and its mix stage
    /// (`0x12268F`); B3, Farfisa and B3 Bass install no generator in PNO Core1 and Vox installs
    /// the 84-oscillator bank `0x12272D`. Their tone source is not located (OPEN-QUESTIONS), so
    /// they keep the `Legacy` model (ASSUMED).
    pub const fn default_tone_source(self) -> ToneWheelSource {
        match self {
            Self::Pipe1 | Self::Pipe2 => ToneWheelSource::Firmware,
            Self::B3 | Self::Vox | Self::Farfisa => ToneWheelSource::Legacy,
        }
    }
}

/// Source of the tonewheel (oscillator bank) waveform and phase arithmetic.
///
/// Default is [`ToneWheelSource::ModelDefault`]: `Firmware` on the Pipe models, `Legacy` on the
/// others ([`OrganModel::default_tone_source`]).
///
/// CORRECTION (`docs/research/organ-model-paths.md`, VERIFIED): the 92-wheel bank is installed by
/// the firmware organ models Pipe1 / Pipe2 only. The earlier attribution to the B3 model is
/// disproved; the bank algorithm, ladder and weights stay VERIFIED against the SHARC oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToneWheelSource {
    /// Pick the source from the model ([`OrganModel::default_tone_source`]).
    #[default]
    ModelDefault,
    /// PNO Core1 oscillator-bank parity: `u32` phase, `phase >> 20` into the 4096-point firmware
    /// `x` table, no interpolation, exact 12-TET pitches (`docs/research/organ-tonewheel-bank.md`).
    /// Algorithm and ladder VERIFIED; the role is the Pipe organ bank (models Pipe1 / Pipe2).
    /// Forcing it on `B3` is an experiment only (attribution disproved); Vox and Farfisa ignore
    /// it (they use `Legacy`).
    Firmware,
    /// Pre-bank waveform: float phase, Hammond gear ratios, sine plus ad hoc low-wheel harmonics
    /// (ASSUMED).
    Legacy,
    /// B3 generator `0x121BBE` of PNO core 1: the firmware pitch ladder (12 oscillators, octave
    /// doubling, `ssfr` rounding; VERIFIED to 2.5e-11 against the oracle) and the measured wheel
    /// tables, one wheel per note. Opt-in: the per-key weights and per-note amplitudes of the
    /// firmware are not modelled yet (`docs/research/b3-generator.md`). Applies to `B3` only; the
    /// other models fall back to `Legacy`.
    B3Wheels,
}

impl ToneWheelSource {
    /// Source in force for `model`: `ModelDefault` resolves per model, an explicit `Firmware`
    /// applies to `B3` (experiment) and the Pipe models, and falls back to `Legacy` on Vox and
    /// Farfisa, which have no 92-wheel bank.
    pub const fn resolve(self, model: OrganModel) -> Self {
        match (self, model) {
            (Self::ModelDefault, m) => m.default_tone_source(),
            (Self::Firmware, OrganModel::Vox | OrganModel::Farfisa) => Self::Legacy,
            (Self::B3Wheels, m) if !matches!(m, OrganModel::B3) => Self::Legacy,
            (explicit, _) => explicit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_table_and_defaults() {
        assert_eq!(HAMMOND_GEAR_RATIOS.len(), 12);
        assert_eq!(HAMMOND_GEAR_RATIOS[9], (88, 64));
        assert_eq!(ToneWheelSource::default(), ToneWheelSource::ModelDefault);
        assert_eq!(ScannerMode::default(), ScannerMode::Off);
        assert_eq!(ROTARY_CROSSOVER_HZ, 800.0);
        assert_eq!(ROTARY_BASS_HORN_BALANCES[0], (0.70, 0.30));
        assert_eq!(ROTARY_BASS_HORN_BALANCES[4], (0.30, 0.70));
        assert_eq!(NUM_TONEWHEELS, 91);
    }

    #[test]
    fn model_to_source_mapping() {
        use OrganModel::*;
        let fw = [(B3, 0), (Vox, 1), (Farfisa, 2), (Pipe1, 3), (Pipe2, 4)];
        for (m, v) in fw {
            assert_eq!(m.firmware_value(), v);
        }
        for m in [B3, Vox, Farfisa] {
            assert_eq!(m.default_tone_source(), ToneWheelSource::Legacy);
            assert_eq!(
                ToneWheelSource::ModelDefault.resolve(m),
                ToneWheelSource::Legacy
            );
        }
        for m in [Pipe1, Pipe2] {
            assert_eq!(
                ToneWheelSource::ModelDefault.resolve(m),
                ToneWheelSource::Firmware
            );
            assert_eq!(ToneWheelSource::Legacy.resolve(m), ToneWheelSource::Legacy);
        }
        // explicit Firmware: B3 experiment honoured, Vox/Farfisa have no 92-wheel bank
        assert_eq!(
            ToneWheelSource::Firmware.resolve(B3),
            ToneWheelSource::Firmware
        );
        assert_eq!(
            ToneWheelSource::Firmware.resolve(Vox),
            ToneWheelSource::Legacy
        );
        assert_eq!(
            ToneWheelSource::Firmware.resolve(Farfisa),
            ToneWheelSource::Legacy
        );
    }
}
