//! MON/COPY, PASTE and SWAP (manual p.43-44): copy a Layer, an Effect chain, a Morph or a whole
//! Program, paste it elsewhere, or swap two Layers of the same program.
//!
//! The clipboard lives in `Stage4Engine::clipboard` and survives program changes, so a copy can
//! be pasted into another program. Everything here allocates: control thread only.

use super::snapshot::released;
use super::*;

/// Why a paste or swap was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardError {
    /// Nothing was copied.
    Empty,
    /// The clipboard holds another kind of item (or a layer of another section).
    Mismatch,
}

#[derive(Debug, Clone)]
enum LayerPanel {
    Organ(Box<OrganLayer>),
    Piano(Box<PianoLayer>),
    Synth(Box<SynthLayer>),
}

/// A copied layer: its panel state, its FX chain parameters and its level.
#[derive(Debug, Clone)]
pub struct LayerClip {
    panel: LayerPanel,
    fx: FxParams,
    level: MorphParameter,
    cutoff: Option<MorphParameter>,
}

impl LayerClip {
    /// Section the copied layer came from; a layer can only be pasted into the same section.
    pub fn section(&self) -> Section {
        match self.panel {
            LayerPanel::Organ(_) => Section::Organ,
            LayerPanel::Piano(_) => Section::Piano,
            LayerPanel::Synth(_) => Section::Synth,
        }
    }
}

/// Contents of the copy buffer.
#[derive(Debug, Clone)]
pub enum Clip {
    Layer(LayerClip),
    Fx(FxParams),
    Morph(MorphClip),
    Program(Box<ProgramState>),
}

impl Stage4Engine {
    /// Reads layer `id` with notes released (control thread).
    pub fn capture_layer(&self, id: LayerId) -> LayerClip {
        let panel = match id {
            LayerId::OrganA => {
                LayerPanel::Organ(Box::new(released(&self.organ_a, |l, n| l.note_off(n))))
            }
            LayerId::OrganB => {
                LayerPanel::Organ(Box::new(released(&self.organ_b, |l, n| l.note_off(n))))
            }
            LayerId::PianoA => {
                LayerPanel::Piano(Box::new(released(&self.piano_a, |l, n| l.note_off(n))))
            }
            LayerId::PianoB => {
                LayerPanel::Piano(Box::new(released(&self.piano_b, |l, n| l.note_off(n))))
            }
            LayerId::SynthA => {
                LayerPanel::Synth(Box::new(released(&self.synth_a, |l, n| l.note_off(n))))
            }
            LayerId::SynthB => {
                LayerPanel::Synth(Box::new(released(&self.synth_b, |l, n| l.note_off(n))))
            }
            LayerId::SynthC => {
                LayerPanel::Synth(Box::new(released(&self.synth_c, |l, n| l.note_off(n))))
            }
        };
        let synth_idx = (id as usize).checked_sub(LayerId::SynthA as usize);
        LayerClip {
            panel,
            fx: FxParams::capture(&self.fx_chains[id as usize]),
            level: self.layer_levels[id as usize],
            cutoff: synth_idx.map(|i| self.synth_cutoff_morph[i]),
        }
    }

    fn write_layer(&mut self, id: LayerId, clip: &LayerClip) -> Result<(), ClipboardError> {
        if clip.section() != Section::of(id) {
            return Err(ClipboardError::Mismatch);
        }
        match (&clip.panel, id) {
            (LayerPanel::Organ(l), LayerId::OrganA) => self.organ_a = (**l).clone(),
            (LayerPanel::Organ(l), LayerId::OrganB) => self.organ_b = (**l).clone(),
            (LayerPanel::Piano(l), LayerId::PianoA) => self.piano_a = (**l).clone(),
            (LayerPanel::Piano(l), LayerId::PianoB) => self.piano_b = (**l).clone(),
            (LayerPanel::Synth(l), LayerId::SynthA) => self.synth_a = (**l).clone(),
            (LayerPanel::Synth(l), LayerId::SynthB) => self.synth_b = (**l).clone(),
            (LayerPanel::Synth(l), LayerId::SynthC) => self.synth_c = (**l).clone(),
            _ => return Err(ClipboardError::Mismatch),
        }
        clip.fx.apply(&mut self.fx_chains[id as usize]);
        self.layer_levels[id as usize] = clip.level;
        if let (Some(i), Some(c)) = (
            (id as usize).checked_sub(LayerId::SynthA as usize),
            clip.cutoff,
        ) {
            self.synth_cutoff_morph[i] = c;
        }
        Ok(())
    }

    /// COPY a layer.
    pub fn copy_layer(&mut self, id: LayerId) {
        self.clipboard = Some(Clip::Layer(self.capture_layer(id)));
    }

    /// COPY an effect chain (the FX of layer `id`).
    pub fn copy_fx(&mut self, id: LayerId) {
        self.clipboard = Some(Clip::Fx(FxParams::capture(&self.fx_chains[id as usize])));
    }

    /// COPY a morph source.
    pub fn copy_morph(&mut self, source: MorphSource) {
        self.clipboard = Some(Clip::Morph(self.capture_morph(source)));
    }

    /// COPY the current program.
    pub fn copy_program(&mut self) {
        self.clipboard = Some(Clip::Program(self.capture_state()));
    }

    /// PASTE a copied layer into `id` (same section only). Undoable.
    pub fn paste_layer(&mut self, id: LayerId) -> Result<(), ClipboardError> {
        let Some(Clip::Layer(clip)) = self.clipboard.clone() else {
            return Err(self.paste_error());
        };
        if clip.section() != Section::of(id) {
            return Err(ClipboardError::Mismatch);
        }
        self.begin_undoable(UndoOp::CopyPaste);
        self.write_layer(id, &clip)
    }

    /// PASTE a copied effect chain into the FX of layer `id`. Undoable.
    pub fn paste_fx(&mut self, id: LayerId) -> Result<(), ClipboardError> {
        let Some(Clip::Fx(params)) = self.clipboard.clone() else {
            return Err(self.paste_error());
        };
        self.begin_undoable(UndoOp::CopyPaste);
        params.apply(&mut self.fx_chains[id as usize]);
        Ok(())
    }

    /// PASTE a copied morph into `target`. Undoable.
    pub fn paste_morph(&mut self, target: MorphSource) -> Result<(), ClipboardError> {
        let Some(Clip::Morph(clip)) = self.clipboard.clone() else {
            return Err(self.paste_error());
        };
        self.apply_morph_clip(&clip, target);
        Ok(())
    }

    /// PASTE a copied program over the current one. Undoable.
    pub fn paste_program(&mut self) -> Result<(), ClipboardError> {
        let Some(Clip::Program(state)) = self.clipboard.clone() else {
            return Err(self.paste_error());
        };
        self.begin_undoable(UndoOp::CopyPaste);
        self.apply_state(state);
        Ok(())
    }

    /// SWAP two layers of the same section (layer, FX parameters, level). Undoable.
    pub fn swap_layers(&mut self, a: LayerId, b: LayerId) -> Result<(), ClipboardError> {
        if Section::of(a) != Section::of(b) {
            return Err(ClipboardError::Mismatch);
        }
        let (ca, cb) = (self.capture_layer(a), self.capture_layer(b));
        self.begin_undoable(UndoOp::CopyPaste);
        self.write_layer(a, &cb)?;
        self.write_layer(b, &ca)
    }

    fn paste_error(&self) -> ClipboardError {
        if self.clipboard.is_none() {
            ClipboardError::Empty
        } else {
            ClipboardError::Mismatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_copy_paste_within_section_and_undo() {
        let mut e = Stage4Engine::new(48_000.0);
        assert_eq!(e.paste_layer(LayerId::SynthB), Err(ClipboardError::Empty));
        e.synth_a.cutoff_hz = 4321.0;
        e.fx_chains[4].delay_mix = 0.8;
        e.layer_levels[4].base = 0.42;
        e.synth_cutoff_morph[0].delta_wheel = 0.3;
        e.copy_layer(LayerId::SynthA);
        e.paste_layer(LayerId::SynthC).unwrap();
        assert_eq!(e.synth_c.cutoff_hz, 4321.0);
        assert_eq!(e.fx_chains[6].delay_mix, 0.8);
        assert_eq!(e.layer_levels[6].base, 0.42);
        assert_eq!(e.synth_cutoff_morph[2].delta_wheel, 0.3);
        assert_eq!(e.undo(), Some(UndoOp::CopyPaste));
        assert_ne!(e.synth_c.cutoff_hz, 4321.0);
    }

    #[test]
    fn layer_paste_across_sections_is_refused() {
        let mut e = Stage4Engine::new(48_000.0);
        e.copy_layer(LayerId::OrganA);
        assert_eq!(
            e.paste_layer(LayerId::PianoA),
            Err(ClipboardError::Mismatch)
        );
        assert_eq!(e.paste_fx(LayerId::PianoA), Err(ClipboardError::Mismatch));
        assert_eq!(e.undo_available(), None);
    }

    #[test]
    fn swap_exchanges_two_layers_and_refuses_cross_section() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_a.cutoff_hz = 1000.0;
        e.synth_b.cutoff_hz = 2000.0;
        e.layer_levels[4].base = 0.1;
        e.layer_levels[5].base = 0.9;
        e.swap_layers(LayerId::SynthA, LayerId::SynthB).unwrap();
        assert_eq!(e.synth_a.cutoff_hz, 2000.0);
        assert_eq!(e.synth_b.cutoff_hz, 1000.0);
        assert_eq!((e.layer_levels[4].base, e.layer_levels[5].base), (0.9, 0.1));
        assert_eq!(
            e.swap_layers(LayerId::SynthA, LayerId::PianoA),
            Err(ClipboardError::Mismatch)
        );
    }

    #[test]
    fn fx_morph_and_program_copy_paste() {
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[0].reverb_mix = 0.95;
        e.copy_fx(LayerId::OrganA);
        e.paste_fx(LayerId::SynthC).unwrap();
        assert_eq!(e.fx_chains[6].reverb_mix, 0.95);

        e.layer_levels[1].delta_pedal = 0.5;
        e.copy_morph(MorphSource::ControlPedal);
        e.paste_morph(MorphSource::ModWheel).unwrap();
        assert_eq!(e.layer_levels[1].delta_wheel, 0.5);
        assert_eq!(e.paste_fx(LayerId::OrganA), Err(ClipboardError::Mismatch));

        e.set_transpose(4);
        e.copy_program();
        e.set_transpose(-1);
        e.paste_program().unwrap();
        assert_eq!(e.transpose(), 4);
    }
}
