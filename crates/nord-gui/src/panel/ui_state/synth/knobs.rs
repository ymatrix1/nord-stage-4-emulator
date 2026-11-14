//! The ten Synth pots and their pick-up behaviour (`nord_panel::behaviour::PotTracker`).
//!
//! A pot is a physical object: one position, shared by the three layers. After the focus moves to
//! another layer the stored value differs from the pot position, so the pot ignores input until
//! it crosses or comes near the stored value ([`nord_panel::behaviour::SNAP_TOLERANCE`]).

use super::pots::{norm_of, FREQ_MAX, FREQ_MIN, GLIDE_MAX, LFO_MAX, LFO_MIN};
use super::{Scope, SynthUi};
use crate::audio::{PatternParam, SynthParam};

/// Pots of the Synth section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    OscCtrl,
    OscEnvAmt,
    FilterFreq,
    FilterRes,
    FilterEnvAmt,
    LfoRate,
    LfoModAmt,
    Glide,
    ArpRate,
    ArpRange,
}

impl Knob {
    pub const ALL: [Knob; 10] = [
        Knob::OscCtrl,
        Knob::OscEnvAmt,
        Knob::FilterFreq,
        Knob::FilterRes,
        Knob::FilterEnvAmt,
        Knob::LfoRate,
        Knob::LfoModAmt,
        Knob::Glide,
        Knob::ArpRate,
        Knob::ArpRange,
    ];

    /// Inventory id of the control.
    pub fn id(self) -> &'static str {
        match self {
            Self::OscCtrl => "synth.osc.ctrl",
            Self::OscEnvAmt => "synth.osc.env_amt",
            Self::FilterFreq => "synth.filter.freq",
            Self::FilterRes => "synth.filter.res",
            Self::FilterEnvAmt => "synth.filter.env_amt",
            Self::LfoRate => "synth.lfo.rate",
            Self::LfoModAmt => "synth.lfo.mod_amt",
            Self::Glide => "synth.voice.glide",
            Self::ArpRate => "synth.arp.rate",
            Self::ArpRange => "synth.arp.range",
        }
    }

    pub(super) fn idx(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

impl SynthUi {
    /// Pointer input on pot `k` at position `pos` (`0..=1`). The pot moves; the layer value changes
    /// only once the pot has picked up the stored value.
    pub fn knob_input(&mut self, k: Knob, pos: f32) {
        let pos = pos.clamp(0.0, 1.0);
        self.pots.phys[k.idx()] = pos;
        if let Some(n) = self.pots.tr[k.idx()].input(pos) {
            self.apply_knob(k, n);
        }
    }

    /// Shift + turn of a RATE/TIME knob: clockwise syncs to the Master Clock, counter-clockwise
    /// returns to the free rate (manual p.34 "MST CLK", p.36).
    pub fn rate_clock_sync(&mut self, k: Knob, clockwise: bool) {
        match k {
            Knob::LfoRate => {
                self.edit(Scope::Lfo, SynthParam::LfoClockSync(clockwise), |l| {
                    l.lfo_sync = clockwise
                });
                let div = self.layer().lfo_div;
                self.edit(Scope::Lfo, SynthParam::LfoDivision(div), |_| {});
            }
            Knob::ArpRate => {
                self.edit(Scope::Arp, SynthParam::ArpClockSync(clockwise), |l| {
                    l.arp_sync = clockwise
                });
                let div = self.layer().arp_div;
                self.edit(Scope::Arp, SynthParam::ArpDivision(div), |_| {});
            }
            _ => return,
        }
        let n = norm_of(self.layer(), k);
        self.pots.tr[k.idx()].stored_changed(n);
    }

    /// Write normalized pot value `n` into the focused layer and queue the engine edit.
    fn apply_knob(&mut self, k: Knob, n: f32) {
        match k {
            Knob::OscCtrl => {
                self.layer_mut().osc_ctrl = n;
                let fixed_pulse = self
                    .layer()
                    .analog_entry()
                    .is_some_and(|e| e.pulse.is_some());
                if !fixed_pulse {
                    self.edit(Scope::Layer, SynthParam::OscCtrl(n), |l| l.osc_ctrl = n);
                }
            }
            Knob::OscEnvAmt => {
                let v = n * 2.0 - 1.0;
                self.edit(Scope::Layer, SynthParam::OscEnvAmount(v), |l| {
                    l.osc_env_amt = v
                });
            }
            Knob::FilterFreq => {
                let hz = FREQ_MIN * (FREQ_MAX / FREQ_MIN).powf(n);
                self.edit(Scope::Filter, SynthParam::Cutoff(hz), |l| l.cutoff_hz = hz);
            }
            Knob::FilterRes => {
                self.edit(Scope::Filter, SynthParam::Resonance(n), |l| l.resonance = n);
            }
            Knob::FilterEnvAmt => {
                let v = n * 2.0 - 1.0;
                self.edit(Scope::Filter, SynthParam::FilterEnvAmount(v), |l| {
                    l.filter_env_amt = v
                });
            }
            Knob::LfoRate if self.layer().lfo_sync => {
                let d = (n * 9.0).round() as u8;
                self.edit(Scope::Lfo, SynthParam::LfoDivision(d), |l| l.lfo_div = d);
            }
            Knob::LfoRate => {
                let hz = LFO_MIN * (LFO_MAX / LFO_MIN).powf(n);
                self.edit(Scope::Lfo, SynthParam::LfoRate(hz), |l| l.lfo_rate_hz = hz);
            }
            Knob::LfoModAmt => {
                self.edit(Scope::Lfo, SynthParam::LfoAmount(n), |l| l.lfo_amt = n);
            }
            Knob::Glide => {
                let g = GLIDE_MAX * n * n;
                self.edit(Scope::Layer, SynthParam::Glide(g), |l| l.glide = g);
            }
            Knob::ArpRate if self.layer().arp_sync => {
                let d = (n * 9.0).round() as u8;
                self.edit(Scope::Arp, SynthParam::ArpDivision(d), |l| l.arp_div = d);
            }
            Knob::ArpRate => {
                let bpm = 20.0 + 280.0 * n;
                self.edit(Scope::Arp, SynthParam::ArpRate(bpm), |l| l.arp_rate = bpm);
            }
            Knob::ArpRange if self.layer().arp_kind == 2 => {
                self.edit(Scope::Arp, SynthParam::GateEnv(n), |l| l.gate_env = n);
            }
            Knob::ArpRange => {
                // 10 detents: 1 octave .. 4 octaves in steps of 1/3 octave (manual p.35).
                let d = (n * 9.0).round() as u8;
                let (oct, thirds) = (1 + d / 3, d % 3);
                self.edit(Scope::Arp, SynthParam::ArpRange(oct), |l| {
                    l.arp_range = 1.0 + f32::from(d) / 3.0
                });
                self.edit(
                    Scope::Arp,
                    SynthParam::Pattern(PatternParam::RangeThirds(thirds)),
                    |_| {},
                );
            }
        }
    }

    /// Put every pot at the focused layer's values and engage it.
    pub fn reset_pots(&mut self) {
        let l = self.layers[self.focus.min(2)].clone();
        self.pots.reset(&l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{SynthEdit, SynthTarget};
    use nord_panel::behaviour::PotTracker;

    fn sent(s: &mut SynthUi) -> Vec<SynthParam> {
        s.take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn knob_ids_are_unique_inventory_ids() {
        let ids: Vec<_> = Knob::ALL.iter().map(|k| k.id()).collect();
        for id in &ids {
            assert!(
                nord_panel::all_controls().iter().any(|c| c.id == *id),
                "{id}"
            );
        }
        let mut u = ids.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), ids.len());
    }

    #[test]
    fn norm_and_value_round_trip_for_continuous_pots() {
        let mut s = SynthUi::new();
        for (k, n) in [
            (Knob::FilterFreq, 0.8),
            (Knob::LfoRate, 0.3),
            (Knob::Glide, 0.6),
            (Knob::ArpRate, 0.25),
            (Knob::OscEnvAmt, 0.9),
            (Knob::FilterEnvAmt, 0.1),
        ] {
            s.pots.tr[k.idx()] = PotTracker::new(n);
            s.knob_input(k, n);
            let back = norm_of(s.layer(), k);
            assert!((back - n).abs() < 1e-4, "{k:?}: {back} vs {n}");
        }
    }

    #[test]
    fn each_pot_emits_its_engine_param() {
        let mut s = SynthUi::new();
        s.take_edits();
        for k in Knob::ALL {
            s.pots.tr[k.idx()] = PotTracker::new(0.7);
            s.knob_input(k, 0.7);
        }
        let p = sent(&mut s);
        assert!(p.iter().any(|x| matches!(x, SynthParam::OscCtrl(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::OscEnvAmount(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::Cutoff(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::Resonance(_))));
        assert!(p
            .iter()
            .any(|x| matches!(x, SynthParam::FilterEnvAmount(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::LfoRate(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::LfoAmount(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::Glide(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::ArpRate(_))));
        assert!(p.iter().any(|x| matches!(x, SynthParam::ArpRange(_))));
    }

    #[test]
    fn arp_range_moves_in_thirds_of_an_octave() {
        let mut s = SynthUi::new();
        for (pos, oct, thirds, range) in [
            (0.0, 1, 0, 1.0),
            (1.0 / 9.0, 1, 1, 1.0 + 1.0 / 3.0),
            (2.0 / 9.0, 1, 2, 1.0 + 2.0 / 3.0),
            (4.0 / 9.0, 2, 1, 2.0 + 1.0 / 3.0),
            (1.0, 4, 0, 4.0),
        ] {
            s.pots.tr[Knob::ArpRange.idx()] = PotTracker::new(pos);
            s.take_edits();
            s.knob_input(Knob::ArpRange, pos);
            let p = sent(&mut s);
            assert!(p.contains(&SynthParam::ArpRange(oct)), "{pos}");
            assert!(
                p.contains(&SynthParam::Pattern(PatternParam::RangeThirds(thirds))),
                "{pos}"
            );
            assert!((s.layer().arp_range - range).abs() < 1e-5);
            assert!((s.layer().arp_range - s.layer().arp_range.round()).abs() < 0.5);
        }
    }

    #[test]
    fn cutoff_pot_spans_the_log_range() {
        let mut s = SynthUi::new();
        s.pots.tr[Knob::FilterFreq.idx()] = PotTracker::new(1.0);
        s.knob_input(Knob::FilterFreq, 1.0);
        assert!((s.layer().cutoff_hz - 15_000.0).abs() < 1.0);
        s.pots.tr[Knob::FilterFreq.idx()] = PotTracker::new(0.0);
        s.knob_input(Knob::FilterFreq, 0.0);
        assert!((s.layer().cutoff_hz - 40.0).abs() < 1e-3);
    }

    #[test]
    fn pot_ignores_input_until_picked_up_after_a_focus_change() {
        let mut s = SynthUi::new();
        s.layers[1].resonance = 0.9;
        s.focus_layer(1);
        s.take_edits();
        // The physical pot is at 0.3 (layer A's value), layer B stores 0.9.
        s.knob_input(Knob::FilterRes, 0.35);
        assert_eq!(s.layers[1].resonance, 0.9, "not picked up yet");
        assert!(s.take_edits().is_empty());
        assert_eq!(s.pots.phys(Knob::FilterRes), 0.35);
        s.knob_input(Knob::FilterRes, 0.89);
        assert!(s.pots.is_engaged(Knob::FilterRes));
        assert!((s.layers[1].resonance - 0.89).abs() < 1e-6);
        assert_eq!(s.layers[0].resonance, 0.3);
    }

    #[test]
    fn crossing_the_stored_value_picks_the_pot_up() {
        let mut s = SynthUi::new();
        s.layers[1].resonance = 0.5;
        s.focus_layer(1);
        s.knob_input(Knob::FilterRes, 0.1);
        s.knob_input(Knob::FilterRes, 0.8);
        assert!(s.pots.is_engaged(Knob::FilterRes));
        assert!((s.layers[1].resonance - 0.8).abs() < 1e-6);
    }

    #[test]
    fn reset_pots_engages_every_pot_at_the_stored_values() {
        let mut s = SynthUi::new();
        s.layers[2].cutoff_hz = 500.0;
        s.focus_layer(2);
        s.reset_pots();
        assert!(Knob::ALL.iter().all(|k| s.pots.is_engaged(*k)));
        assert!((s.pots.phys(Knob::FilterFreq) - s.pots.stored(Knob::FilterFreq)).abs() < 1e-6);
    }

    #[test]
    fn shift_turn_syncs_the_lfo_to_the_master_clock() {
        let mut s = SynthUi::new();
        s.take_edits();
        s.rate_clock_sync(Knob::LfoRate, true);
        assert!(s.layer().lfo_sync);
        let p = sent(&mut s);
        assert!(p.contains(&SynthParam::LfoClockSync(true)));
        s.pots.tr[Knob::LfoRate.idx()] = PotTracker::new(1.0);
        s.knob_input(Knob::LfoRate, 1.0);
        assert_eq!(s.layer().lfo_div, 9, "the knob picks the subdivision");
        s.rate_clock_sync(Knob::LfoRate, false);
        assert!(!s.layer().lfo_sync);
    }

    #[test]
    fn shift_turn_syncs_the_arp_and_ignores_other_pots() {
        let mut s = SynthUi::new();
        s.rate_clock_sync(Knob::ArpRate, true);
        assert!(s.layer().arp_sync);
        let before = s.layer().clone();
        s.rate_clock_sync(Knob::FilterFreq, true);
        assert_eq!(*s.layer(), before);
    }

    #[test]
    fn arp_range_pot_controls_the_gate_envelope_in_gate_mode() {
        let mut s = SynthUi::new();
        s.layer_mut().arp_kind = 2;
        s.pots.tr[Knob::ArpRange.idx()] = PotTracker::new(0.75);
        s.take_edits();
        s.knob_input(Knob::ArpRange, 0.75);
        assert_eq!(s.layer().gate_env, 0.75);
        assert_eq!(sent(&mut s), vec![SynthParam::GateEnv(0.75)]);
    }

    #[test]
    fn grouped_filter_pot_reaches_every_layer() {
        let mut s = SynthUi::new();
        s.group_filter.double_tap();
        s.take_edits();
        s.pots.tr[Knob::FilterRes.idx()] = PotTracker::new(0.6);
        s.knob_input(Knob::FilterRes, 0.6);
        assert!(s.layers.iter().all(|l| (l.resonance - 0.6).abs() < 1e-6));
        let e = s.take_edits();
        assert!(matches!(e[0], SynthEdit::Param(SynthTarget::All, _)));
    }

    #[test]
    fn osc_ctrl_is_not_sent_for_a_fixed_pulse_waveform() {
        let mut s = SynthUi::new();
        s.layer_mut().wave_idx = 4; // Pulse 33
        s.take_edits();
        s.pots.tr[Knob::OscCtrl.idx()] = PotTracker::new(0.9);
        s.knob_input(Knob::OscCtrl, 0.9);
        assert_eq!(s.layer().osc_ctrl, 0.9);
        assert!(s.take_edits().is_empty());
    }
}
