//! The organ output level law: one gain stage shared by every firmware organ path (Pipe bank,
//! `B3Wheels`, Vox) and the number the Legacy bus is compared against.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.7. Research: `docs/research/organ-output-law.md`.
//!
//! The level the PNO core 1 chain leaves in its output lanes is `K * swell * level^2` times the
//! oracle-faithful bank output:
//!
//! | Factor | Source | Tag |
//! |---|---|---|
//! | `swell` word `520 v^2 / 2^23` | ARM sender `0xC0223B90` (edit id 2, `Fnc_Swell`, `v` in 0..127) | VERIFIED |
//! | layer level `(v / 127)^2` | ARM handler `0xC011AB8C` (`0xB1` / `0xB2`) | VERIFIED |
//! | `K`, the gain between the oracle lanes and the FX chain input | three video clips | EMPIRICAL |

use super::vox_source::OUTPUT_LEVEL as VOX_BAKED_SWELL;

/// Which level law the firmware organ paths use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputLevel {
    /// `Firmware` for a Pipe layer on the firmware bank (oracle lines 0.00 dB, video RMS +2.9 dB,
    /// D22), `Legacy` for every other source (`docs/research/organ-output-law.md` section 5).
    #[default]
    ModelDefault,
    /// The paths keep the levels they were built with (Legacy bus scale `0.08`, the Vox swell
    /// word of the boot, layer level applied linearly by the engine).
    Legacy,
    /// `K` times the swell word of the engine's pedal (neutral = full) on the oracle-faithful
    /// bank output, and the layer level squared by the engine.
    Firmware,
}

/// Default swell value of the boot (`Fnc_Swell`, `arm-edit-params.csv` default 100).
///
/// VERIFIED: the boot sends `v = 100` through the sender `0xC0223B90` (code hook on the callback
/// `0xC0223AEC`: arguments `(type 2, value 0x64)`), and `swell_word(100)` is `0x3F1EB100`, the
/// word the oracle gain bisection found (`organ-vox-click.md` section 3).
pub const SWELL_DEFAULT: u8 = 100;

/// The swell gain word the ARM sender `0xC0223B90` writes for the pedal value `v` (0..127):
/// `520 v^2 * 2^-23`, and `0x7FFFFF * 2^-23` for `v = 127`.
///
/// VERIFIED: disassembly (`r3 = (v * v * 65) << 3`, int to float with the `2^-23` scale of the
/// sender `0xC001FF18`, literal `0x7FFFFF` on the `v == 127` branch) and the boot word at
/// `v = 100`: `520 * 10000 * 2^-23 = 0.6198883 = 0x3F1EB100` (bit exact, test below).
pub fn swell_word(v: u8) -> f32 {
    let v = v.min(127);
    let n = if v == 127 {
        0x7F_FFFF
    } else {
        520 * u32::from(v) * u32::from(v)
    };
    n as f32 / 8_388_608.0
}

/// The gain the organ layer level `level` (0..1, `v / 127`) applies in the firmware: the handler
/// `0xC011AB8C` multiplies the q16 value by `1 / (127 * 65536)` and squares it.
///
/// VERIFIED: disassembly (`fmul` by `0x34010204` = 1.2014794e-7 = `1 / (127 * 65536)`, then
/// `fmul` of the result with itself, stored at `[table + 4 (7 layer + slot + 0x34)]`). A level of
/// zero stores the sentinel `-0.05` (`0xBD4CCCCD`) that mutes the layer; here it is 0.
pub fn layer_level_gain(level: f32) -> f32 {
    let x = level.clamp(0.0, 1.0);
    x * x
}

/// Gain between the oracle lanes (what `sharc-pass` and the Rust bank paths produce at swell and
/// layer level 1) and the FX chain input, `+8.9 dB`.
///
/// EMPIRICAL: the structure above is firmware, this scale is not. Needed gain on the three
/// hardware clips at swell 127 and the squared layer level: Vox `+9.9 dB` (video RMS -13.7 dB at
/// the boot swell word 0.6199, `+14.0 dB` needed, `-4.15 dB` of it the swell word), B3 `+11.3 dB`
/// (routing, tail, rotary drive in the firmware unit, level `0.795^2`), Pipe at most `+8.2 dB`
/// (the legacy bus is `+14.39 dB` over the ARM weight unit and the organ alone is `+8.5 dB` over
/// the clip, which also holds the synth). The spread is +-1.6 dB around 3.0 (K = 3: B3 -2.1, Vox -0.1, Pipe +3.5 dB); the clips fix neither
/// the performance velocity nor the pedal. 2.8 centres the RMS errors of the three clips (B3
/// -2.7, Vox -0.7, Pipe +2.9 dB). Open: the FX chain input trim of the organ unit.
pub const ORGAN_BUS_GAIN: f32 = 2.8;

/// `+14.39 dB`: the Legacy drawbar bus `0.08` against the ARM weight unit `0.01526` (`L = 1`),
/// the factor the Legacy organ paths carry instead of [`ORGAN_BUS_GAIN`] and the level law.
///
/// ASSUMED: the Legacy `0.08` has no firmware source (`pipe-video-gap.md` section 1).
pub const LEGACY_BUS_OVER_UNIT: f32 = 0.08 / super::weights::UNIT_WEIGHT;

impl super::OrganLayer {
    /// `true` when the drawbar weights are the smoothed firmware weights: `WeightMode::Firmware`,
    /// or a Pipe layer on the firmware bank under the [`OutputLevel::Firmware`] law (the bank and
    /// mix stage match the oracle on every line in both weight modes, `pipe_lines.rs`, so the law
    /// takes the weight mode whose scale is the verified ARM unit).
    pub(super) fn firmware_weights(&self) -> bool {
        use super::{OrganModel, ToneWheelSource, WeightMode};
        self.weight_mode == WeightMode::Firmware
            || (self.level_law() == OutputLevel::Firmware
                && matches!(self.model, OrganModel::Pipe1 | OrganModel::Pipe2)
                && self.effective_tone_source() == ToneWheelSource::Firmware)
    }

    /// The level law in force: `Firmware` or `Legacy`, with [`OutputLevel::ModelDefault`]
    /// resolved (Pipe on the firmware bank takes the firmware law).
    pub fn level_law(&self) -> OutputLevel {
        use super::{OrganModel, ToneWheelSource};
        match self.output_level {
            OutputLevel::ModelDefault => {
                if matches!(self.model, OrganModel::Pipe1 | OrganModel::Pipe2)
                    && self.effective_tone_source() == ToneWheelSource::Firmware
                {
                    OutputLevel::Firmware
                } else {
                    OutputLevel::Legacy
                }
            }
            law => law,
        }
    }
}

impl OutputLevel {
    /// Multiplier on a firmware path whose own output is at swell and layer level 1 (the weight
    /// mix, the `B3Wheels` sum).
    pub fn bus_gain(self) -> f32 {
        match self {
            Self::Legacy | Self::ModelDefault => 1.0,
            Self::Firmware => ORGAN_BUS_GAIN,
        }
    }

    /// Multiplier on the Vox output, which already carries the boot swell word.
    pub fn vox_gain(self) -> f32 {
        match self {
            Self::Legacy | Self::ModelDefault => 1.0,
            Self::Firmware => ORGAN_BUS_GAIN / VOX_BAKED_SWELL,
        }
    }

    /// Gain the engine applies for an organ layer level (linear in the Legacy law).
    pub fn layer_gain(self, level: f32) -> f32 {
        match self {
            Self::Legacy | Self::ModelDefault => level,
            Self::Firmware => layer_level_gain(level),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swell_word_is_the_boot_word_at_the_default_pedal() {
        // VERIFIED: the boot writes 0x3F1EB100 for Fnc_Swell = 100
        let w = swell_word(SWELL_DEFAULT);
        assert_eq!(w.to_bits(), 0x3F1E_B100);
        assert!((w - 0.619_888_3).abs() < 1e-7);
        assert!(
            (w - VOX_BAKED_SWELL).abs() < 2e-5,
            "the Vox constant is this word"
        );
    }

    #[test]
    fn swell_word_is_full_scale_at_127_and_squares_in_between() {
        assert!((swell_word(127) - 1.0).abs() < 1e-6);
        assert_eq!(swell_word(0), 0.0);
        let ratio = swell_word(64) / swell_word(32);
        assert!((ratio - 4.0).abs() < 1e-5, "{ratio}");
        assert_eq!(swell_word(200), swell_word(127));
    }

    #[test]
    fn layer_level_gain_squares_the_level() {
        assert_eq!(layer_level_gain(0.0), 0.0);
        assert_eq!(layer_level_gain(1.0), 1.0);
        // program default 101 / 127 = 0.795, the squared level the B3 program carries
        assert!((layer_level_gain(101.0 / 127.0) - 0.632_463).abs() < 1e-5);
        assert_eq!(layer_level_gain(-1.0), 0.0);
        assert_eq!(layer_level_gain(2.0), 1.0);
    }

    #[test]
    fn legacy_is_unity_and_firmware_is_the_bus_gain() {
        assert_eq!(OutputLevel::Legacy.bus_gain(), 1.0);
        assert_eq!(
            OutputLevel::ModelDefault.bus_gain(),
            1.0,
            "resolved by level_law first"
        );
        assert_eq!(OutputLevel::Legacy.vox_gain(), 1.0);
        assert_eq!(OutputLevel::Legacy.layer_gain(0.8), 0.8);
        assert_eq!(OutputLevel::Firmware.bus_gain(), ORGAN_BUS_GAIN);
        let vox = OutputLevel::Firmware.vox_gain();
        assert!(
            (vox * VOX_BAKED_SWELL - ORGAN_BUS_GAIN).abs() < 1e-6,
            "swell word removed"
        );
        assert!((OutputLevel::Firmware.layer_gain(0.5) - 0.25).abs() < 1e-7);
    }

    #[test]
    fn firmware_level_switches_a_pipe_layer_to_the_firmware_weights() {
        use super::super::{OrganLayer, OrganModel};
        let mut o = OrganLayer::new(48_000.0);
        o.model = OrganModel::Pipe2;
        assert!(
            o.firmware_weights(),
            "Pipe on the firmware bank is the model default"
        );
        o.output_level = OutputLevel::Legacy;
        assert!(!o.firmware_weights());
        o.output_level = OutputLevel::Firmware;
        assert!(o.firmware_weights());
        o.model = OrganModel::B3;
        assert!(!o.firmware_weights(), "B3 keeps its own weight path");
        o.output_level = OutputLevel::ModelDefault;
        assert_eq!(o.level_law(), OutputLevel::Legacy);
        o.model = OrganModel::Pipe1;
        assert_eq!(o.level_law(), OutputLevel::Firmware);
    }

    #[test]
    fn firmware_level_scales_a_pipe_chord_by_the_unit_and_bus_gain() {
        use super::super::{OrganLayer, OrganModel};
        let render = |level: OutputLevel| {
            let mut o = OrganLayer::new(48_000.0);
            o.model = OrganModel::Pipe2;
            o.rotary_enabled = false;
            o.key_click_enabled = false;
            o.output_level = level;
            o.note_on(60);
            let mut peak = 0.0f32;
            for _ in 0..24_000 {
                peak = peak.max(o.render_frame()[0].abs());
            }
            peak
        };
        let (legacy, firmware) = (render(OutputLevel::Legacy), render(OutputLevel::Firmware));
        let expect = LEGACY_BUS_OVER_UNIT.recip() * ORGAN_BUS_GAIN;
        assert!(legacy > 0.0 && firmware > 0.0);
        let ratio = firmware / legacy;
        assert!(
            (ratio / expect - 1.0).abs() < 0.2,
            "ratio {ratio} expect {expect}"
        );
    }

    #[test]
    fn engine_squares_the_organ_layer_level_under_the_firmware_law() {
        use crate::engine::Stage4Engine;
        use crate::midi::MidiEvent;
        let rms = |level: OutputLevel| {
            let mut e = Stage4Engine::new(48_000.0);
            e.layer_levels[0].base = 0.5;
            e.layer_levels[1].base = 0.0;
            e.organ_a.output_level = level;
            e.organ_a.rotary_enabled = false;
            e.organ_a.key_click_enabled = false;
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 8,
            });
            let acc: f32 = (0..24_000).map(|_| e.render_frame()[0].powi(2)).sum();
            (acc / 24_000.0).sqrt()
        };
        let (legacy, firmware) = (rms(OutputLevel::Legacy), rms(OutputLevel::Firmware));
        assert!(legacy > 1e-6, "the organ layer is audible");
        let ratio = firmware / legacy;
        assert!(
            (ratio - 0.5).abs() < 0.02,
            "0.25 / 0.5 expected, got {ratio}"
        );
    }

    #[test]
    fn legacy_bus_is_14_4_db_over_the_weight_unit() {
        let db = 20.0 * LEGACY_BUS_OVER_UNIT.log10();
        assert!((db - 14.39).abs() < 0.01, "{db}");
    }
}
