//! Link between the organ's vibrato setting and the firmware bank's vibrato-enable level.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.4. Research: `docs/research/arm-organ-messages.md`
//! section 7, `docs/research/organ-tone-source.md` section 2.

use super::types::ScannerMode;
use serde::{Deserialize, Serialize};

/// Vibrato-enable level the ARM routine `0xC022D9F0(this, level)` receives.
///
/// VERIFIED: level 1 programs the detuned increment table and the stereo-blend gains, levels
/// 0, 2 and 3 the exact table and copy-0-on-both-lanes gains (`tools/arm_bank.py`). Which
/// levels occur on the instrument is not known; the organ handler stores `(value == 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VibratoLevel(pub u8);

impl VibratoLevel {
    /// The level that selects the detuned table.
    pub const DETUNED: Self = Self(1);

    /// `true` for the level that selects the detuned table and the stereo-blend gains.
    pub fn detuned(self) -> bool {
        self == Self::DETUNED
    }
}

/// Level for the organ's scanner setting: any vibrato or chorus mode is "vibrato enabled" (1),
/// `Off` is level 0.
///
/// ASSUMED: the ARM passes the layer's vibrato-enable byte (`Edt_Org_Preset_kVibratoEnable`,
/// `(value == 1)`), which is the switch the preset decoder already maps onto
/// `ScannerMode::Off` versus `V1..C3` (`cbin::organ_preset::scanner_mode`). That the V and C
/// types behave alike for the bank table is shape-supported only (the type selects post-stage
/// constants, not the bank).
pub fn level_for_scanner(mode: ScannerMode) -> VibratoLevel {
    if mode == ScannerMode::Off {
        VibratoLevel(0)
    } else {
        VibratoLevel::DETUNED
    }
}

/// How the organ's bank table and lane gains are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VibratoLink {
    /// Firmware behaviour (default): the vibrato-enable level follows `scanner_mode`
    /// ([`level_for_scanner`]; `detuned_copies` forces level 1), the group gains ramp with the
    /// VERIFIED one-pole smoother and the two lanes go to the stereo path.
    #[default]
    Firmware,
    /// Previous behaviour: `detuned_copies` alone selects the table, the gains apply instantly
    /// as a mono mean (`vibrato_bank::copy_weights`), the organ stays mono until the rotary.
    Manual,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_level_one_is_detuned() {
        assert!(VibratoLevel(1).detuned());
        for l in [0, 2, 3, 255] {
            assert!(!VibratoLevel(l).detuned());
        }
    }

    #[test]
    fn scanner_modes_map_to_levels() {
        assert_eq!(level_for_scanner(ScannerMode::Off), VibratoLevel(0));
        for m in [
            ScannerMode::V1,
            ScannerMode::V2,
            ScannerMode::V3,
            ScannerMode::C1,
            ScannerMode::C2,
            ScannerMode::C3,
        ] {
            assert!(level_for_scanner(m).detuned(), "{m:?}");
        }
        assert_eq!(VibratoLink::default(), VibratoLink::Firmware);
    }
}
