//! Button functions: primary, SHIFT and hold (manual p.12, p.27-37, p.44), GROUP and Sound Init.

use super::buttons::Btn;
use super::layer::SynthLayerUi;
use super::page::SynthPage;
use super::tables::{PRIORITIES, VIBRATO_MODES};
use super::{Scope, SynthUi};
use crate::audio::{SynthEdit, SynthGroup, SynthParam};
use nord_panel::behaviour::{cycle, Trigger};

impl SynthUi {
    /// Run a button function. `t` comes from `behaviour::resolve`.
    pub fn press(&mut self, b: Btn, t: Trigger) {
        let shifted = t == Trigger::Shifted;
        match (b, t) {
            (Btn::On, Trigger::Hold) => self.solo = !self.solo,
            (Btn::On, _) => self.section_on = !self.section_on,
            (Btn::Mode, _) if shifted => self.toggle_extern(),
            (Btn::Mode, _) => {
                let v = !self.layer().sample_mode;
                self.layer_mut().sample_mode = v;
                self.page = SynthPage::Waveform;
                self.resend_osc();
            }
            (Btn::Waveform, _) if shifted => self.sound_init(),
            (Btn::Waveform, _) => self.open(SynthPage::Waveform),
            (Btn::PitchSmp, _) if shifted => {
                let v = !self.layer().env_to_pitch;
                self.edit(Scope::Layer, SynthParam::OscEnvToPitch(v), |l| {
                    l.env_to_pitch = v
                });
            }
            (Btn::PitchSmp, _) => self.open(SynthPage::PitchSmp),
            (Btn::OscEnvelope, _) if shifted => {
                let v = !self.layer().osc_vel;
                self.edit(Scope::Layer, SynthParam::OscEnvVelocity(v), |l| {
                    l.osc_vel = v
                });
            }
            (Btn::OscEnvelope, _) => self.open(SynthPage::EnvOsc),
            (Btn::FilterType, _) if shifted => self.toggle_group(SynthGroup::Filter),
            (Btn::FilterType, _) => self.open(SynthPage::FilterType),
            (Btn::FilterEnvelope, _) if shifted => {
                let v = !self.layer().filter_vel;
                self.edit(
                    Scope::Filter,
                    SynthParam::FilterEnvVelocity(if v { 1.0 } else { 0.0 }),
                    |l| l.filter_vel = v,
                );
            }
            (Btn::FilterEnvelope, _) => self.open(SynthPage::EnvFilter),
            (Btn::AmpEnvelope, _) if shifted => {
                let v = (self.layer().amp_vel + 1) % 4;
                self.edit(
                    Scope::Layer,
                    SynthParam::AmpVelocity(f32::from(v) / 3.0),
                    |l| l.amp_vel = v,
                );
            }
            (Btn::AmpEnvelope, _) => self.open(SynthPage::EnvAmp),
            (Btn::LfoWaveform, _) if shifted => self.toggle_group(SynthGroup::Lfo),
            (Btn::LfoWaveform, _) => self.open(SynthPage::LfoWave),
            (Btn::LfoDest, _) => {
                // VERIFIED (video): hardware taps run Off, OSC CTRL, OSC PITCH, FILTER (both
                // LEDs), Off; the table is in LED order, so the cycle steps backwards.
                let d = cycle(usize::from(self.layer().lfo_dest), 4, false) as u8;
                let dest = super::tables::LFO_DESTS[usize::from(d)].1;
                self.edit(Scope::Lfo, SynthParam::LfoDest(dest), |l| l.lfo_dest = d);
            }
            (Btn::VoiceMode, _) if shifted => {
                let p = cycle(usize::from(self.layer().priority), 3, true) as u8;
                self.edit(
                    Scope::Layer,
                    SynthParam::Priority(PRIORITIES[usize::from(p)].1),
                    |l| l.priority = p,
                );
            }
            (Btn::VoiceMode, _) => {
                let v = cycle(usize::from(self.layer().voice), 3, true) as u8;
                self.edit(Scope::Layer, SynthParam::Voice(v), |l| l.voice = v);
            }
            (Btn::Unison, _) => {
                let u = cycle(usize::from(self.layer().unison), 4, true) as u8;
                self.edit(Scope::Layer, SynthParam::Unison(u), |l| l.unison = u);
            }
            (Btn::VibratoSel, _) => {
                let m = cycle(usize::from(self.layer().vib_mode), 6, true) as u8;
                self.edit(
                    Scope::Layer,
                    SynthParam::VibratoMode(VIBRATO_MODES[usize::from(m)].1),
                    |l| l.vib_mode = m,
                );
            }
            (Btn::VibratoMenu, _) => self.open(SynthPage::Vibrato),
            (Btn::ArpRun, _) if shifted => {
                let v = !self.layer().kb_sync;
                self.edit(Scope::Arp, SynthParam::KbSync(v), |l| l.kb_sync = v);
            }
            (Btn::ArpRun, _) => {
                let v = !self.layer().arp_run;
                self.edit_arp_mode(|l| l.arp_run = v);
            }
            (Btn::ArpMode, _) if shifted => {
                let v = !self.layer().pattern_on;
                self.edit(Scope::Arp, SynthParam::PatternOn(v), |l| l.pattern_on = v);
            }
            (Btn::ArpMode, _) => {
                let k = cycle(usize::from(self.layer().arp_kind), 3, true) as u8;
                self.edit_arp_mode(|l| l.arp_kind = k);
            }
            (Btn::ArpMenu, _) if shifted => self.toggle_group(SynthGroup::Arp),
            (Btn::ArpMenu, _) => {
                if matches!(self.page, SynthPage::Arp(_)) {
                    self.open(self.page);
                } else {
                    self.open(SynthPage::Arp(1));
                }
            }
            (Btn::KbHold, _) if shifted => {
                let v = !self.layer().kb_hold_exclude;
                self.edit(Scope::Layer, SynthParam::KbHoldExclude(v), |l| {
                    l.kb_hold_exclude = v
                });
            }
            (Btn::KbHold, _) => {
                self.kb_hold = !self.kb_hold;
                let v = self.kb_hold;
                self.edit(Scope::Section, SynthParam::KbHold(v), |_| {});
            }
            (Btn::OctaveDown | Btn::OctaveUp, _) => {
                let dir = if b == Btn::OctaveUp { 1 } else { -1 };
                if shifted {
                    let z = crate::panel::kb_zone::step(self.layer().kb_zone, dir > 0);
                    self.layer_mut().kb_zone = z;
                    self.out.push(SynthEdit::Zone {
                        layer: self.focus.min(2) as u8,
                        leds: crate::panel::kb_zone::leds(z),
                    });
                } else {
                    let o = (i32::from(self.layer().octave) + dir).clamp(-3, 3) as i8;
                    self.edit(Scope::Layer, SynthParam::Octave(o), |l| l.octave = o);
                }
            }
        }
    }

    /// EXTERN (Shift+Mode, manual p.46): switch the focused layer into or out of Extern mode and
    /// show the Extern page while it is on. ASSUMED: leaving Extern returns to the waveform page.
    fn toggle_extern(&mut self) {
        let on = !self.layer().extern_on;
        self.layer_mut().extern_on = on;
        self.out.push(SynthEdit::Extern {
            layer: self.focus.min(2) as u8,
            on,
        });
        self.page = if on {
            SynthPage::Extern
        } else {
            SynthPage::Waveform
        };
        self.list_view = false;
        self.info_view = false;
        self.dial_acc = [0.0; 3];
    }

    /// Apply `f` to the arp run/kind selection and send the resulting engine mode.
    fn edit_arp_mode(&mut self, f: impl Fn(&mut SynthLayerUi) + Copy) {
        let mut t = self.layer().clone();
        f(&mut t);
        self.edit(Scope::Arp, SynthParam::ArpMode(t.arp_engine_mode()), f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{SynthEdit, SynthTarget};
    use nord_core::synth::SynthOscMode;
    use nord_core::synth_types::{ArpMode, LfoDestination, NotePriority, VibratoMode};
    use nord_panel::behaviour::resolve;

    fn params(s: &mut SynthUi) -> Vec<SynthParam> {
        s.take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }

    fn tap(s: &mut SynthUi, b: Btn) {
        s.press(b, Trigger::Primary);
    }

    fn shifted(s: &mut SynthUi, b: Btn) {
        s.press(b, Trigger::Shifted);
    }

    #[test]
    fn mode_toggles_samples_and_analog_and_shift_opens_extern() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::Mode);
        assert!(s.layer().sample_mode);
        assert!(params(&mut s).contains(&SynthParam::OscMode(SynthOscMode::Sample)));
        tap(&mut s, Btn::Mode);
        assert!(!s.layer().sample_mode);
        shifted(&mut s, Btn::Mode);
        assert_eq!(s.page, SynthPage::Extern);
        assert!(s.layer().extern_on);
        assert_eq!(
            s.take_edits().last(),
            Some(&SynthEdit::Extern { layer: 0, on: true })
        );
        shifted(&mut s, Btn::Mode);
        assert!(!s.layer().extern_on && s.page == SynthPage::Waveform);
        assert_eq!(
            s.take_edits(),
            vec![SynthEdit::Extern {
                layer: 0,
                on: false
            }]
        );
    }

    #[test]
    fn function_buttons_open_their_pages_and_toggle_back() {
        let mut s = SynthUi::new();
        for (b, p) in [
            (Btn::FilterType, SynthPage::FilterType),
            (Btn::OscEnvelope, SynthPage::EnvOsc),
            (Btn::FilterEnvelope, SynthPage::EnvFilter),
            (Btn::AmpEnvelope, SynthPage::EnvAmp),
            (Btn::LfoWaveform, SynthPage::LfoWave),
            (Btn::VibratoMenu, SynthPage::Vibrato),
            (Btn::PitchSmp, SynthPage::PitchSmp),
            (Btn::ArpMenu, SynthPage::Arp(1)),
        ] {
            tap(&mut s, b);
            assert_eq!(s.page, p, "{b:?}");
            assert_eq!(p.button_id(), b.id());
            tap(&mut s, b);
            assert_eq!(s.page, SynthPage::Waveform, "{b:?} toggles back");
        }
        tap(&mut s, Btn::Waveform);
        assert_eq!(s.page, SynthPage::Waveform);
    }

    #[test]
    fn arp_menu_button_leaves_any_menu_page() {
        let mut s = SynthUi::new();
        s.page = SynthPage::Arp(4);
        tap(&mut s, Btn::ArpMenu);
        assert_eq!(s.page, SynthPage::Waveform);
    }

    #[test]
    fn shift_functions_toggle_velocity_and_env_to_pitch() {
        let mut s = SynthUi::new();
        shifted(&mut s, Btn::PitchSmp);
        shifted(&mut s, Btn::OscEnvelope);
        shifted(&mut s, Btn::FilterEnvelope);
        shifted(&mut s, Btn::AmpEnvelope);
        assert!(s.layer().env_to_pitch && s.layer().osc_vel && s.layer().filter_vel);
        assert_eq!(s.layer().amp_vel, 0, "amp velocity cycles 3 -> 0");
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::OscEnvToPitch(true)));
        assert!(p.contains(&SynthParam::OscEnvVelocity(true)));
        assert!(p.contains(&SynthParam::FilterEnvVelocity(1.0)));
        assert!(p.contains(&SynthParam::AmpVelocity(0.0)));
        shifted(&mut s, Btn::AmpEnvelope);
        assert_eq!(params(&mut s), vec![SynthParam::AmpVelocity(1.0 / 3.0)]);
        assert_eq!(
            s.page,
            SynthPage::Waveform,
            "shift functions do not change the page"
        );
    }

    #[test]
    fn lfo_destination_taps_follow_the_hardware_order() {
        // DSoundman LFO video 0:30-0:35: Off -> OSC CTRL -> OSC PITCH -> FILTER.
        let mut s = SynthUi::new();
        let mut seen = Vec::new();
        for _ in 0..4 {
            tap(&mut s, Btn::LfoDest);
            seen.extend(params(&mut s));
        }
        assert_eq!(seen[0], SynthParam::LfoDest(LfoDestination::Off));
        assert_eq!(seen[1], SynthParam::LfoDest(LfoDestination::OscCtrl));
        assert_eq!(seen[2], SynthParam::LfoDest(LfoDestination::OscPitch));
        assert_eq!(seen[3], SynthParam::LfoDest(LfoDestination::FilterCutoff));
        assert_eq!(s.layer().lfo_dest, 0);
    }

    #[test]
    fn voice_unison_and_priority_buttons_cycle() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::VoiceMode);
        tap(&mut s, Btn::VoiceMode);
        assert_eq!(s.layer().voice, 2);
        tap(&mut s, Btn::VoiceMode);
        assert_eq!(s.layer().voice, 0);
        shifted(&mut s, Btn::VoiceMode);
        shifted(&mut s, Btn::VoiceMode);
        assert_eq!(s.layer().priority, 2);
        for _ in 0..4 {
            tap(&mut s, Btn::Unison);
        }
        assert_eq!(s.layer().unison, 0);
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::Voice(2)));
        assert!(p.contains(&SynthParam::Priority(NotePriority::Hi)));
        assert!(p.contains(&SynthParam::Unison(3)));
    }

    #[test]
    fn vibrato_selector_cycles_six_positions() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::VibratoSel);
        assert!(params(&mut s).contains(&SynthParam::VibratoMode(VibratoMode::On)));
        for _ in 0..5 {
            tap(&mut s, Btn::VibratoSel);
        }
        assert_eq!(s.layer().vib_mode, 0);
    }

    #[test]
    fn arp_run_mode_pattern_and_kb_sync() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::ArpRun);
        assert!(params(&mut s).contains(&SynthParam::ArpMode(ArpMode::Up)));
        tap(&mut s, Btn::ArpMode);
        assert!(params(&mut s).contains(&SynthParam::ArpMode(ArpMode::Poly)));
        tap(&mut s, Btn::ArpMode);
        assert!(params(&mut s).contains(&SynthParam::ArpMode(ArpMode::Gate)));
        shifted(&mut s, Btn::ArpMode);
        shifted(&mut s, Btn::ArpRun);
        assert!(s.layer().pattern_on && s.layer().kb_sync);
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::PatternOn(true)));
        assert!(p.contains(&SynthParam::KbSync(true)));
        tap(&mut s, Btn::ArpRun);
        assert!(params(&mut s).contains(&SynthParam::ArpMode(ArpMode::Off)));
    }

    #[test]
    fn kb_hold_is_section_wide_and_exclude_is_per_layer() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::KbHold);
        assert!(s.kb_hold);
        assert!(matches!(
            s.take_edits()[0],
            SynthEdit::Param(SynthTarget::All, SynthParam::KbHold(true))
        ));
        s.focus_layer(1);
        shifted(&mut s, Btn::KbHold);
        assert!(s.layers[1].kb_hold_exclude && !s.layers[0].kb_hold_exclude);
        assert!(matches!(
            s.take_edits()[0],
            SynthEdit::Param(SynthTarget::Layer(1), SynthParam::KbHoldExclude(true))
        ));
    }

    #[test]
    fn octave_buttons_clamp_and_shift_moves_the_kb_zone() {
        let mut s = SynthUi::new();
        for _ in 0..5 {
            tap(&mut s, Btn::OctaveUp);
        }
        assert_eq!(s.layer().octave, 3);
        tap(&mut s, Btn::OctaveDown);
        assert!(params(&mut s).contains(&SynthParam::Octave(2)));
        shifted(&mut s, Btn::OctaveUp);
        shifted(&mut s, Btn::OctaveUp);
        shifted(&mut s, Btn::OctaveDown);
        assert_eq!(s.layer().kb_zone, 1);
        let zones: Vec<_> = s
            .take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Zone { layer, leds } => Some((layer, leds)),
                _ => None,
            })
            .collect();
        assert_eq!(zones, vec![(0, 0b0001), (0, 0b0010), (0, 0b0001)]);
        assert_eq!(s.layer().octave, 2);
    }

    #[test]
    fn on_toggles_the_section_and_hold_toggles_solo() {
        let mut s = SynthUi::new();
        tap(&mut s, Btn::On);
        assert!(!s.section_on);
        s.press(Btn::On, Trigger::Hold);
        assert!(s.solo);
        assert!(!s.section_on, "hold does not toggle the section");
    }

    #[test]
    fn long_press_resolves_to_the_shifted_function() {
        use nord_panel::behaviour::{ButtonTracker, HOLD_MS};
        let mut tr = ButtonTracker::new();
        tr.press(0);
        let g = tr.tick(HOLD_MS).unwrap();
        let mut s = SynthUi::new();
        let t = resolve(g, false, Btn::FilterType.has_shift_fn());
        s.press(Btn::FilterType, t);
        assert!(s.group_filter.is_active(), "hold-to-shift reached GROUP");
        assert!(!Btn::On.has_shift_fn());
    }
}
