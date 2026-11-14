//! UI-only state of the Fx area (widgets that have no engine field of their own).
//!
//! `FxUi` holds a [`ChainUi`] mirror for each of the six FX focus slots, the GROUP / GLOBAL
//! sharing sources, button gesture trackers and the delay tap tempo. The legacy fields of
//! `Stage4PanelState` (`mod1_type`, `reverb_mix`, ...) are kept as a write-through view of the
//! focused chain so `apply_program` keeps working: [`FxUi::pull_external`] adopts what a program
//! load changed there (`StateEcho`).

mod chain;
mod echo;
mod section;
mod tables;
mod tap;
mod unit_edits;

pub use chain::{ChainUi, Tone, Unit};
pub use echo::StateEcho;
pub use tables::{
    mst_beats, next_supported, Choice, DELAY_FX, MOD1, MOD2, MST_LABELS, REVERB,
    REVERB_CHORALE_FROM, REVERB_DEFAULT,
};
pub use tap::TapTempo;

use crate::audio::{FxEdit, FX_SLOTS};
use nord_core::engine::FxSharing;
use nord_core::fx::DelayFeedbackFx;
use nord_panel::behaviour::ButtonTracker;
use std::collections::HashMap;

/// Panel sections an FX focus button selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSection {
    Organ,
    Piano,
    Synth,
}

/// Focus slots of a section (`0` Organ, `1..=2` Piano A/B, `3..=5` Synth A/B/C).
pub fn section_slots(s: FocusSection) -> std::ops::Range<usize> {
    match s {
        FocusSection::Organ => 0..1,
        FocusSection::Piano => 1..3,
        FocusSection::Synth => 3..6,
    }
}

/// Section a slot belongs to.
pub fn section_of_slot(slot: usize) -> FocusSection {
    match slot {
        0 => FocusSection::Organ,
        1 | 2 => FocusSection::Piano,
        _ => FocusSection::Synth,
    }
}

/// Focus slot for the legacy `fx_focus` index (`0` Organ, `1` Piano, `2..=4` Synth A/B/C).
pub fn slot_for(fx_focus: usize, piano_b: bool) -> usize {
    match fx_focus {
        0 => 0,
        1 => 1 + usize::from(piano_b),
        n => (3 + (n - 2)).min(FX_SLOTS - 1),
    }
}

/// Per-button gesture trackers (never compared; the state is not `PartialEq`).
type Trackers = HashMap<&'static str, ButtonTracker>;

/// UI state for the Fx area.
#[derive(Debug, Clone, Default)]
pub struct FxUi {
    /// Panel view of the chains of the six focus slots.
    pub chains: [ChainUi; FX_SLOTS],
    /// Piano focus is on Layer B (the legacy `fx_focus` has one Piano value).
    pub piano_b: bool,
    /// GROUP source slot per section (`None` = Layer mode, manual p.48).
    pub piano_group: Option<u8>,
    pub synth_group: Option<u8>,
    /// GLOBAL source slot per unit (manual p.48).
    pub global_delay: Option<u8>,
    pub global_comp: Option<u8>,
    pub global_reverb: Option<u8>,
    /// TAP/SET detector of the Delay.
    pub tap: TapTempo,
    /// Gesture tracker per button id (tap / hold-to-shift).
    pub trackers: Trackers,
    /// SECTION EDIT is active (manual p.43): a unit edit reaches the section's other chains.
    /// The bay copies `ProgramUi::section_edit_active()` here each frame.
    pub section_edit: bool,
    echo: Option<StateEcho>,
}

impl FxUi {
    /// Engine sharing modes (chain indices) for the current GROUP / GLOBAL sources.
    pub fn sharing(&self) -> FxSharing {
        let c = |s: Option<u8>| s.map(|s| crate::audio::chain_of_slot(s as usize));
        FxSharing {
            delay_global: c(self.global_delay),
            comp_global: c(self.global_comp),
            reverb_global: c(self.global_reverb),
            piano_group: c(self.piano_group),
            synth_group: c(self.synth_group),
        }
    }

    /// GROUP source of the section, if the mode is on.
    pub fn group_of(&self, s: FocusSection) -> Option<u8> {
        match s {
            FocusSection::Piano => self.piano_group,
            FocusSection::Synth => self.synth_group,
            FocusSection::Organ => None,
        }
    }

    /// GLOBAL source of `unit`, if the mode is on.
    pub fn global_of(&self, unit: Unit) -> Option<u8> {
        match unit {
            Unit::Delay => self.global_delay,
            Unit::Comp => self.global_comp,
            Unit::Reverb => self.global_reverb,
            _ => None,
        }
    }

    /// Press of a section's FX FOCUS button: focus the section, or step to its next layer.
    pub fn focus_press(&mut self, fx_focus: &mut usize, s: FocusSection) {
        match s {
            FocusSection::Organ => *fx_focus = 0,
            FocusSection::Piano if *fx_focus == 1 => self.piano_b = !self.piano_b,
            FocusSection::Piano => *fx_focus = 1,
            FocusSection::Synth if (2..=4).contains(fx_focus) => {
                *fx_focus = 2 + nord_panel::behaviour::cycle(*fx_focus - 2, 3, true);
            }
            FocusSection::Synth => *fx_focus = 2,
        }
    }

    /// Shift + Piano / Synth FX FOCUS: toggle GROUP; entering it applies `slot`'s settings to the
    /// whole section (manual p.48). Returns the edits to send.
    pub fn toggle_group(&mut self, s: FocusSection, slot: usize) -> Vec<FxEdit> {
        let cur = self.group_of(s);
        let next = if cur.is_some() {
            None
        } else {
            Some(slot as u8)
        };
        match s {
            FocusSection::Piano => self.piano_group = next,
            FocusSection::Synth => self.synth_group = next,
            FocusSection::Organ => return Vec::new(),
        }
        if next.is_some() {
            self.copy_section(s, slot);
        }
        vec![FxEdit::Sharing(self.sharing())]
    }

    /// Shift + On of Delay / Compressor / Reverb: toggle GLOBAL; entering it applies `slot`'s
    /// settings of that unit to every layer (manual p.48).
    pub fn toggle_global(&mut self, unit: Unit, slot: usize) -> Vec<FxEdit> {
        let cur = self.global_of(unit);
        let next = if cur.is_some() {
            None
        } else {
            Some(slot as u8)
        };
        match unit {
            Unit::Delay => self.global_delay = next,
            Unit::Comp => self.global_comp = next,
            Unit::Reverb => self.global_reverb = next,
            _ => return Vec::new(),
        }
        if next.is_some() {
            self.copy_unit(unit, slot);
        }
        vec![FxEdit::Sharing(self.sharing())]
    }

    fn copy_section(&mut self, s: FocusSection, src: usize) {
        let from = self.chains[src].clone();
        for d in section_slots(s).filter(|&d| d != src) {
            self.chains[d] = from.clone();
        }
    }

    fn copy_unit(&mut self, unit: Unit, src: usize) {
        let from = self.chains[src].clone();
        for d in (0..FX_SLOTS).filter(|&d| d != src) {
            match unit {
                Unit::Delay => self.chains[d].copy_delay_from(&from),
                Unit::Comp => self.chains[d].copy_comp_from(&from),
                Unit::Reverb => self.chains[d].copy_reverb_from(&from),
                _ => {}
            }
        }
    }

    /// An edit of `unit` on `slot` happened. While a GROUP / GLOBAL mode covers it, the edited
    /// slot becomes the engine's sharing source (otherwise the engine copy would undo the edit)
    /// and the other mirrors follow. Returns the edits to send, in order.
    pub fn after_edit(&mut self, slot: usize, unit: Unit) -> Vec<FxEdit> {
        let before = self.sharing();
        let sec = section_of_slot(slot);
        match sec {
            FocusSection::Piano if self.piano_group.is_some() => {
                self.piano_group = Some(slot as u8)
            }
            FocusSection::Synth if self.synth_group.is_some() => {
                self.synth_group = Some(slot as u8)
            }
            _ => {}
        }
        if self.group_of(sec).is_some() {
            self.copy_section(sec, slot);
        }
        if self.global_of(unit).is_some() {
            match unit {
                Unit::Delay => self.global_delay = Some(slot as u8),
                Unit::Comp => self.global_comp = Some(slot as u8),
                Unit::Reverb => self.global_reverb = Some(slot as u8),
                _ => {}
            }
            self.copy_unit(unit, slot);
        }
        let mut out = Vec::new();
        if self.sharing() != before {
            out.push(FxEdit::Sharing(self.sharing()));
        }
        out.extend(self.chains[slot].edits(unit, slot as u8));
        out.extend(self.section_edits(slot, unit));
        out
    }

    /// Adopt what an external writer (program load) changed in the legacy state view since the
    /// last frame, into the chain of `slot`. The first call only records the baseline.
    pub fn pull_external(&mut self, now: StateEcho, slot: usize) {
        if let Some(prev) = self.echo {
            if prev != now {
                echo::adopt(&mut self.chains[slot], &prev, &now);
            }
        }
    }

    /// Record the legacy state view after this frame wrote it.
    pub fn set_echo(&mut self, echo: StateEcho) {
        self.echo = Some(echo);
    }

    /// Label of the delay feedback effect LED `i` (for tooltips and tests).
    pub fn delay_fx_kind(i: usize) -> Option<DelayFeedbackFx> {
        DELAY_FX.get(i).and_then(|c| c.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_mapping_follows_the_legacy_focus_index() {
        assert_eq!(slot_for(0, false), 0);
        assert_eq!(slot_for(1, false), 1);
        assert_eq!(slot_for(1, true), 2);
        assert_eq!(
            (slot_for(2, false), slot_for(3, false), slot_for(4, false)),
            (3, 4, 5)
        );
        assert_eq!(slot_for(9, false), 5, "out of range stays in range");
    }

    #[test]
    fn sections_partition_the_slots() {
        for slot in 0..FX_SLOTS {
            assert!(section_slots(section_of_slot(slot)).contains(&slot));
        }
    }

    #[test]
    fn focus_buttons_cycle_the_layers_of_their_section() {
        let mut fx = FxUi::default();
        let mut f = 0usize;
        fx.focus_press(&mut f, FocusSection::Piano);
        assert_eq!((f, fx.piano_b), (1, false));
        fx.focus_press(&mut f, FocusSection::Piano);
        assert!(fx.piano_b);
        fx.focus_press(&mut f, FocusSection::Synth);
        assert_eq!(f, 2);
        fx.focus_press(&mut f, FocusSection::Synth);
        fx.focus_press(&mut f, FocusSection::Synth);
        assert_eq!(f, 4);
        fx.focus_press(&mut f, FocusSection::Synth);
        assert_eq!(f, 2, "Synth C wraps to A");
        fx.focus_press(&mut f, FocusSection::Organ);
        assert_eq!(f, 0);
    }

    #[test]
    fn group_toggle_copies_the_source_and_maps_to_engine_chains() {
        let mut fx = FxUi::default();
        fx.chains[4].drive = 0.7;
        let e = fx.toggle_group(FocusSection::Synth, 4);
        assert_eq!(fx.chains[3].drive, 0.7);
        assert_eq!(fx.chains[5].drive, 0.7);
        assert_eq!(fx.chains[1].drive, 0.0);
        let FxEdit::Sharing(s) = e[0] else {
            panic!("expected Sharing")
        };
        assert_eq!(s.synth_group, Some(5), "slot 4 is engine chain 5");
        assert_eq!(s.piano_group, None);
        let e = fx.toggle_group(FocusSection::Synth, 4);
        let FxEdit::Sharing(s) = e[0] else {
            panic!("expected Sharing")
        };
        assert!(!s.any());
    }

    #[test]
    fn organ_has_no_group_mode() {
        let mut fx = FxUi::default();
        assert!(fx.toggle_group(FocusSection::Organ, 0).is_empty());
        assert!(!fx.sharing().any());
    }

    #[test]
    fn global_toggle_copies_only_that_unit_to_every_layer() {
        let mut fx = FxUi::default();
        fx.chains[2].feedback = 0.8;
        fx.chains[2].drive = 0.9;
        let e = fx.toggle_global(Unit::Delay, 2);
        assert!(fx.chains.iter().all(|c| c.feedback == 0.8));
        assert_eq!(fx.chains[0].drive, 0.0);
        let FxEdit::Sharing(s) = e[0] else {
            panic!("expected Sharing")
        };
        assert_eq!(s.delay_global, Some(3));
        assert_eq!(s.comp_global, None);
    }

    #[test]
    fn editing_another_layer_while_global_repoints_the_source() {
        let mut fx = FxUi::default();
        fx.toggle_global(Unit::Reverb, 0);
        fx.chains[5].reverb_mix = 0.9;
        let e = fx.after_edit(5, Unit::Reverb);
        let FxEdit::Sharing(s) = e[0] else {
            panic!("expected Sharing first")
        };
        assert_eq!(s.reverb_global, Some(6));
        assert_eq!(fx.chains[0].reverb_mix, 0.9, "mirrors follow the edit");
        assert!(e.len() > 1, "the unit's own edits follow the sharing edit");
    }

    #[test]
    fn plain_edit_sends_no_sharing_change() {
        let mut fx = FxUi::default();
        let e = fx.after_edit(1, Unit::Comp);
        assert!(e.iter().all(|x| !matches!(x, FxEdit::Sharing(_))));
    }

    #[test]
    fn group_edit_propagates_to_the_other_layers_of_the_section() {
        let mut fx = FxUi::default();
        fx.toggle_group(FocusSection::Piano, 1);
        fx.chains[2].mod1_rate = 9.0;
        fx.after_edit(2, Unit::Mod1);
        assert_eq!(fx.chains[1].mod1_rate, 9.0);
        assert_eq!(fx.piano_group, Some(2));
    }

    #[test]
    fn delay_fx_lookup_matches_the_table() {
        assert_eq!(FxUi::delay_fx_kind(0), Some(DelayFeedbackFx::Off));
        assert_eq!(FxUi::delay_fx_kind(5), Some(DelayFeedbackFx::Space));
        assert_eq!(FxUi::delay_fx_kind(6), None);
    }
}
