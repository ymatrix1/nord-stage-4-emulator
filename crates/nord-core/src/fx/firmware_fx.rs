//! The opt-in switch that routes the engine's FX parameters through the firmware closed forms.

/// Which firmware-derived behaviours a [`super::LayerFxChain`] uses. `Default` is [`Self::DEFAULT`]:
/// only `amp` is on (the one switch verified end to end, D22); [`Self::OFF`] restores the engine
/// paths everywhere.
///
/// Status of each switch (`docs/specs/effects-and-routing.md` s4.0c):
///
/// * `delay_laws`: the delay's send and dry gains follow `kDlyMix` (`fw_map::delay_send`,
///   `delay_dry`; VERIFIED words, EMPIRICAL roles) instead of the engine's `dry + wet * mix`.
///   The feedback law (`v / 127`) equals the engine scale, so only the `0.95` stability clamp
///   remains.
/// * `delay_time`: the free-running delay time follows the firmware law of `kDlyRateClkDiv`
///   (`fw_delay_time`: 14 bit value to `c[0x15]` samples, first echo `c[0x15] + 2` samples
///   later). Both halves VERIFIED (ARM harness, `sharc-fx` oracle). It needs
///   `LayerFxChain::delay_rate_raw` (the preset's 14 bit value, set by `apply_fx_chain`); without
///   it, or in tempo sync, the engine time law is used. The firmware cross-fades two taps for
///   45 ms when the value changes; the engine keeps its own slew.
/// * `reverb_mix`: the reverb send and dry gains follow `kRvbAmt` (`fw_map::reverb_send` / `reverb_dry`,
///   equal power; words VERIFIED on all 128 values, role EMPIRICAL). Only the mix: the engine's
///   reverb network keeps its own taps, decay and damping (the multi-tap routines are not ported).
/// * `reverb`: the Spring reverbs (`kRvbType` 0 and 1, `ReverbModel::Spring` and its variation)
///   run the firmware routine `0x12417E` with the ported core `0x123F61` ([`super::FwReverb`],
///   bit exact against the `sharc-fx` oracle, `tests_fw_rvb_golden.rs`) instead of the engine FDN.
///   The routine does the dry/wet mix itself (`kRvbAmt` words), so `reverb_mix` is not applied on
///   top. **Latency 4 frames** while it runs; the other reverb types keep the engine FDN (their
///   routines are not ported). Default off: see `docs/research/fx-reverb-leslie.md` s10 for the
///   comparison with the video.
/// * `compressor`: the layer compressor is the firmware gain computer ([`super::FwCompressor`])
///   with the `kCmpAmt` words. Algorithm VERIFIED, `kCmpAmt` words VERIFIED, slope and log gain
///   ASSUMED. **Latency 4 frames (83 us at 48 kHz)** on that layer; the stage runs even when
///   `comp_enabled` is false (amount 0, unity) so every layer of the mix keeps the same latency.
///
/// * `amp`: the Twin, Twin II, JC, JC II, Small and Small II amps run the firmware drive stage
///   ([`super::FwAmpDrive`] with the real tables and the words of `kAmpTypeNew`, `kAmpDrive` =
///   `127 * drive / 10`) instead of the engine's `tanh` drive and RBJ cabinet voicing. Words,
///   tables and algorithm VERIFIED against the oracle (`tests_amp_golden.rs`); **latency 4 frames**
///   on that layer. The firmware's cabinet FIR and the organ unit's tone routines are not
///   modelled, so the engine's 3-band EQ still follows; To Rotary, LP24 and HP24 keep the engine
///   path (their type switch sends no drive stage words).
///
/// * `mod1`: the Mod 1 types A-PAN (`kFx1Type` 0, 1) and TREM (2, 3) run the firmware routines
///   `0x122D93` ([`super::FwAutoPan`]) and `0x122C39` ([`super::FwAllpass`] at mix 0) with the
///   instance words of the FX image and the `kFx1Amt` / `kFx1RateClkDiv` words
///   (`fw_mod1`; `docs/research/fx-mod-types.md`). Routines VERIFIED against the oracle, words
///   VERIFIED, the TREM role EMPIRICAL. The LFO runs at the firmware frame rate (Hz equal), the
///   rate scale is 0.03 .. 5.3 Hz. **Latency 4 frames** on that layer. Free running only
///   (Master Clock sync keeps the engine path); RM, A-WAH, WAH and PUMP keep the engine models.
///
/// Not switchable yet, because the audio path needs words or tables that are not decoded: the
/// tone stack (`FwToneStack2`, base words of the Amp type switch), the all-pass (`FwAllpass`, Fx1 type to engine `Mod1Type` map) and the
/// delay line itself (`FwDelay` / the ring variant `0x120AB5`: ring length and routing words).
/// Their parameter fills exist as `fw_params::set_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareFx {
    /// Delay mix law of `kDlyMix`.
    pub delay_laws: bool,
    /// Firmware compressor with the `kCmpAmt` words.
    pub compressor: bool,
    /// Firmware delay time law of `kDlyRateClkDiv` (free-running delay only).
    pub delay_time: bool,
    /// Reverb mix law of `kRvbAmt`.
    pub reverb_mix: bool,
    /// Firmware reverb routine for the Spring types (`kRvbType` 0, 1); other types use the FDN.
    pub reverb: bool,
    /// Firmware Amp drive stage with the `kAmpTypeNew` words (Twin, JC, Small families).
    pub amp: bool,
    /// Firmware A-PAN (Fx1 types 0, 1) and TREM (types 2, 3) with the `kFx1Amt` /
    /// `kFx1RateClkDiv` words.
    pub mod1: bool,
}

impl Default for FirmwareFx {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FirmwareFx {
    /// The engine default: only the verified firmware amp (`amp`, see its doc) is on.
    pub const DEFAULT: Self = Self {
        amp: true,
        ..Self::OFF
    };

    /// Every switch off.
    pub const OFF: Self = Self {
        delay_laws: false,
        compressor: false,
        delay_time: false,
        reverb_mix: false,
        reverb: false,
        amp: false,
        mod1: false,
    };

    /// Every switch on.
    pub const ALL: Self = Self {
        delay_laws: true,
        compressor: true,
        delay_time: true,
        reverb_mix: true,
        reverb: true,
        amp: true,
        mod1: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_all_turns_every_switch_on() {
        assert_eq!(FirmwareFx::default(), FirmwareFx::DEFAULT);
        let d = std::hint::black_box(FirmwareFx::DEFAULT);
        assert!(d.amp && !d.delay_laws && !d.compressor);
        let all = std::hint::black_box(FirmwareFx::ALL);
        assert!(
            all.delay_laws
                && all.compressor
                && all.delay_time
                && all.reverb_mix
                && all.reverb
                && all.amp
                && all.mod1
        );
        assert!(!d.reverb, "the firmware reverb is opt-in");
        assert!(!d.mod1 && all.mod1, "the firmware Mod 1 is opt-in");
        assert_ne!(all, FirmwareFx::OFF);
    }
}
