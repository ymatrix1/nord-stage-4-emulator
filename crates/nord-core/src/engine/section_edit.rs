//! SECTION EDIT (manual p.43): an edit made while Section Edit is held applies to every layer
//! of the section. The panel layer edits one layer, then calls these helpers with accessors so
//! the same value reaches the siblings.

use super::*;

/// Organ, Piano or Synth section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Organ,
    Piano,
    Synth,
}

impl Section {
    /// Section a layer belongs to.
    pub fn of(layer: LayerId) -> Self {
        match layer {
            LayerId::OrganA | LayerId::OrganB => Self::Organ,
            LayerId::PianoA | LayerId::PianoB => Self::Piano,
            _ => Self::Synth,
        }
    }

    /// Layers of the section.
    pub fn layers(self) -> &'static [LayerId] {
        match self {
            Self::Organ => &[LayerId::OrganA, LayerId::OrganB],
            Self::Piano => &[LayerId::PianoA, LayerId::PianoB],
            Self::Synth => &[LayerId::SynthA, LayerId::SynthB, LayerId::SynthC],
        }
    }
}

impl Stage4Engine {
    /// Copies one Organ setting of `src` to both Organ layers (no-op if `src` is not an Organ layer).
    pub fn section_edit_organ<T>(
        &mut self,
        src: LayerId,
        get: impl Fn(&OrganLayer) -> T,
        set: impl Fn(&mut OrganLayer, T) + Copy,
    ) where
        T: Copy,
    {
        let v = match src {
            LayerId::OrganA => get(&self.organ_a),
            LayerId::OrganB => get(&self.organ_b),
            _ => return,
        };
        set(&mut self.organ_a, v);
        set(&mut self.organ_b, v);
    }

    /// Copies one Piano setting of `src` to both Piano layers (no-op if `src` is not a Piano layer).
    pub fn section_edit_piano<T>(
        &mut self,
        src: LayerId,
        get: impl Fn(&PianoLayer) -> T,
        set: impl Fn(&mut PianoLayer, T) + Copy,
    ) where
        T: Copy,
    {
        let v = match src {
            LayerId::PianoA => get(&self.piano_a),
            LayerId::PianoB => get(&self.piano_b),
            _ => return,
        };
        set(&mut self.piano_a, v);
        set(&mut self.piano_b, v);
    }

    /// Copies one Synth setting of `src` to all three Synth layers (no-op if `src` is not a Synth layer).
    pub fn section_edit_synth<T>(
        &mut self,
        src: LayerId,
        get: impl Fn(&SynthLayer) -> T,
        set: impl Fn(&mut SynthLayer, T) + Copy,
    ) where
        T: Copy,
    {
        let v = match src {
            LayerId::SynthA => get(&self.synth_a),
            LayerId::SynthB => get(&self.synth_b),
            LayerId::SynthC => get(&self.synth_c),
            _ => return,
        };
        set(&mut self.synth_a, v);
        set(&mut self.synth_b, v);
        set(&mut self.synth_c, v);
    }

    /// Copies one FX setting of `src`'s chain to the chains of its section ("setting up some of
    /// the effects the same for a whole Section").
    pub fn section_edit_fx<T>(
        &mut self,
        src: LayerId,
        get: impl Fn(&LayerFxChain) -> T,
        set: impl Fn(&mut LayerFxChain, T) + Copy,
    ) where
        T: Copy,
    {
        let v = get(&self.fx_chains[src as usize]);
        for layer in Section::of(src).layers() {
            set(&mut self.fx_chains[*layer as usize], v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_partition_the_seven_layers() {
        let total: usize = [Section::Organ, Section::Piano, Section::Synth]
            .iter()
            .map(|s| s.layers().len())
            .sum();
        assert_eq!(total, 7);
        assert_eq!(Section::of(LayerId::SynthC), Section::Synth);
        assert_eq!(Section::of(LayerId::PianoB), Section::Piano);
    }

    #[test]
    fn synth_edit_reaches_all_synth_layers_only() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_b.cutoff_hz = 1234.0;
        e.section_edit_synth(LayerId::SynthB, |l| l.cutoff_hz, |l, v| l.cutoff_hz = v);
        assert_eq!(e.synth_a.cutoff_hz, 1234.0);
        assert_eq!(e.synth_c.cutoff_hz, 1234.0);
        // Wrong section source: nothing happens.
        e.section_edit_synth(LayerId::OrganA, |l| l.cutoff_hz, |l, _| l.cutoff_hz = 1.0);
        assert_eq!(e.synth_a.cutoff_hz, 1234.0);
    }

    #[test]
    fn organ_and_fx_edits_stay_in_their_section() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_b.drawbars[2] = 7;
        e.section_edit_organ(LayerId::OrganB, |l| l.drawbars[2], |l, v| l.drawbars[2] = v);
        assert_eq!(e.organ_a.drawbars[2], 7);
        e.fx_chains[5].reverb_mix = 0.6;
        e.section_edit_fx(LayerId::SynthB, |c| c.reverb_mix, |c, v| c.reverb_mix = v);
        assert_eq!(e.fx_chains[4].reverb_mix, 0.6);
        assert_eq!(e.fx_chains[6].reverb_mix, 0.6);
        assert_ne!(e.fx_chains[2].reverb_mix, 0.6);
    }

    #[test]
    fn piano_edit_copies_between_piano_layers() {
        let mut e = Stage4Engine::new(48_000.0);
        e.piano_a.unison = 2;
        e.section_edit_piano(LayerId::PianoA, |l| l.unison, |l, v| l.unison = v);
        assert_eq!(e.piano_b.unison, 2);
    }
}
