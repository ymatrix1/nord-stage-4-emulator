//! Engine edits a [`ChainUi`] produces for one unit (or all of them).

use super::chain::{ChainUi, Unit};
use super::tables;
use crate::audio::{FxEdit, FxFlag, FxFloat};
use nord_core::fx::DelayFeedbackFx;

fn float(slot: u8, param: FxFloat, value: f32) -> FxEdit {
    FxEdit::Float { slot, param, value }
}

fn flag(slot: u8, param: FxFlag, on: bool) -> FxEdit {
    FxEdit::Flag { slot, param, on }
}

impl ChainUi {
    /// The engine edits that bring the chain of `slot` in line with this view for `unit`.
    pub fn edits(&self, unit: Unit, slot: u8) -> Vec<FxEdit> {
        let amp_live = self.live(self.amp_on);
        let g = |v: f32| if amp_live { v } else { 0.0 };
        match unit {
            Unit::Mod1 => vec![
                FxEdit::Mod1Type {
                    slot,
                    kind: self.mod1_engine(),
                },
                float(slot, FxFloat::Mod1Rate, self.mod1_rate),
                float(slot, FxFloat::Mod1Amount, self.mod1_amount),
                float(slot, FxFloat::Mod1PedPos, self.mod1_ped_pos),
                flag(slot, FxFlag::Mod1Variation, self.mod1_variation),
                flag(slot, FxFlag::Mod1Ped, self.mod1_ped && self.mod1_has_ped()),
                FxEdit::Mod1Sync {
                    slot,
                    beats: tables::mst_beats(self.mod1_mst).filter(|_| self.mod1_has_clock()),
                },
            ],
            Unit::Mod2 => vec![
                FxEdit::Mod2Type {
                    slot,
                    kind: self.mod2_engine(),
                },
                float(slot, FxFloat::Mod2Rate, self.mod2_rate),
                float(slot, FxFloat::Mod2Amount, self.mod2_amount),
                flag(slot, FxFlag::Mod2Variation, self.mod2_variation),
            ],
            Unit::Amp => vec![
                FxEdit::Cabinet {
                    slot,
                    cabinet: self.cabinet_engine(),
                },
                flag(slot, FxFlag::AmpVariation, self.amp_variation),
                float(slot, FxFloat::Drive, g(self.drive)),
                float(slot, FxFloat::EqBass, g(self.bass)),
                float(slot, FxFloat::EqMid, g(self.mid)),
                float(slot, FxFloat::EqMidFreq, self.mid_freq),
                float(slot, FxFloat::EqTreble, g(self.treble)),
                float(slot, FxFloat::FilterRes, self.res * 10.0),
            ],
            Unit::Comp => vec![
                flag(slot, FxFlag::CompEnabled, self.live(self.comp_on)),
                float(slot, FxFloat::CompAmount, self.comp_amount),
                flag(slot, FxFlag::CompFast, self.comp_fast),
            ],
            Unit::Delay => vec![
                flag(slot, FxFlag::DelayEnabled, self.live(self.delay_on)),
                float(slot, FxFloat::DelayTime, self.tempo_ms),
                FxEdit::DelaySync {
                    slot,
                    beats: tables::mst_beats(self.mst),
                },
                float(slot, FxFloat::DelayFeedback, self.feedback),
                float(slot, FxFloat::DelayMix, self.delay_mix),
                FxEdit::DelayFx {
                    slot,
                    fx: tables::DELAY_FX[self.delay_fx.min(5)]
                        .engine
                        .unwrap_or(DelayFeedbackFx::Off),
                },
                FxEdit::DelayFilter {
                    slot,
                    mode: self.delay_filter,
                },
                flag(slot, FxFlag::DelayPingPong, self.ping_pong),
                flag(slot, FxFlag::DelayAnalog, self.analog),
                flag(slot, FxFlag::DelayFxVariation, self.delay_fx_variation),
            ],
            Unit::Reverb => vec![
                FxEdit::ReverbModel {
                    slot,
                    model: self.reverb_engine(),
                },
                FxEdit::ReverbColor {
                    slot,
                    color: self.color_engine(),
                },
                flag(
                    slot,
                    FxFlag::ReverbVariation,
                    self.reverb_variation_engine(),
                ),
                float(slot, FxFloat::ReverbMix, self.reverb_mix),
            ],
        }
    }

    /// Every edit of every unit (LAYER EFFECTS ON, program load).
    pub fn all_edits(&self, slot: u8) -> Vec<FxEdit> {
        const UNITS: [Unit; 6] = [
            Unit::Mod1,
            Unit::Mod2,
            Unit::Amp,
            Unit::Comp,
            Unit::Delay,
            Unit::Reverb,
        ];
        UNITS.iter().flat_map(|&u| self.edits(u, slot)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::fx::{AmpCabinet, Mod1Type, Mod2Type, ReverbModel};

    #[test]
    fn unit_off_sends_off_types_and_neutral_values() {
        let mut c = ChainUi {
            mod1_sel: 1,
            drive: 0.8,
            bass: 5.0,
            cabinet: AmpCabinet::Twin,
            ..ChainUi::default()
        };
        c.mod1_on = true;
        assert_eq!(c.mod1_engine(), Mod1Type::Tremolo);
        c.mod1_on = false;
        assert_eq!(c.mod1_engine(), Mod1Type::Off);
        let e = c.edits(Unit::Amp, 1);
        assert!(e.contains(&float(1, FxFloat::Drive, 0.0)));
        assert!(e.contains(&float(1, FxFloat::EqBass, 0.0)));
        assert!(e.contains(&FxEdit::Cabinet {
            slot: 1,
            cabinet: AmpCabinet::Off
        }));
        c.amp_on = true;
        let e = c.edits(Unit::Amp, 1);
        assert!(e.contains(&float(1, FxFloat::Drive, 0.8)));
        assert!(e.contains(&float(1, FxFloat::EqBass, 5.0)));
    }

    #[test]
    fn layer_effects_off_gates_every_unit() {
        let mut c = ChainUi {
            mod1_on: true,
            mod2_on: true,
            comp_on: true,
            delay_on: true,
            ..ChainUi::default()
        };
        c.all_on = false;
        assert_eq!(c.mod1_engine(), Mod1Type::Off);
        assert_eq!(c.reverb_engine(), ReverbModel::Off);
        let e = c.all_edits(0);
        assert!(e.contains(&flag(0, FxFlag::CompEnabled, false)));
        assert!(e.contains(&flag(0, FxFlag::DelayEnabled, false)));
    }

    #[test]
    fn a_wah_and_spin_selection_reach_the_engine_while_the_unit_is_on() {
        let c = ChainUi {
            mod1_on: true,
            mod1_sel: 3, // A-WAH
            mod2_on: true,
            mod2_sel: 5, // SPIN
            ..ChainUi::default()
        };
        assert_eq!(c.mod1_engine(), Mod1Type::AutoWah);
        assert_eq!(c.mod2_engine(), Mod2Type::Spin);
        let off = ChainUi {
            mod1_sel: 3,
            mod2_sel: 5,
            ..ChainUi::default()
        };
        assert_eq!(off.mod1_engine(), Mod1Type::Off, "unit off");
        assert_eq!(off.mod2_engine(), Mod2Type::Off);
    }

    #[test]
    fn ped_applies_to_wah_and_pump_only() {
        let mut c = ChainUi {
            mod1_ped: true,
            mod1_sel: 1,
            ..ChainUi::default()
        };
        assert!(c
            .edits(Unit::Mod1, 0)
            .contains(&flag(0, FxFlag::Mod1Ped, false)));
        c.mod1_sel = 4;
        assert!(c.mod1_has_ped());
        assert!(c
            .edits(Unit::Mod1, 0)
            .contains(&flag(0, FxFlag::Mod1Ped, true)));
        c.mod1_sel = 3; // A-WAH has VARIATION, not PED
        assert!(c
            .edits(Unit::Mod1, 0)
            .contains(&flag(0, FxFlag::Mod1Ped, false)));
    }

    #[test]
    fn filter_res_is_scaled_to_the_engine_range() {
        let c = ChainUi {
            res: 0.5,
            ..ChainUi::default()
        };
        assert!(c
            .edits(Unit::Amp, 2)
            .contains(&float(2, FxFloat::FilterRes, 5.0)));
    }

    #[test]
    fn delay_edits_carry_the_master_clock_division() {
        let c = ChainUi {
            mst: 6,
            ..ChainUi::default()
        };
        assert!(c.edits(Unit::Delay, 3).contains(&FxEdit::DelaySync {
            slot: 3,
            beats: Some(0.75)
        }));
    }

    #[test]
    fn new_types_reach_the_engine_and_pump_ped_and_clock_are_sent() {
        let mut c = ChainUi {
            mod1_on: true,
            mod1_sel: 5, // PUMP
            mod1_ped: true,
            mod1_mst: 5, // 1/4
            mod2_on: true,
            mod2_sel: 5, // SPIN
            ..ChainUi::default()
        };
        let e = c.edits(Unit::Mod1, 2);
        assert!(e.contains(&FxEdit::Mod1Type {
            slot: 2,
            kind: Mod1Type::Pump
        }));
        assert!(e.contains(&flag(2, FxFlag::Mod1Ped, true)));
        assert!(e.contains(&FxEdit::Mod1Sync {
            slot: 2,
            beats: Some(1.0)
        }));
        assert!(c.edits(Unit::Mod2, 2).contains(&FxEdit::Mod2Type {
            slot: 2,
            kind: Mod2Type::Spin
        }));
        c.mod1_sel = 2; // RM has no Master Clock and no PED
        let e = c.edits(Unit::Mod1, 2);
        assert!(e.contains(&FxEdit::Mod1Sync {
            slot: 2,
            beats: None
        }));
        assert!(e.contains(&flag(2, FxFlag::Mod1Ped, false)));
    }

    #[test]
    fn reverb_variation_is_sent_for_the_left_hand_types_only() {
        let mut c = ChainUi {
            rev_sel: 0, // SPRING
            rev_alt: true,
            ..ChainUi::default()
        };
        assert!(c
            .edits(Unit::Reverb, 0)
            .contains(&flag(0, FxFlag::ReverbVariation, true)));
        c.rev_sel = 4; // HALL: CHORALE instead
        assert!(c
            .edits(Unit::Reverb, 0)
            .contains(&flag(0, FxFlag::ReverbVariation, false)));
    }
}
