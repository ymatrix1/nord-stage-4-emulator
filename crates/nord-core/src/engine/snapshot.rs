//! `ProgramState`: the panel state of a whole program, used by Undo, Copy/Paste of a program
//! and the Live Mode slots (manual p.43-44).
//!
//! Capturing clones the layers, so it allocates: call it from the control thread only. The
//! captured layers have all notes released, so restoring a state never revives a sounding note.
//! FX chains are stored as [`FxParams`] (no delay or reverb buffers), which keeps a state small.

use super::*;

/// Panel state of one program. Not part of it: Master bus, MIDI channels, sample rate and
/// the notes currently held.
#[derive(Debug, Clone)]
pub struct ProgramState {
    pub(super) organ_a: OrganLayer,
    pub(super) organ_b: OrganLayer,
    pub(super) piano_a: PianoLayer,
    pub(super) piano_b: PianoLayer,
    pub(super) synth_a: SynthLayer,
    pub(super) synth_b: SynthLayer,
    pub(super) synth_c: SynthLayer,
    pub(super) fx: [FxParams; 7],
    pub(super) layer_levels: [MorphParameter; 7],
    pub(super) synth_cutoff_morph: [MorphParameter; 3],
    pub(super) scene: LayerSceneState,
    pub(super) split_router: KeybedSplitRouter,
    pub(super) transpose: i8,
    pub(super) program_level_db: f32,
    pub(super) tempo_bpm: f32,
    pub(super) compressor_amount: f32,
    pub(super) fx_sharing: FxSharing,
}

/// Clones `layer` with every note released (all 128 note-offs on the copy).
pub(super) fn released<L: Clone>(layer: &L, off: impl Fn(&mut L, u8)) -> L {
    let mut copy = layer.clone();
    for note in 0..128u8 {
        off(&mut copy, note);
    }
    copy
}

impl Stage4Engine {
    /// Captures the panel state of the current program (allocates; control thread only).
    pub fn capture_state(&self) -> Box<ProgramState> {
        Box::new(ProgramState {
            organ_a: released(&self.organ_a, |l, n| l.note_off(n)),
            organ_b: released(&self.organ_b, |l, n| l.note_off(n)),
            piano_a: released(&self.piano_a, |l, n| l.note_off(n)),
            piano_b: released(&self.piano_b, |l, n| l.note_off(n)),
            synth_a: released(&self.synth_a, |l, n| l.note_off(n)),
            synth_b: released(&self.synth_b, |l, n| l.note_off(n)),
            synth_c: released(&self.synth_c, |l, n| l.note_off(n)),
            fx: std::array::from_fn(|i| FxParams::capture(&self.fx_chains[i])),
            layer_levels: self.layer_levels,
            synth_cutoff_morph: self.synth_cutoff_morph,
            scene: self.scene,
            split_router: self.split_router,
            transpose: self.transpose,
            program_level_db: self.program_level_db,
            tempo_bpm: self.master_clock.bpm,
            compressor_amount: self.master_bus.compressor_amount,
            fx_sharing: self.fx_sharing,
        })
    }

    /// Replaces the panel state with `state` (moves the layers in; no clone). Held notes keep
    /// their hold counts, so later note-offs stay balanced.
    pub fn apply_state(&mut self, state: Box<ProgramState>) {
        let s = *state;
        self.organ_a = s.organ_a;
        self.organ_b = s.organ_b;
        self.piano_a = s.piano_a;
        self.piano_b = s.piano_b;
        self.synth_a = s.synth_a;
        self.synth_b = s.synth_b;
        self.synth_c = s.synth_c;
        for (chain, params) in self.fx_chains.iter_mut().zip(&s.fx) {
            params.apply(chain);
        }
        self.layer_levels = s.layer_levels;
        self.synth_cutoff_morph = s.synth_cutoff_morph;
        self.scene = s.scene;
        self.split_router = s.split_router;
        self.set_transpose(s.transpose);
        self.set_program_level_db(s.program_level_db);
        self.fx_sharing = s.fx_sharing;
        self.master_clock.bpm = s.tempo_bpm;
        self.master_bus.compressor_amount = s.compressor_amount;
        for fx in &mut self.fx_chains {
            fx.snap_delay_time(self.master_clock.bpm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_then_apply_restores_panel_state() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.drawbars[0] = 3;
        e.layer_levels[2].base = 0.33;
        e.fx_chains[4].delay_mix = 0.9;
        e.set_transpose(-4);
        e.set_program_level_db(-3.5);
        e.master_clock.bpm = 97.0;
        let state = e.capture_state();
        e.organ_a.drawbars[0] = 8;
        e.layer_levels[2].base = 0.9;
        e.fx_chains[4].delay_mix = 0.1;
        e.set_transpose(2);
        e.set_program_level_db(0.0);
        e.master_clock.bpm = 120.0;
        e.apply_state(state);
        assert_eq!(e.organ_a.drawbars[0], 3);
        assert_eq!(e.layer_levels[2].base, 0.33);
        assert_eq!(e.fx_chains[4].delay_mix, 0.9);
        assert_eq!(e.transpose(), -4);
        assert_eq!(e.program_level_db(), -3.5);
        assert_eq!(e.master_clock.bpm, 97.0);
    }

    #[test]
    fn captured_state_has_no_sounding_voices_that_outlive_release() {
        let mut e = Stage4Engine::new(48_000.0);
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        for _ in 0..512 {
            e.render_frame();
        }
        let state = e.capture_state();
        let mut restored = Stage4Engine::new(48_000.0);
        restored.apply_state(state);
        let tail: Vec<f32> = (0..48_000 * 6)
            .map(|_| restored.render_frame()[0])
            .collect();
        assert!(tail.iter().rev().take(512).all(|x| x.abs() < 1e-3));
    }
}
