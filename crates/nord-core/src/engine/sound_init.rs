//! SOUND INIT (Shift+Waveform, manual p.37): initialise one Synth layer's parameters, keeping
//! the Synth Mode and the selected waveform / sample. Undoable (`UndoOp::SynthInit`).
//!
//! Allocates (an Undo snapshot): control thread only.

use super::*;

impl Stage4Engine {
    /// Runs Sound Init on a synth layer (`0` = A, `1` = B, `2` = C; others ignored). Also
    /// resets that layer's cutoff morph. The state before it is kept for [`Stage4Engine::undo`].
    pub fn sound_init(&mut self, synth: usize) {
        if synth >= 3 {
            return;
        }
        self.begin_undoable(UndoOp::SynthInit);
        match synth {
            0 => self.synth_a.sound_init(),
            1 => self.synth_b.sound_init(),
            _ => self.synth_c.sound_init(),
        }
        self.synth_cutoff_morph[synth] = MorphParameter::new(0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth_types::{SynthOscMode, VaWaveform};

    #[test]
    fn sound_init_resets_one_layer_keeps_waveform_and_undoes() {
        let mut e = Stage4Engine::new(48_000.0);
        e.synth_b.cutoff_hz = 777.0;
        e.synth_b.osc_mode = SynthOscMode::Wavetable;
        e.synth_b.waveform = VaWaveform::Pulse;
        e.synth_c.cutoff_hz = 888.0;
        e.sound_init(1);
        assert_eq!(e.synth_b.cutoff_hz, SynthLayer::new(48_000.0).cutoff_hz);
        assert_eq!(e.synth_b.osc_mode, SynthOscMode::Wavetable);
        assert_eq!(e.synth_b.waveform, VaWaveform::Pulse);
        assert_eq!(e.synth_c.cutoff_hz, 888.0);
        assert_eq!(e.undo_available(), Some(UndoOp::SynthInit));
        e.undo();
        assert_eq!(e.synth_b.cutoff_hz, 777.0);
    }

    #[test]
    fn out_of_range_layer_is_ignored() {
        let mut e = Stage4Engine::new(48_000.0);
        e.sound_init(3);
        assert_eq!(e.undo_available(), None);
    }
}
