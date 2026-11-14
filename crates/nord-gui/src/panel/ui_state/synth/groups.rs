//! GROUP and Sound Init (manual p.33, p.35-37).

use super::layer::SynthLayerUi;
use super::SynthUi;
use crate::audio::{SynthEdit, SynthGroup, SynthTarget};

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
    /// selected waveform returns to its default. Resends the layer's whole state, so the engine
    /// layer needs no reallocation (`engine.layer_init` allocates and is control-thread only).
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
        let mut v = Vec::new();
        fresh.emit_all(&mut |p| v.push(p));
        self.out.extend(
            v.into_iter()
                .map(|p| SynthEdit::Param(SynthTarget::Layer(layer), p)),
        );
        *self.layer_mut() = fresh;
        self.reset_pots();
    }
}

#[cfg(test)]
mod tests {
    use super::super::buttons::Btn;
    use super::*;
    use crate::audio::SynthParam;
    use nord_panel::behaviour::Trigger;

    fn params(s: &mut SynthUi) -> Vec<SynthParam> {
        s.take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }

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
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::Cutoff(2400.0)));
        assert!(p.contains(&SynthParam::Unison(0)));
        assert!(p.contains(&SynthParam::Waveform(
            nord_core::synth::VaWaveform::SuperSaw
        )));
        assert!(s.pots.is_engaged(super::super::Knob::FilterFreq));
    }
}
