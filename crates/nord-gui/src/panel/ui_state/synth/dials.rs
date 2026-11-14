//! The three OLED dials: what each does on the current page (manual p.27-38).

use super::layer::step_time;
use super::page::{dial_defs, SynthPage};
use super::tables::FILTER_TYPES;
use super::{Scope, SynthUi};
use crate::audio::{SynthEdit, SynthParam};
use nord_core::synth::SampleTimbre;

fn bump(v: u8, steps: i32, max: u8) -> u8 {
    (i32::from(v) + steps).clamp(0, i32::from(max)) as u8
}

impl SynthUi {
    /// Why dial `dial` has no engine effect on the current page, `None` when it works.
    pub fn dial_unsupported(&self, dial: usize) -> Option<&'static str> {
        dial_defs(self.page, self.layer().osc_mode())
            .get(dial)
            .copied()
            .flatten()
            .and_then(|d| d.unsupported)
    }

    /// The Waveform button is held: KEEP EDITS while browsing samples (manual p.29).
    pub fn keep_edits(&self) -> bool {
        self.trackers
            .get("synth.waveform")
            .is_some_and(|t| t.is_down())
    }

    /// Turn dial `dial` (0 left, 1 middle, 2 right) by `steps` clicks. `shift` is SHIFT held.
    pub fn turn_dial(&mut self, dial: usize, steps: i32, shift: bool) {
        if steps == 0 || dial > 2 {
            return;
        }
        match self.page {
            SynthPage::Waveform => self.dial_waveform(dial, steps, shift),
            SynthPage::FilterType => self.dial_filter(dial, steps),
            SynthPage::EnvOsc | SynthPage::EnvFilter | SynthPage::EnvAmp => {
                self.dial_envelope(dial, steps)
            }
            SynthPage::LfoWave => {
                if dial == 0 {
                    let v = bump(self.layer().lfo_shape, steps, 4);
                    let shape = super::tables::LFO_SHAPES[usize::from(v)].1;
                    self.edit(Scope::Lfo, SynthParam::LfoShape(shape), |l| l.lfo_shape = v);
                }
            }
            SynthPage::Arp(n) => self.dial_arp(n, dial, steps),
            SynthPage::Vibrato => self.dial_vibrato(dial, steps),
            SynthPage::PitchSmp => self.dial_pitch(dial, steps),
            SynthPage::Extern => self.dial_extern(dial, steps),
        }
    }

    /// Extern page: CC VAL 1, CC VAL 2 and PROGRAM dials of the focused Extern layer. The value is
    /// stored and sent as it changes, like the hardware dial (manual p.47).
    fn dial_extern(&mut self, dial: usize, steps: i32) {
        let v = bump(self.layer().extern_vals[dial], steps, 127);
        self.layer_mut().extern_vals[dial] = v;
        let layer = self.focus.min(2) as u8;
        self.out.push(if dial == 2 {
            SynthEdit::ExternProgram { layer, program: v }
        } else {
            SynthEdit::ExternCc {
                layer,
                slot: dial as u8,
                value: v,
            }
        });
    }

    fn dial_waveform(&mut self, dial: usize, steps: i32, shift: bool) {
        self.list_view = shift && dial != 0;
        if self.layer().sample_mode {
            match dial {
                0 if shift => self.info_view = !self.info_view,
                0 => {
                    let on = steps > 0;
                    self.edit(
                        Scope::Layer,
                        SynthParam::SampleTimbre(if on {
                            SampleTimbre::Bright
                        } else {
                            SampleTimbre::Off
                        }),
                        |l| l.sample_bright = on,
                    );
                }
                // CAT / SAMPLE: browse the opened backup's samples and queue the load. Holding
                // Waveform while turning is KEEP EDITS (manual p.29): the engine only swaps the
                // audio and never overwrites the layer's edits, so it holds in both cases.
                1 => self.samples.turn_cat(self.focus.min(2), steps),
                _ => self.samples.turn_sample(self.focus.min(2), steps),
            }
            return;
        }
        match dial {
            0 => {
                let t = bump(self.layer().wave_type, steps, 3);
                let l = self.layer_mut();
                if t != l.wave_type {
                    l.wave_type = t;
                    l.wave_cat = 0;
                    l.wave_idx = 0;
                    l.fm_algo = 0;
                }
            }
            1 => {
                let l = self.layer_mut();
                let max = l.cat_count() - 1;
                if l.wave_type == 1 || l.wave_type == 2 {
                    l.fm_algo = bump(l.fm_algo, steps, max);
                } else {
                    l.wave_cat = bump(l.wave_cat, steps, max);
                }
                l.wave_idx = l.wave_idx.min(l.wave_count() - 1);
            }
            _ => {
                let l = self.layer_mut();
                l.wave_idx = bump(l.wave_idx, steps, l.wave_count() - 1);
            }
        }
        self.resend_osc();
    }

    fn dial_filter(&mut self, dial: usize, steps: i32) {
        match dial {
            0 => {
                let t = bump(
                    self.layer().filter_type,
                    steps,
                    FILTER_TYPES.len() as u8 - 1,
                );
                match FILTER_TYPES[usize::from(t)].model {
                    Some(m) => self.edit(Scope::Filter, SynthParam::FilterModel(m), |l| {
                        l.filter_type = t
                    }),
                    // LP+HP has no engine model: the panel keeps the choice, the sound is unchanged.
                    None => self.layer_mut().filter_type = t,
                }
            }
            1 => {
                let v = bump(self.layer().kb_track, steps, 3);
                self.edit(Scope::Filter, SynthParam::KbTrack(v), |l| l.kb_track = v);
            }
            _ => {
                let v = bump(self.layer().drive, steps, 3);
                self.edit(
                    Scope::Filter,
                    SynthParam::FilterDrive(f32::from(v) / 3.0),
                    |l| l.drive = v,
                );
            }
        }
    }

    fn dial_envelope(&mut self, dial: usize, steps: i32) {
        let page = self.page;
        let cur = match page {
            SynthPage::EnvOsc => self.layer().osc_env,
            SynthPage::EnvFilter => self.layer().filter_env,
            _ => self.layer().amp_env,
        };
        let (attack, decay, release) = (cur.attack, cur.decay, cur.release);
        let v = step_time([attack, decay, release][dial], steps);
        use SynthParam as P;
        let (scope, param) = match (page, dial) {
            (SynthPage::EnvOsc, 0) => (Scope::Layer, P::OscEnvAttack(v)),
            (SynthPage::EnvOsc, 1) => (Scope::Layer, P::OscEnvDecay(v)),
            (SynthPage::EnvOsc, _) => (Scope::Layer, P::OscEnvRelease(v)),
            (SynthPage::EnvFilter, 0) => (Scope::Filter, P::FilterEnvAttack(v)),
            (SynthPage::EnvFilter, 1) => (Scope::Filter, P::FilterEnvDecay(v)),
            (SynthPage::EnvFilter, _) => (Scope::Filter, P::FilterEnvRelease(v)),
            (_, 0) => (Scope::Layer, P::AmpAttack(v)),
            (_, 1) => (Scope::Layer, P::AmpDecay(v)),
            (_, _) => (Scope::Layer, P::AmpRelease(v)),
        };
        self.edit(scope, param, move |l| {
            let e = match page {
                SynthPage::EnvOsc => &mut l.osc_env,
                SynthPage::EnvFilter => &mut l.filter_env,
                _ => &mut l.amp_env,
            };
            match dial {
                0 => e.attack = v,
                1 => e.decay = v,
                _ => e.release = v,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{SynthEdit, SynthTarget};
    use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};

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
    fn waveform_type_dial_switches_the_engine_oscillator_mode() {
        let mut s = SynthUi::new();
        s.turn_dial(0, 1, false);
        assert_eq!(s.layer().wave_type, 1);
        assert!(params(&mut s).contains(&SynthParam::OscMode(SynthOscMode::Fm)));
        s.turn_dial(0, 2, false);
        assert_eq!(s.layer().wave_type, 3);
        assert!(params(&mut s).contains(&SynthParam::OscMode(SynthOscMode::Wavetable)));
        s.turn_dial(0, -9, false);
        assert_eq!(s.layer().wave_type, 0, "clamped at the first type");
        assert!(params(&mut s).contains(&SynthParam::OscMode(SynthOscMode::Analog)));
    }

    #[test]
    fn category_and_wave_dials_pick_an_analog_waveform() {
        let mut s = SynthUi::new();
        s.turn_dial(1, 5, false); // SUPER
        assert_eq!(s.layer().cat_name(), "SUPER");
        s.take_edits();
        s.turn_dial(2, -5, false);
        assert_eq!(s.layer().wave_idx, 0);
        assert_eq!(s.layer().wave_name(), "Super Saw");
        assert!(params(&mut s).contains(&SynthParam::Waveform(VaWaveform::SuperSaw)));
    }

    #[test]
    fn wave_dial_is_clamped_to_the_category() {
        let mut s = SynthUi::new();
        s.turn_dial(2, 100, false);
        assert_eq!(s.layer().wave_idx, 6, "PURE has seven waveforms");
        s.turn_dial(1, 1, false); // SUB OSC has three
        assert_eq!(s.layer().wave_idx, 2);
    }

    #[test]
    fn fm_pages_drive_the_ratio_and_wave_pages_the_table() {
        let mut s = SynthUi::new();
        s.turn_dial(0, 1, false);
        s.take_edits();
        s.turn_dial(2, 4, false);
        assert!(params(&mut s).contains(&SynthParam::FmRatio(4.0)));
        s.turn_dial(0, 2, false);
        s.turn_dial(1, 1, false);
        s.turn_dial(2, 3, false);
        assert!(params(&mut s).contains(&SynthParam::WavetableIndex(11)));
    }

    #[test]
    fn shift_turn_enters_list_view_and_shift_left_toggles_info_in_samples() {
        let mut s = SynthUi::new();
        s.turn_dial(2, 1, true);
        assert!(s.list_view);
        s.turn_dial(2, 1, false);
        assert!(!s.list_view);
        s.layer_mut().sample_mode = true;
        s.turn_dial(0, 1, true);
        assert!(s.info_view);
        s.turn_dial(0, 1, true);
        assert!(!s.info_view);
    }

    #[test]
    fn sample_bright_dial_sets_the_timbre_and_cat_sample_dials_browse_the_library() {
        let mut s = SynthUi::new();
        s.layer_mut().sample_mode = true;
        s.turn_dial(0, 1, false);
        assert!(s.layer().sample_bright);
        assert!(params(&mut s).contains(&SynthParam::SampleTimbre(SampleTimbre::Bright)));
        s.turn_dial(1, 3, false);
        s.turn_dial(2, 3, false);
        assert!(s.take_edits().is_empty(), "no library, nothing queued");
        assert!(s.samples.take_requests().is_empty());
        assert!(s.dial_unsupported(0).is_none());
        assert!(s.dial_unsupported(1).is_none());
        assert!(s.dial_unsupported(2).is_none());
    }

    #[test]
    fn sample_dials_queue_loads_for_the_focused_layer() {
        let mut s = SynthUi::new();
        s.samples.set_catalog(&[
            "Samp Lib/Bass/Fretless.nsmp4".into(),
            "Samp Lib/Bass/Slap.nsmp4".into(),
            "Samp Lib/Choir/Aahs.nsmp4".into(),
        ]);
        s.focus_layer(1);
        s.layer_mut().sample_mode = true;
        s.turn_dial(2, 1, false);
        s.turn_dial(2, 1, false);
        assert_eq!(
            s.samples.take_requests(),
            vec![(1, "Samp Lib/Bass/Slap.nsmp4".to_string())]
        );
        s.turn_dial(1, 1, false);
        assert_eq!(s.samples.category(1), Some("Choir"));
        assert_eq!(s.samples.take_requests()[0].0, 1);
        assert!(s.samples.entry(0).is_none(), "layer A untouched");
    }

    #[test]
    fn keep_edits_holds_with_and_without_the_waveform_button() {
        let mut s = SynthUi::new();
        s.samples.set_catalog(&["Samp Lib/Bass/Slap.nsmp4".into()]);
        s.layer_mut().sample_mode = true;
        s.layer_mut().amp_env.release = 2.5;
        s.turn_dial(2, 1, false);
        assert!(!s.keep_edits());
        s.tracker("synth.waveform").press(0);
        assert!(s.keep_edits());
        s.turn_dial(2, -1, false);
        assert_eq!(s.layer().amp_env.release, 2.5);
        assert!(s.layer().sample_mode);
    }

    #[test]
    fn filter_page_dials_set_type_kbd_track_and_drive() {
        let mut s = SynthUi::new();
        s.open(SynthPage::FilterType);
        s.turn_dial(0, 1, false);
        s.turn_dial(1, 2, false);
        s.turn_dial(2, 9, false);
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::FilterModel(FilterModel::Lp24)));
        assert!(p.contains(&SynthParam::KbTrack(2)));
        assert!(p.contains(&SynthParam::FilterDrive(1.0)));
        assert_eq!(s.layer().filter_type, 1);
    }

    #[test]
    fn lp_hp_type_is_kept_in_the_panel_but_sends_nothing() {
        let mut s = SynthUi::new();
        s.open(SynthPage::FilterType);
        s.turn_dial(0, 3, false);
        assert_eq!(s.layer().filter_type, 3);
        assert!(s.take_edits().is_empty());
    }

    #[test]
    fn envelope_pages_edit_their_own_envelope() {
        let mut s = SynthUi::new();
        s.open(SynthPage::EnvOsc);
        s.turn_dial(0, 5, false);
        assert!(s.layer().osc_env.attack > 0.005);
        assert!(params(&mut s)
            .iter()
            .any(|p| matches!(p, SynthParam::OscEnvAttack(_))));
        s.open(SynthPage::EnvFilter);
        s.turn_dial(1, -5, false);
        assert!(s.layer().filter_env.decay < 0.3);
        assert!(params(&mut s)
            .iter()
            .any(|p| matches!(p, SynthParam::FilterEnvDecay(_))));
        s.open(SynthPage::EnvAmp);
        s.turn_dial(2, 3, false);
        assert!(s.layer().amp_env.release > 0.25);
        assert!(params(&mut s)
            .iter()
            .any(|p| matches!(p, SynthParam::AmpRelease(_))));
        assert_eq!(s.layer().osc_env.release, 0.25, "other envelopes untouched");
    }

    #[test]
    fn lfo_waveform_dial_picks_the_shape() {
        let mut s = SynthUi::new();
        s.open(SynthPage::LfoWave);
        s.turn_dial(0, 1, false);
        assert_eq!(s.layer().lfo_shape, 1);
        assert_eq!(
            params(&mut s),
            vec![SynthParam::LfoShape(
                nord_core::synth_types::LfoShape::SawDown
            )]
        );
        s.turn_dial(1, 1, false);
        assert!(s.take_edits().is_empty(), "only the left dial is used");
    }

    #[test]
    fn extern_page_dials_send_cc_values_and_program() {
        let mut s = SynthUi::new();
        s.focus_layer(2);
        s.open(SynthPage::Extern);
        s.turn_dial(1, 10, false);
        s.turn_dial(2, 3, false);
        s.turn_dial(0, -4, false);
        assert_eq!(s.layer().extern_vals, [0, 10, 3]);
        assert_eq!(
            s.take_edits(),
            vec![
                SynthEdit::ExternCc {
                    layer: 2,
                    slot: 1,
                    value: 10
                },
                SynthEdit::ExternProgram {
                    layer: 2,
                    program: 3
                },
                SynthEdit::ExternCc {
                    layer: 2,
                    slot: 0,
                    value: 0
                },
            ]
        );
        assert!((0..3).all(|d| s.dial_unsupported(d).is_none()));
        assert_eq!(s.layers[0].extern_vals, [0; 3], "values are per layer");
    }

    #[test]
    fn zero_steps_and_bad_dials_are_ignored() {
        let mut s = SynthUi::new();
        s.turn_dial(0, 0, false);
        s.turn_dial(7, 3, false);
        assert!(s.take_edits().is_empty());
    }

    #[test]
    fn grouped_filter_dial_reaches_all_layers() {
        let mut s = SynthUi::new();
        s.group_filter.double_tap();
        s.open(SynthPage::FilterType);
        s.turn_dial(1, 1, false);
        assert!(s.layers.iter().all(|l| l.kb_track == 1));
        assert!(matches!(
            s.take_edits()[0],
            SynthEdit::Param(SynthTarget::All, SynthParam::KbTrack(1))
        ));
    }
}
