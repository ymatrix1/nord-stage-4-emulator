//! The opt-in switch that routes the engine's FX parameters through the firmware closed forms.

/// Which firmware-derived behaviours a [`super::LayerFxChain`] uses. `Default` is all off: the
/// chain then runs exactly as before (default `render_hash` unchanged).
///
/// Status of each switch (`docs/specs/effects-and-routing.md` s4.0c):
///
/// * `delay_laws`: the delay's send and dry gains follow `kDlyMix` (`fw_map::delay_send`,
///   `delay_dry`; VERIFIED words, EMPIRICAL roles) instead of the engine's `dry + wet * mix`.
///   The delay **time** keeps the engine law: the firmware words `c[0x15]`, `c[0x16]`, `c[0x19]`
///   are not decoded far enough to drive the line (`fx-contexts.md` s5.1). The feedback law
///   (`v / 127`) equals the engine scale, so only the `0.95` stability clamp remains.
/// * `compressor`: the layer compressor is the firmware gain computer ([`super::FwCompressor`])
///   with the `kCmpAmt` words. Algorithm VERIFIED, `kCmpAmt` words VERIFIED, slope and log gain
///   ASSUMED. **Latency 4 frames (83 us at 48 kHz)** on that layer; the stage runs even when
///   `comp_enabled` is false (amount 0, unity) so every layer of the mix keeps the same latency.
///
/// Not switchable yet, because the audio path needs words or tables that are not decoded: the Amp
/// drive stage (`FwAmpDrive`, tables are firmware data), the tone stack (`FwToneStack2`, base words
/// of the Amp type switch), the all-pass (`FwAllpass`, Fx1 type to engine `Mod1Type` map) and the
/// delay line (`FwDelay`, time words). Their parameter fills exist as `fw_params::set_*`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareFx {
    /// Delay mix law of `kDlyMix`.
    pub delay_laws: bool,
    /// Firmware compressor with the `kCmpAmt` words.
    pub compressor: bool,
}

impl FirmwareFx {
    /// Every switch off (the default).
    pub const OFF: Self = Self {
        delay_laws: false,
        compressor: false,
    };

    /// Every switch on.
    pub const ALL: Self = Self {
        delay_laws: true,
        compressor: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_all_turns_every_switch_on() {
        assert_eq!(FirmwareFx::default(), FirmwareFx::OFF);
        let all = std::hint::black_box(FirmwareFx::ALL);
        assert!(all.delay_laws && all.compressor);
        assert_ne!(all, FirmwareFx::OFF);
    }
}
