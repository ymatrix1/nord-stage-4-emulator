//! GROUP and Sound Init (manual p.33, p.35-37).

use super::layer::SynthLayerUi;
use super::SynthUi;
use crate::audio::{SynthEdit, SynthGroup, SynthParam, SynthTarget};

impl SynthUi {
    /// GROUP (Shift+Type, Shift+Waveform, Shift+Menu): on = copy this layer's settings to all
    /// layers and keep further edits in sync; off = layers are independent again.
    pub fn toggle_group(&mut self, g: SynthGroup) {
        let latch = match g {
            SynthGroup::Filter => &mut self.group_filter,
            SynthGroup::Lfo => &mut self.group_lfo,
            SynthGroup::Arp => &mut self.group_arp,
        };
        latch.double_tap();
        if latch.is_active() {
            let src = self.layer().clone();
            for (i, l) in self.layers.iter_mut().enumerate() {
                if i != self.focus {
                    l.copy_group_from(&src, g);
                }
            }
            self.push_group_copy(g);
        }
    }

    /// Sound Init (Shift+Waveform, manual p.37): every parameter except the Synth mode and the
    /// selected waveform returns to its default. The engine does it in place with
    /// `SynthLayer::sound_init` (no allocation, no undo snapshot); the panel then resends what the
    /// engine does not keep itself: the selected waveform's oscillator values and the section-wide
    /// KB HOLD.
    pub fn sound_init(&mut self) {
        let keep = self.layer().clone();
        let fresh = SynthLayerUi {
            sample_mode: keep.sample_mode,
            wave_type: keep.wave_type,
            wave_cat: keep.wave_cat,
            wave_idx: keep.wave_idx,
            fm_algo: keep.fm_algo,
            ..SynthLayerUi::default()
        };
        let layer = self.focus.min(2) as u8;
        self.out.push(SynthEdit::SoundInit(layer));
        *self.layer_mut() = fresh;
        self.resend_osc();
        let hold = self.kb_hold;
        self.out.push(SynthEdit::Param(
            SynthTarget::Layer(layer),
            SynthParam::KbHold(hold),
        ));
        self.reset_pots();
    }
}

#[cfg(test)]
mod tests {
    use super::super::buttons::Btn;
    use super::*;
    use nord_panel::behaviour::Trigger;

    fn shifted(s: &mut SynthUi, b: Btn) {
        s.press(b, Trigger::Shifted);
    }

    #[test]
    fn group_on_copies_to_all_layers_and_keeps_edits_in_sync_until_off() {
        let mut s = SynthUi::new();
        s.layer_mut().cutoff_hz = 800.0;
        shifted(&mut s, Btn::FilterType);
        assert!(s.layers.iter().all(|l| l.cutoff_hz == 800.0));
        assert!(matches!(
            s.take_edits()[0],
            SynthEdit::Group {
                src: 0,
                group: SynthGroup::Filter
            }
        ));
        shifted(&mut s, Btn::FilterType);
        assert!(!s.group_filter.is_active());
        assert!(s.take_edits().is_empty(), "turning GROUP off sends nothing");
        shifted(&mut s, Btn::LfoWaveform);
        shifted(&mut s, Btn::ArpMenu);
        assert!(s.group_lfo.is_active() && s.group_arp.is_active());
        assert_eq!(s.take_edits().len(), 2);
    }

    #[test]
    fn sound_init_resets_everything_but_mode_and_waveform() {
        let mut s = SynthUi::new();
        s.layer_mut().wave_cat = 5;
        s.layer_mut().wave_idx = 1;
        s.layer_mut().cutoff_hz = 123.0;
        s.layer_mut().unison = 3;
        s.layer_mut().amp_env.attack = 5.0;
        shifted(&mut s, Btn::Waveform);
        let l = s.layer();
        assert_eq!((l.wave_cat, l.wave_idx), (5, 1));
        assert_eq!(l.cutoff_hz, 2400.0);
        assert_eq!(l.unison, 0);
        assert_eq!(l.amp_env.attack, 0.005);
        let all = s.take_edits();
        assert_eq!(all[0], SynthEdit::SoundInit(0), "the engine resets first");
        let p: Vec<_> = all
            .iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(*p),
                _ => None,
            })
            .collect();
        assert!(p.contains(&SynthParam::Waveform(
            nord_core::synth::VaWaveform::SuperSaw
        )));
        assert!(
            !p.iter().any(|x| matches!(x, SynthParam::Cutoff(_))),
            "plain values are not resent: the engine reset them"
        );
        assert!(s.pots.is_engaged(super::super::Knob::FilterFreq));
    }

    #[test]
    fn sound_init_applied_to_an_engine_matches_the_panel_state() {
        use nord_core::synth::SynthLayer;
        let mut s = SynthUi::new();
        s.focus_layer(1);
        s.layer_mut().cutoff_hz = 123.0;
        s.layer_mut().zigzag = true;
        s.kb_hold = true;
        let mut e = nord_core::engine::Stage4Engine::new(48_000.0);
        e.synth_b.cutoff_hz = 123.0;
        e.synth_b.perf.zigzag = true;
        shifted(&mut s, Btn::Waveform);
        for edit in s.take_edits() {
            edit.apply(&mut e);
        }
        let fresh = SynthLayer::new(48_000.0);
        assert_eq!(e.synth_b.cutoff_hz, fresh.cutoff_hz);
        assert!(!e.synth_b.perf.zigzag);
        assert!(e.synth_b.perf.kb_hold, "section-wide KB HOLD is kept");
        assert_eq!(s.layer().cutoff_hz, 2400.0);
        assert!(!s.layer().zigzag);
    }
}
