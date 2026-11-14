//! SECTION EDIT (manual p.43) for the Layer Effects: "setting up just some of the effects the same
//! for a whole Section". An edit of one unit (Mod 1, Mod 2, Amp, Comp, Delay, Reverb) on a
//! Piano or Synth layer copies that unit to the other chains of the section and sends the same
//! values there. Other units stay as they are (GROUP copies the whole chain instead).

use super::{section_of_slot, section_slots, ChainUi, FxUi, Unit};
use crate::audio::FxEdit;

impl ChainUi {
    /// Copy the settings of `unit` (and the LAYER EFFECTS ON gate that scales them) from `src`.
    pub fn copy_unit_from(&mut self, unit: Unit, src: &ChainUi) {
        self.all_on = src.all_on;
        match unit {
            Unit::Mod1 => {
                self.mod1_on = src.mod1_on;
                self.mod1_sel = src.mod1_sel;
                self.mod1_rate = src.mod1_rate;
                self.mod1_amount = src.mod1_amount;
                self.mod1_variation = src.mod1_variation;
                self.mod1_ped = src.mod1_ped;
                self.mod1_ped_pos = src.mod1_ped_pos;
                self.mod1_mst = src.mod1_mst;
            }
            Unit::Mod2 => {
                self.mod2_on = src.mod2_on;
                self.mod2_sel = src.mod2_sel;
                self.mod2_rate = src.mod2_rate;
                self.mod2_amount = src.mod2_amount;
                self.mod2_variation = src.mod2_variation;
            }
            Unit::Amp => {
                self.amp_on = src.amp_on;
                self.cabinet = src.cabinet;
                self.amp_variation = src.amp_variation;
                self.drive = src.drive;
                self.bass = src.bass;
                self.mid = src.mid;
                self.mid_freq = src.mid_freq;
                self.treble = src.treble;
                self.res = src.res;
            }
            Unit::Comp => self.copy_comp_from(src),
            Unit::Delay => self.copy_delay_from(src),
            Unit::Reverb => self.copy_reverb_from(src),
        }
    }
}

impl FxUi {
    /// An edit of `unit` on `slot` happened while SECTION EDIT is active: copy the unit to the
    /// other chains of the section and return the edits for them. Empty when SECTION EDIT is off,
    /// the section has one chain (Organ) or GROUP already shares the whole chain.
    pub(super) fn section_edits(&mut self, slot: usize, unit: Unit) -> Vec<FxEdit> {
        let sec = section_of_slot(slot);
        if !self.section_edit || self.group_of(sec).is_some() {
            return Vec::new();
        }
        let from = self.chains[slot].clone();
        let mut out = Vec::new();
        for d in section_slots(sec).filter(|&d| d != slot) {
            self.chains[d].copy_unit_from(unit, &from);
            out.extend(self.chains[d].edits(unit, d as u8));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::FxFloat;

    #[test]
    fn copy_unit_moves_only_that_unit() {
        let src = ChainUi {
            mod1_rate: 9.0,
            mod2_rate: 5.0,
            drive: 0.7,
            all_on: false,
            ..ChainUi::default()
        };
        let mut d = ChainUi::default();
        d.copy_unit_from(Unit::Mod1, &src);
        assert_eq!((d.mod1_rate, d.mod2_rate, d.drive), (9.0, 0.8, 0.0));
        assert!(!d.all_on, "the gate that scales the unit follows");
        d.copy_unit_from(Unit::Amp, &src);
        assert_eq!(d.drive, 0.7);
    }

    #[test]
    fn section_edit_copies_a_unit_to_the_sibling_chains() {
        let mut fx = FxUi {
            section_edit: true,
            ..FxUi::default()
        };
        fx.chains[3].reverb_mix = 0.8;
        let e = fx.after_edit(3, Unit::Reverb);
        assert!(fx.chains[4].reverb_mix == 0.8 && fx.chains[5].reverb_mix == 0.8);
        for slot in [3u8, 4, 5] {
            assert!(e.contains(&FxEdit::Float {
                slot,
                param: FxFloat::ReverbMix,
                value: 0.8
            }));
        }
        assert_eq!(fx.chains[1].reverb_mix, 0.15, "Piano keeps its own chain");
    }

    #[test]
    fn nothing_extra_without_section_edit_or_with_one_chain() {
        let mut fx = FxUi::default();
        fx.chains[1].reverb_mix = 0.8;
        let e = fx.after_edit(1, Unit::Reverb);
        assert_eq!(fx.chains[2].reverb_mix, 0.15);
        assert!(e.iter().all(|x| matches!(
            x,
            FxEdit::Float { slot: 1, .. }
                | FxEdit::ReverbModel { slot: 1, .. }
                | FxEdit::ReverbColor { slot: 1, .. }
                | FxEdit::Flag { slot: 1, .. }
        )));
        fx.section_edit = true;
        assert!(fx.after_edit(0, Unit::Reverb).iter().all(|x| !matches!(
            x,
            FxEdit::Float { slot, .. } if *slot != 0
        )));
    }
}
