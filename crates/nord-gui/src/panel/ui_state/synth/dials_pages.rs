//! Dial functions of the Arpeggiator menu, Vibrato menu and PITCH/SMP pages (manual p.28, p.36-37).

use super::page::{SynthPage, ARP_PAGES};
use super::tables::VIBRATO_DELAYS;
use super::{Scope, SynthUi};
#[cfg(test)]
use crate::audio::PatternParam;
use crate::audio::SynthParam;

fn bump(v: u8, steps: i32, max: u8) -> u8 {
    (i32::from(v) + steps).clamp(0, i32::from(max)) as u8
}

impl SynthUi {
    /// Arpeggiator MENU page `n`; the left dial navigates, the other two edit the page.
    pub(super) fn dial_arp(&mut self, n: u8, dial: usize, steps: i32) {
        if dial == 0 {
            let p = (i32::from(n) + steps).clamp(1, i32::from(ARP_PAGES)) as u8;
            self.page = SynthPage::Arp(p);
            return;
        }
        match (n, dial) {
            (1, 1) => {
                let dir = bump(self.layer().arp_dir, steps, 3);
                let mut t = self.layer().clone();
                t.arp_dir = dir;
                self.edit(Scope::Arp, SynthParam::ArpMode(t.arp_engine_mode()), |l| {
                    l.arp_dir = dir
                });
            }
            (1, _) => self.set_zigzag(steps > 0),
            (2, 1) => {
                let p = bump(self.layer().pattern_preset, steps, 7);
                self.choose_preset(p);
            }
            (2, _) => self.step_pattern_len(steps),
            (_, 1) => self.move_cursor(steps),
            (3, _) => self.step_gate(steps),
            (4, _) => self.step_accent(steps),
            _ => self.step_pan(steps),
        }
    }

    pub(super) fn dial_vibrato(&mut self, dial: usize, steps: i32) {
        match dial {
            0 => {
                let v = (self.layer().vib_rate + steps as f32 * 0.1).clamp(2.0, 8.0);
                self.edit(Scope::Layer, SynthParam::VibratoRate(v), |l| l.vib_rate = v);
            }
            1 => {
                let v = (self.layer().vib_amount + steps as f32 * 0.25).clamp(0.0, 10.0);
                // ASSUMED: Amount 10 = 100 cents (the host range of `synthm_vib_depth`).
                self.edit(Scope::Layer, SynthParam::VibratoDepth(v * 10.0), |l| {
                    l.vib_amount = v
                });
            }
            _ => {
                let d = bump(self.layer().vib_delay, steps, 3);
                self.edit(
                    Scope::Layer,
                    SynthParam::VibratoDelay(VIBRATO_DELAYS[usize::from(d)]),
                    |l| l.vib_delay = d,
                );
            }
        }
    }

    pub(super) fn dial_pitch(&mut self, dial: usize, steps: i32) {
        match dial {
            0 | 1 => {
                let l = self.layer();
                let (mut semi, mut fine) = (l.pitch_semi, l.fine_cents);
                if dial == 0 {
                    semi = (i32::from(semi) + steps).clamp(-24, 24) as i8;
                } else {
                    fine = (i32::from(fine) + steps).clamp(-50, 50) as i8;
                }
                let cents = f32::from(semi) * 100.0 + f32::from(fine);
                self.edit(Scope::Layer, SynthParam::PitchCents(cents), |l| {
                    l.pitch_semi = semi;
                    l.fine_cents = fine;
                });
            }
            _ if self.layer().sample_mode => {
                let l = self.layer_mut();
                l.sample_dyn = bump(l.sample_dyn, steps, 2);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SynthEdit;

    fn params(s: &mut SynthUi) -> Vec<SynthParam> {
        s.take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn arp_menu_left_dial_navigates_five_pages() {
        let mut s = SynthUi::new();
        s.open(SynthPage::Arp(1));
        s.turn_dial(0, 1, false);
        assert_eq!(s.page, SynthPage::Arp(2));
        s.turn_dial(0, 10, false);
        assert_eq!(s.page, SynthPage::Arp(5));
        s.turn_dial(0, -10, false);
        assert_eq!(s.page, SynthPage::Arp(1));
    }

    #[test]
    fn arp_pages_set_direction_preset_and_keep_ui_only_values() {
        let mut s = SynthUi::new();
        s.layer_mut().arp_run = true;
        s.open(SynthPage::Arp(1));
        s.turn_dial(1, 1, false);
        assert!(
            params(&mut s).contains(&SynthParam::ArpMode(nord_core::synth_types::ArpMode::Down))
        );
        s.take_edits();
        s.turn_dial(2, 1, false);
        assert!(s.layer().zigzag);
        assert_eq!(
            params(&mut s),
            vec![SynthParam::Pattern(PatternParam::Zigzag(true))]
        );
        s.turn_dial(2, -1, false);
        assert!(!s.layer().zigzag);
        s.page = SynthPage::Arp(2);
        s.turn_dial(1, 3, false);
        assert_eq!(s.layer().pattern_preset, 3);
        assert!(params(&mut s).contains(&SynthParam::PatternPreset(3)));
        s.turn_dial(2, 4, false);
        assert_eq!(s.layer().pattern_len, 12, "preset 4 has 8 steps, +4");
        assert!(params(&mut s).contains(&SynthParam::Pattern(PatternParam::Len(12))));
    }

    #[test]
    fn pattern_pages_edit_gate_accent_and_pan_of_the_cursor_step() {
        let mut s = SynthUi::new();
        s.page = SynthPage::Arp(3);
        s.turn_dial(1, 2, false);
        assert_eq!(s.layer().pattern_cursor, 2);
        s.take_edits();
        s.turn_dial(2, 2, false);
        assert!(
            params(&mut s).contains(&SynthParam::Pattern(PatternParam::Gate {
                step: 2,
                eighths: 6
            }))
        );
        s.page = SynthPage::Arp(4);
        s.turn_dial(2, 1, false);
        assert!(
            params(&mut s).contains(&SynthParam::Pattern(PatternParam::Accent {
                step: 2,
                on: true
            }))
        );
        s.page = SynthPage::Arp(5);
        s.turn_dial(2, -1, false);
        assert!(
            params(&mut s).contains(&SynthParam::Pattern(PatternParam::Pan {
                step: 2,
                pan: nord_core::synth::PatternPan::Left
            }))
        );
        for n in 1..=5 {
            s.page = SynthPage::Arp(n);
            assert!(s.dial_unsupported(1).is_none() && s.dial_unsupported(2).is_none());
        }
    }

    #[test]
    fn vibrato_page_sets_rate_amount_and_delay() {
        let mut s = SynthUi::new();
        s.open(SynthPage::Vibrato);
        s.turn_dial(0, 5, false);
        s.turn_dial(1, 4, false);
        s.turn_dial(2, 2, false);
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::VibratoRate(5.5)));
        assert!(p.contains(&SynthParam::VibratoDepth(10.0)));
        assert!(p.contains(&SynthParam::VibratoDelay(1.2)));
        s.turn_dial(0, 1000, false);
        assert_eq!(s.layer().vib_rate, 8.0);
    }

    #[test]
    fn pitch_page_sets_semitones_and_fine_cents() {
        let mut s = SynthUi::new();
        s.open(SynthPage::PitchSmp);
        s.turn_dial(0, 100, false);
        s.turn_dial(1, -7, false);
        assert_eq!((s.layer().pitch_semi, s.layer().fine_cents), (24, -7));
        assert!(params(&mut s).contains(&SynthParam::PitchCents(2393.0)));
        s.layer_mut().sample_mode = true;
        s.turn_dial(2, 1, false);
        assert_eq!(s.layer().sample_dyn, 1);
    }
}
