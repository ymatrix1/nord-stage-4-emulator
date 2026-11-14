//! GUI mirror of one layer effects chain and the engine edits it produces.
//!
//! The engine cannot be read from the UI thread, so each of the six FX focus slots keeps the
//! panel's view of its chain here. A unit's ON button does not exist in the engine: `Off` types,
//! disabled flags and neutral values are what the engine gets while the unit is off
//! ([`ChainUi::edits`]). Defaults equal `LayerFxChain::new`, so an untouched panel changes nothing.

use super::tables::{self, MOD1, MOD2, REVERB};
use nord_core::fx::{AmpCabinet, DelayFilterMode, Mod1Type, Mod2Type, ReverbColor, ReverbModel};

/// One effects unit of the Layer Effects bay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Mod1,
    Mod2,
    Amp,
    Comp,
    Delay,
    Reverb,
}

/// Bright / Dark mode of the Reverb (manual p.53); `Normal` is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Bright,
    Dark,
}

/// Panel view of one chain.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainUi {
    /// LAYER EFFECTS ON: gates every unit of this chain at once (manual p.48).
    pub all_on: bool,
    pub mod1_on: bool,
    pub mod1_sel: usize,
    pub mod1_rate: f32,
    pub mod1_amount: f32,
    pub mod1_variation: bool,
    pub mod1_ped: bool,
    pub mod1_ped_pos: f32,
    /// Mod 1 MST CLK step (`0` = free Rate, `n` = `DELAY_SYNC_BEATS[n - 1]`).
    pub mod1_mst: usize,
    pub mod2_on: bool,
    pub mod2_sel: usize,
    pub mod2_rate: f32,
    pub mod2_amount: f32,
    pub mod2_variation: bool,
    pub amp_on: bool,
    /// Selected model / filter / TO ROTARY (`Off` = neutral EQ and drive only).
    pub cabinet: AmpCabinet,
    pub amp_variation: bool,
    pub drive: f32,
    pub bass: f32,
    pub mid: f32,
    pub mid_freq: f32,
    pub treble: f32,
    /// Filter RES `0..=1` (the engine takes `0..=10`).
    pub res: f32,
    pub comp_on: bool,
    pub comp_amount: f32,
    pub comp_fast: bool,
    pub delay_on: bool,
    pub tempo_ms: f32,
    /// MST CLK step (`0` = free time, `n` = `DELAY_SYNC_BEATS[n - 1]`).
    pub mst: usize,
    pub feedback: f32,
    pub delay_mix: f32,
    pub delay_fx: usize,
    pub delay_filter: DelayFilterMode,
    pub ping_pong: bool,
    pub analog: bool,
    pub delay_fx_variation: bool,
    pub reverb_on: bool,
    pub rev_sel: usize,
    pub rev_tone: Tone,
    /// CHORALE (right-hand types) or VARIATION (left-hand types: `reverb_variation` in the engine).
    pub rev_alt: bool,
    pub reverb_mix: f32,
}

impl Default for ChainUi {
    fn default() -> Self {
        Self {
            all_on: true,
            mod1_on: false,
            mod1_sel: 0,
            mod1_rate: 4.0,
            mod1_amount: 0.4,
            mod1_variation: false,
            mod1_ped: false,
            mod1_ped_pos: 0.5,
            mod1_mst: 0,
            mod2_on: false,
            mod2_sel: 0,
            mod2_rate: 0.8,
            mod2_amount: 0.35,
            mod2_variation: false,
            amp_on: false,
            cabinet: AmpCabinet::Off,
            amp_variation: false,
            drive: 0.0,
            bass: 0.0,
            mid: 0.0,
            mid_freq: 1000.0,
            treble: 0.0,
            res: 0.0,
            comp_on: false,
            comp_amount: 0.0,
            comp_fast: false,
            delay_on: false,
            tempo_ms: 375.0,
            mst: 0,
            feedback: 0.35,
            delay_mix: 0.25,
            delay_fx: 0,
            delay_filter: DelayFilterMode::Off,
            ping_pong: false,
            analog: false,
            delay_fx_variation: false,
            reverb_on: true,
            rev_sel: tables::REVERB_DEFAULT,
            rev_tone: Tone::Normal,
            rev_alt: false,
            reverb_mix: 0.15,
        }
    }
}

impl ChainUi {
    /// Unit switch combined with LAYER EFFECTS ON.
    pub(super) fn live(&self, on: bool) -> bool {
        on && self.all_on
    }

    /// Engine Mod 1 type (`Off` while off or while the selected type has no engine model).
    pub fn mod1_engine(&self) -> Mod1Type {
        if self.live(self.mod1_on) {
            MOD1[self.mod1_sel.min(5)].engine.unwrap_or(Mod1Type::Off)
        } else {
            Mod1Type::Off
        }
    }

    /// Engine Mod 2 type.
    pub fn mod2_engine(&self) -> Mod2Type {
        if self.live(self.mod2_on) {
            MOD2[self.mod2_sel.min(5)].engine.unwrap_or(Mod2Type::Off)
        } else {
            Mod2Type::Off
        }
    }

    /// Engine cabinet: TO ROTARY is a routing and stays while the unit is off.
    pub fn cabinet_engine(&self) -> AmpCabinet {
        if self.cabinet == AmpCabinet::ToRotary || self.live(self.amp_on) {
            self.cabinet
        } else {
            AmpCabinet::Off
        }
    }

    /// Engine reverb type (`Off` while the unit is off).
    pub fn reverb_engine(&self) -> ReverbModel {
        if self.live(self.reverb_on) {
            REVERB[self.rev_sel.min(5)]
                .engine
                .unwrap_or(ReverbModel::Off)
        } else {
            ReverbModel::Off
        }
    }

    /// Engine reverb colour: CHORALE on the right-hand types, otherwise Bright / Dark / Normal.
    pub fn color_engine(&self) -> ReverbColor {
        if self.rev_alt && self.rev_sel >= tables::REVERB_CHORALE_FROM {
            return ReverbColor::Chorale;
        }
        match self.rev_tone {
            Tone::Normal => ReverbColor::Normal,
            Tone::Bright => ReverbColor::Bright,
            Tone::Dark => ReverbColor::Dark,
        }
    }

    /// Selected Mod 1 type, whether or not the unit is on.
    pub fn mod1_selected(&self) -> Mod1Type {
        MOD1[self.mod1_sel.min(5)].engine.unwrap_or(Mod1Type::Off)
    }

    /// Whether the Mod 1 shift function means PED (Wah, Pump) rather than VARIATION.
    pub fn mod1_has_ped(&self) -> bool {
        matches!(self.mod1_selected(), Mod1Type::Wah | Mod1Type::Pump)
    }

    /// Whether the selected Mod 1 type can follow the Master Clock (A-PAN, TREM, WAH, PUMP; the
    /// engine's `modulation.rs` has no clock for RM, which is a pitch, nor for A-WAH).
    pub fn mod1_has_clock(&self) -> bool {
        matches!(
            self.mod1_selected(),
            Mod1Type::AutoPan | Mod1Type::Tremolo | Mod1Type::Wah | Mod1Type::Pump
        )
    }

    /// Reverb VARIATION is on (Spring longer, Booth / Room longer pre-delay); the right-hand types
    /// use CHORALE instead.
    pub fn reverb_variation_engine(&self) -> bool {
        self.rev_alt && self.rev_sel < tables::REVERB_CHORALE_FROM
    }

    /// GLOBAL Delay: copy the Delay settings of `src`.
    pub fn copy_delay_from(&mut self, src: &ChainUi) {
        self.delay_on = src.delay_on;
        self.tempo_ms = src.tempo_ms;
        self.mst = src.mst;
        self.feedback = src.feedback;
        self.delay_mix = src.delay_mix;
        self.delay_fx = src.delay_fx;
        self.delay_filter = src.delay_filter;
        self.ping_pong = src.ping_pong;
        self.analog = src.analog;
        self.delay_fx_variation = src.delay_fx_variation;
    }

    /// GLOBAL Compressor: copy the Compressor settings of `src`.
    pub fn copy_comp_from(&mut self, src: &ChainUi) {
        self.comp_on = src.comp_on;
        self.comp_amount = src.comp_amount;
        self.comp_fast = src.comp_fast;
    }

    /// GLOBAL Reverb: copy the Reverb settings of `src`.
    pub fn copy_reverb_from(&mut self, src: &ChainUi) {
        self.reverb_on = src.reverb_on;
        self.rev_sel = src.rev_sel;
        self.rev_tone = src.rev_tone;
        self.rev_alt = src.rev_alt;
        self.reverb_mix = src.reverb_mix;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chain_sends_the_engine_defaults() {
        let c = ChainUi::default();
        assert_eq!(c.mod1_engine(), Mod1Type::Off);
        assert_eq!(c.mod2_engine(), Mod2Type::Off);
        assert_eq!(c.cabinet_engine(), AmpCabinet::Off);
        assert_eq!(c.reverb_engine(), ReverbModel::Stage);
        assert_eq!(c.color_engine(), ReverbColor::Normal);
    }

    #[test]
    fn to_rotary_survives_the_unit_being_off() {
        let c = ChainUi {
            cabinet: AmpCabinet::ToRotary,
            amp_on: false,
            ..ChainUi::default()
        };
        assert_eq!(c.cabinet_engine(), AmpCabinet::ToRotary);
    }

    #[test]
    fn chorale_only_on_the_right_hand_reverb_types() {
        let mut c = ChainUi {
            rev_alt: true,
            rev_sel: 0,
            ..ChainUi::default()
        };
        assert_eq!(c.color_engine(), ReverbColor::Normal);
        assert!(c.reverb_variation_engine(), "left-hand type: VARIATION");
        c.rev_sel = 4;
        assert_eq!(c.color_engine(), ReverbColor::Chorale);
        assert!(!c.reverb_variation_engine(), "right-hand type: CHORALE");
        c.rev_alt = false;
        c.rev_tone = Tone::Dark;
        assert_eq!(c.color_engine(), ReverbColor::Dark);
    }

    #[test]
    fn sharing_copies_move_only_their_unit() {
        let src = ChainUi {
            feedback: 0.7,
            comp_amount: 0.9,
            reverb_mix: 0.6,
            drive: 0.5,
            ..ChainUi::default()
        };
        let mut d = ChainUi::default();
        d.copy_delay_from(&src);
        assert_eq!(d.feedback, 0.7);
        assert_eq!(d.comp_amount, 0.0);
        d.copy_comp_from(&src);
        d.copy_reverb_from(&src);
        assert_eq!((d.comp_amount, d.reverb_mix, d.drive), (0.9, 0.6, 0.0));
    }

    #[test]
    fn default_engine_values_match_a_new_chain() {
        let reference = nord_core::fx::LayerFxChain::new(48_000.0);
        let c = ChainUi::default();
        assert_eq!(c.mod1_rate, reference.mod1_rate_hz);
        assert_eq!(c.mod2_amount, reference.mod2_amount);
        assert_eq!(c.tempo_ms, reference.delay_time_ms);
        assert_eq!(c.feedback, reference.delay_feedback);
        assert_eq!(c.reverb_mix, reference.reverb_mix);
        assert_eq!(c.mid_freq, reference.eq_mid_freq_hz);
        assert_eq!(c.mod1_ped_pos, reference.mod1_ped_pos);
    }

    #[test]
    fn ped_and_clock_follow_the_selected_mod1_type() {
        let mut c = ChainUi::default();
        for (sel, ped, clock) in [
            (0, false, true),  // A-PAN
            (1, false, true),  // TREM
            (2, false, false), // RM
            (3, false, false), // A-WAH
            (4, true, true),   // WAH
            (5, true, true),   // PUMP
        ] {
            c.mod1_sel = sel;
            assert_eq!(c.mod1_has_ped(), ped, "ped {sel}");
            assert_eq!(c.mod1_has_clock(), clock, "clock {sel}");
        }
    }
}
