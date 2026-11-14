//! Write-through view of the focused chain in `Stage4PanelState`'s legacy Fx fields.
//!
//! `Stage4PanelState::apply_program` writes `mod1_type`, `delay_on`, `reverb_model` and (with
//! knob sync) the knob values straight into those fields. The bay mirrors its focused chain into
//! them every frame ([`StateEcho::write_to`]) and remembers what it wrote; a difference on the
//! next frame was written by someone else and is adopted into the chain ([`adopt`]).

use super::chain::ChainUi;
use super::tables::{MOD1, MOD2, REVERB};
use crate::panel::Stage4PanelState;
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};

/// The legacy Fx fields of the panel state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateEcho {
    pub mod1_type: Mod1Type,
    pub mod1_rate: f32,
    pub mod1_amount: f32,
    pub mod2_type: Mod2Type,
    pub mod2_rate: f32,
    pub mod2_amount: f32,
    pub eq_drive: f32,
    pub eq_bass: f32,
    pub eq_mid: f32,
    pub eq_mid_freq: f32,
    pub eq_treble: f32,
    pub delay_on: bool,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub reverb_model: ReverbModel,
    pub reverb_mix: f32,
}

impl StateEcho {
    /// Read the legacy fields of `s`.
    pub fn read(s: &Stage4PanelState) -> Self {
        Self {
            mod1_type: s.mod1_type,
            mod1_rate: s.mod1_rate,
            mod1_amount: s.mod1_amount,
            mod2_type: s.mod2_type,
            mod2_rate: s.mod2_rate,
            mod2_amount: s.mod2_amount,
            eq_drive: s.eq_drive,
            eq_bass: s.eq_bass,
            eq_mid: s.eq_mid,
            eq_mid_freq: s.eq_mid_freq,
            eq_treble: s.eq_treble,
            delay_on: s.delay_on,
            delay_feedback: s.delay_feedback,
            delay_mix: s.delay_mix,
            reverb_model: s.reverb_model,
            reverb_mix: s.reverb_mix,
        }
    }

    /// What `chain` looks like through the legacy fields.
    pub fn of(c: &ChainUi) -> Self {
        Self {
            mod1_type: c.mod1_engine(),
            mod1_rate: c.mod1_rate,
            mod1_amount: c.mod1_amount,
            mod2_type: c.mod2_engine(),
            mod2_rate: c.mod2_rate,
            mod2_amount: c.mod2_amount,
            eq_drive: c.drive,
            eq_bass: c.bass,
            eq_mid: c.mid,
            eq_mid_freq: c.mid_freq,
            eq_treble: c.treble,
            delay_on: c.delay_on,
            delay_feedback: c.feedback,
            delay_mix: c.delay_mix,
            reverb_model: c.reverb_engine(),
            reverb_mix: c.reverb_mix,
        }
    }

    /// Write the legacy fields of `s`.
    pub fn write_to(&self, s: &mut Stage4PanelState) {
        s.mod1_type = self.mod1_type;
        s.mod1_rate = self.mod1_rate;
        s.mod1_amount = self.mod1_amount;
        s.mod2_type = self.mod2_type;
        s.mod2_rate = self.mod2_rate;
        s.mod2_amount = self.mod2_amount;
        s.eq_drive = self.eq_drive;
        s.eq_bass = self.eq_bass;
        s.eq_mid = self.eq_mid;
        s.eq_mid_freq = self.eq_mid_freq;
        s.eq_treble = self.eq_treble;
        s.delay_on = self.delay_on;
        s.delay_feedback = self.delay_feedback;
        s.delay_mix = self.delay_mix;
        s.reverb_model = self.reverb_model;
        s.reverb_mix = self.reverb_mix;
    }
}

/// Adopt the fields of `now` that differ from `prev` into `c`.
pub fn adopt(c: &mut ChainUi, prev: &StateEcho, now: &StateEcho) {
    if now.mod1_type != prev.mod1_type {
        c.mod1_on = now.mod1_type != Mod1Type::Off;
        if let Some(i) = MOD1.iter().position(|x| x.engine == Some(now.mod1_type)) {
            c.mod1_sel = i;
        }
    }
    if now.mod2_type != prev.mod2_type {
        c.mod2_on = now.mod2_type != Mod2Type::Off;
        if let Some(i) = MOD2.iter().position(|x| x.engine == Some(now.mod2_type)) {
            c.mod2_sel = i;
        }
    }
    if now.reverb_model != prev.reverb_model {
        c.reverb_on = now.reverb_model != ReverbModel::Off;
        if let Some(i) = REVERB
            .iter()
            .position(|x| x.engine == Some(now.reverb_model))
        {
            c.rev_sel = i;
        }
    }
    if now.delay_on != prev.delay_on {
        c.delay_on = now.delay_on;
    }
    let pairs = [
        (now.mod1_rate, prev.mod1_rate, &mut c.mod1_rate),
        (now.mod1_amount, prev.mod1_amount, &mut c.mod1_amount),
        (now.mod2_rate, prev.mod2_rate, &mut c.mod2_rate),
        (now.mod2_amount, prev.mod2_amount, &mut c.mod2_amount),
        (now.eq_drive, prev.eq_drive, &mut c.drive),
        (now.eq_bass, prev.eq_bass, &mut c.bass),
        (now.eq_mid, prev.eq_mid, &mut c.mid),
        (now.eq_mid_freq, prev.eq_mid_freq, &mut c.mid_freq),
        (now.eq_treble, prev.eq_treble, &mut c.treble),
        (now.delay_feedback, prev.delay_feedback, &mut c.feedback),
        (now.delay_mix, prev.delay_mix, &mut c.delay_mix),
        (now.reverb_mix, prev.reverb_mix, &mut c.reverb_mix),
    ];
    for (n, p, dst) in pairs {
        if n != p {
            *dst = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_round_trips_through_the_state() {
        let mut s = Stage4PanelState::default();
        let c = ChainUi {
            mod1_on: true,
            mod1_sel: 1,
            drive: 0.4,
            ..ChainUi::default()
        };
        let e = StateEcho::of(&c);
        e.write_to(&mut s);
        assert_eq!(StateEcho::read(&s), e);
        assert_eq!(s.mod1_type, Mod1Type::Tremolo);
        assert_eq!(s.eq_drive, 0.4);
    }

    #[test]
    fn only_changed_fields_are_adopted() {
        let mut c = ChainUi {
            drive: 0.4,
            ..ChainUi::default()
        };
        let prev = StateEcho::of(&c);
        let mut now = prev;
        now.mod2_type = Mod2Type::Vibe;
        now.delay_on = true;
        now.reverb_mix = 0.9;
        adopt(&mut c, &prev, &now);
        assert!(c.mod2_on);
        assert_eq!(MOD2[c.mod2_sel].engine, Some(Mod2Type::Vibe));
        assert!(c.delay_on);
        assert_eq!(c.reverb_mix, 0.9);
        assert_eq!(c.drive, 0.4, "untouched field kept");
    }

    #[test]
    fn program_turning_a_unit_off_clears_its_switch() {
        let mut c = ChainUi {
            mod1_on: true,
            reverb_on: true,
            ..ChainUi::default()
        };
        let prev = StateEcho::of(&c);
        let mut now = prev;
        now.mod1_type = Mod1Type::Off;
        now.reverb_model = ReverbModel::Off;
        adopt(&mut c, &prev, &now);
        assert!(!c.mod1_on && !c.reverb_on);
    }
}
