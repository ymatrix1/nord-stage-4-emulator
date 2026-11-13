//! The opt-in switch that routes the engine's FX parameters through the firmware closed forms.

/// Which firmware-derived behaviours a [`super::LayerFxChain`] uses. `Default` is all off: the
/// chain then runs exactly as before (default `render_hash` unchanged).
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
/// * `compressor`: the layer compressor is the firmware gain computer ([`super::FwCompressor`])
///   with the `kCmpAmt` words. Algorithm VERIFIED, `kCmpAmt` words VERIFIED, slope and log gain
///   ASSUMED. **Latency 4 frames (83 us at 48 kHz)** on that layer; the stage runs even when
///   `comp_enabled` is false (amount 0, unity) so every layer of the mix keeps the same latency.
///
/// Not switchable yet, because the audio path needs words or tables that are not decoded: the Amp
/// drive stage (`FwAmpDrive`, tables are firmware data), the tone stack (`FwToneStack2`, base words
/// of the Amp type switch), the all-pass (`FwAllpass`, Fx1 type to engine `Mod1Type` map) and the
/// delay line itself (`FwDelay` / the ring variant `0x120AB5`: ring length and routing words).
/// Their parameter fills exist as `fw_params::set_*`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareFx {
    /// Delay mix law of `kDlyMix`.
    pub delay_laws: bool,
    /// Firmware compressor with the `kCmpAmt` words.
    pub compressor: bool,
    /// Firmware delay time law of `kDlyRateClkDiv` (free-running delay only).
    pub delay_time: bool,
}

impl FirmwareFx {
    /// Every switch off (the default).
    pub const OFF: Self = Self {
        delay_laws: false,
        compressor: false,
        delay_time: false,
    };

    /// Every switch on.
    pub const ALL: Self = Self {
        delay_laws: true,
        compressor: true,
        delay_time: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_all_turns_every_switch_on() {
        assert_eq!(FirmwareFx::default(), FirmwareFx::OFF);
        let all = std::hint::black_box(FirmwareFx::ALL);
        assert!(all.delay_laws && all.compressor && all.delay_time);
        assert_ne!(all, FirmwareFx::OFF);
    }
}
