//! SECTION EDIT (manual p.43) for the controls that do not go through [`SynthUi::edit`]: the
//! oscillator selection (WAVEFORM dials) and the KB ZONE buttons. Every other control reaches all
//! three layers through `edit` itself (see `SynthUi::grouped`).

use super::SynthUi;
use crate::audio::{SynthEdit, SynthTarget};
use crate::panel::kb_zone;
use crate::panel::section_edit::layers_for;
use std::ops::Range;

impl SynthUi {
    /// Layers an edit of the focused layer writes (all three while SECTION EDIT is active).
    pub(crate) fn edit_layers(&self) -> Range<usize> {
        layers_for(self.focus, 3, self.section_edit)
    }

    /// Queue the oscillator writes of the focused layer's selection (mode, waveform, OSC CTRL ...).
    /// With SECTION EDIT the other layers take the same waveform selection first and get the same
    /// writes; each layer keeps its own Sample / Synth mode.
    pub(crate) fn resend_osc(&mut self) {
        let src = self.layer().clone();
        for i in self.edit_layers() {
            if i != self.focus {
                let l = &mut self.layers[i];
                (l.wave_type, l.wave_cat, l.wave_idx) = (src.wave_type, src.wave_cat, src.wave_idx);
                (l.fm_algo, l.osc_ctrl) = (src.fm_algo, src.osc_ctrl);
            }
            let mut v = Vec::new();
            self.layers[i].emit_osc(&mut |p| v.push(p));
            let target = SynthTarget::Layer(i as u8);
            self.out
                .extend(v.into_iter().map(|p| SynthEdit::Param(target, p)));
        }
    }

    /// KB ZONE step of the focused layer; with SECTION EDIT all Synth layers move to the same zone
    /// (the manual's "set the KB Zone for all Synth Layers at once", p.37).
    pub(crate) fn step_zone(&mut self, forward: bool) {
        let z = kb_zone::step(self.layer().kb_zone, forward);
        for i in self.edit_layers() {
            self.layers[i].kb_zone = z;
            self.out.push(SynthEdit::Zone {
                layer: i as u8,
                leds: kb_zone::leds(z),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SynthParam;

    #[test]
    fn one_layer_without_section_edit() {
        let mut s = SynthUi::new();
        s.focus_layer(1);
        s.step_zone(true);
        assert_eq!(s.take_edits().len(), 1);
        assert_eq!(s.layers[0].kb_zone, 0);
        s.resend_osc();
        assert!(s
            .take_edits()
            .iter()
            .all(|e| matches!(e, SynthEdit::Param(SynthTarget::Layer(1), _))));
    }

    #[test]
    fn section_edit_moves_every_layer_to_the_zone_and_waveform() {
        let mut s = SynthUi::new();
        s.section_edit = true;
        s.focus_layer(1);
        s.layers[1].wave_idx = 3;
        s.step_zone(true);
        let zones: Vec<_> = s
            .take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Zone { layer, leds } => Some((layer, leds)),
                _ => None,
            })
            .collect();
        assert_eq!(zones.len(), 3);
        assert!(zones.iter().all(|z| z.1 == zones[0].1));
        assert!(s.layers.iter().all(|l| l.kb_zone == s.layers[1].kb_zone));
        s.resend_osc();
        let e = s.take_edits();
        assert!(s.layers.iter().all(|l| l.wave_idx == 3));
        for want in 0..3u8 {
            assert!(e.iter().any(|x| matches!(
                x,
                SynthEdit::Param(SynthTarget::Layer(l), SynthParam::OscMode(_)) if *l == want
            )));
        }
    }
}
