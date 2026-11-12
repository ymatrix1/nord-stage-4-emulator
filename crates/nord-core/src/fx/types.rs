//! FX enums, `MasterClock` and the per-chain morph set.

use super::*;

/// Modulation Slot 1 modes (`Mod 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mod1Type {
    Off,
    Tremolo,
    AutoPan,
    RingMod,
    Wah,
}

/// Modulation Slot 2 modes (`Mod 2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mod2Type {
    Off,
    Chorus,
    Flanger,
    Phaser,
    Vibe,
}

/// Amp simulation modes (`Amp Sim / EQ`).
///
/// VERIFIED (names/order): UI strings at `0x2cc6cc` in OS v1.66 `bank0_CODE.bin` read
/// `Twin Type I/II`, `JC Type I/II`, `Small Speaker`, `Very Small Speaker`, `To Rotary`, `LP24`, `HP24`.
/// EMPIRICAL (numeric index 0..9 = off, Twin, Twin II, JC, JC II, Small, Small II, To Rotary, LP, HP):
/// third-party `ns4decode` (MIT, Randy 2024) `ns4names.AmpEQmode`, cross-checked on factory programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmpCabinet {
    #[default]
    Off,
    Small,
    Jc,
    Twin,
    ToRotary,
    LpFilter,
    HpFilter,
}

/// Feedback loop filter voicings for `Delay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelayFilterMode {
    #[default]
    Off,
    Hp,
    Lp,
    Bp,
}

/// Delay feedback effect (`Delay Feedback Effect`): `Off` = Bypass.
///
/// VERIFIED (names, order): UI strings `Ensemble Deep`, `Vibe`, `Flam 20 ms`, `Flam 40 ms`,
/// `Space 50 Wet`, `Space 100 Wet` at `0x2cc790..0x2cc7e0` of OS v1.66 `bank0_CODE.bin`.
/// VERIFIED (behaviour, vendor manual v1.6X pp. 50-51): ENS/VIBE/CHOR are the Mod 2 counterparts;
/// FLAM = ping-pong with offset L/R channels (variation: 40 ms instead of 20 ms); SPACE = reverb-like
/// diffusion, 50 % mixed (variation: 100 % wet). The DSP behind them is not in the firmware image, so all
/// numeric parameters below are ASSUMED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelayFeedbackFx {
    #[default]
    Off,
    Ensemble,
    Vibe,
    Chorus,
    Flam,
    Space,
}

/// Length of the short modulation / offset buffers of the delay feedback effect (samples).
pub(super) const DFX_BUF_LEN: usize = 4096;
/// Schroeder all-pass lengths for `Space` (`[L0, L1, R0, R1]`, primes).
pub(super) const DFX_AP_LENGTHS: [usize; 4] = [151, 233, 173, 257];

/// Tempo-synced rhythmic subdivisions for `Delay` when `delay_clock_sync` is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelaySubdivision {
    Quarter,
    #[default]
    DottedEighth,
    Eighth,
    Sixteenth,
}

impl DelaySubdivision {
    /// Returns beat multiplier relative to a quarter note (`1.0`).
    #[inline]
    pub fn beats(self) -> f32 {
        match self {
            Self::Quarter => 1.0,
            Self::DottedEighth => 0.75,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
        }
    }
}

/// Reverb room size models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverbModel {
    Off,
    Booth,
    Room,
    Stage,
    Spring,
    Hall,
    Cathedral,
}

/// Reverb tonal voicing & Chorale modulation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReverbColor {
    #[default]
    Normal,
    Bright,
    Dark,
    Chorale,
}

/// Global Master Clock (`BPM` and normalized beat phase).
#[derive(Debug, Clone, Copy)]
pub struct MasterClock {
    pub bpm: f32,
    pub beat_phase: f32,
}

impl Default for MasterClock {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beat_phase: 0.0,
        }
    }
}

impl MasterClock {
    pub fn advance(&mut self, sample_rate: f32) {
        let beats_per_sec = self.bpm.clamp(30.0, 300.0) / 60.0;
        self.beat_phase = (self.beat_phase + beats_per_sec / sample_rate) % 1.0;
    }
}

/// Per-parameter Morph (`Wheel` / `A.T.` / `CtrlPed`) bases and deltas for a layer FX chain.
///
/// A parameter is driven only when at least one delta is non-zero; otherwise the plain field is left alone.
#[derive(Debug, Clone, Copy)]
pub struct FxMorphSet {
    pub mod1_rate: MorphParameter,
    pub mod1_amount: MorphParameter,
    pub mod2_rate: MorphParameter,
    pub mod2_amount: MorphParameter,
    pub eq_mid_freq: MorphParameter,
    pub amp_drive: MorphParameter,
    pub delay_mix: MorphParameter,
    pub delay_feedback: MorphParameter,
    pub reverb_amount: MorphParameter,
}

impl Default for FxMorphSet {
    fn default() -> Self {
        let p = MorphParameter::new(0.0);
        Self {
            mod1_rate: p,
            mod1_amount: p,
            mod2_rate: p,
            mod2_amount: p,
            eq_mid_freq: p,
            amp_drive: p,
            delay_mix: p,
            delay_feedback: p,
            reverb_amount: p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_clock_advances_beat_phase_and_wraps() {
        let mut c = MasterClock::default();
        for _ in 0..200_000 {
            c.advance(48_000.0);
            assert!((0.0..1.0).contains(&c.beat_phase));
        }
    }
}
