//! Undoable operations the engine owns (manual OS v1.6x p.42-43): preset load, Group and Global
//! effect modes, and All FX Off. Each takes the Undo snapshot with `begin_undoable` first.
//!
//! All of this allocates (snapshot, program load): control thread only, never `process_block`.
//!
//! Not engine-owned, so the host must wrap them with [`Stage4Engine::edit_undoable`]: loading a
//! single-layer Piano/Synth preset from a sound bank (`load_piano_npno_bytes`,
//! `load_synth_nsmp4_bytes`) and the Synth Group function for LFO / Filter / Arpeggiator, which
//! only exists as host-side parameter fan-out (`SectionEdit`) today.

use super::*;

impl Stage4Engine {
    /// Runs `edit` after taking an Undo snapshot labelled `op` (for host-owned operations).
    pub fn edit_undoable<R>(&mut self, op: UndoOp, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.begin_undoable(op);
        edit(self)
    }

    /// Loads a decoded program as an undoable "Load Preset".
    pub fn apply_program_undoable(&mut self, prog: &crate::cbin::ProgramDecoded) {
        self.begin_undoable(UndoOp::LoadPreset);
        self.apply_program(prog);
    }

    /// Replaces the GROUP / GLOBAL effect modes. Undoable when something changes: a Synth group
    /// change is "Synth Group", a Piano group change "FX Group", otherwise "Global FX". Returns
    /// the operation recorded, or `None` when `modes` equals the current modes.
    pub fn set_fx_sharing_undoable(&mut self, modes: FxSharing) -> Option<UndoOp> {
        let cur = self.fx_sharing;
        if modes == cur {
            return None;
        }
        let op = if modes.synth_group != cur.synth_group {
            UndoOp::SynthGroup
        } else if modes.piano_group != cur.piano_group {
            UndoOp::FxFocusGroup
        } else {
            UndoOp::GlobalFx
        };
        self.begin_undoable(op);
        self.fx_sharing = modes;
        Some(op)
    }

    /// All FX Off (Layer Effects area): turns every effect of every layer off and clears the
    /// GROUP / GLOBAL modes. Undoable.
    ///
    /// Reverb has no on/off flag in the chain (the program decoder zeroes `reverb_mix` when it is
    /// off), so it is switched off the same way. ASSUMED: the hardware keeps the knob settings;
    /// confirm on a Stage 4.
    pub fn all_fx_off(&mut self) {
        self.begin_undoable(UndoOp::AllFxOff);
        self.fx_sharing = FxSharing::default();
        for fx in &mut self.fx_chains {
            fx.mod1_type = crate::fx::Mod1Type::Off;
            fx.mod2_type = crate::fx::Mod2Type::Off;
            fx.amp_cabinet = AmpCabinet::Off;
            fx.comp_enabled = false;
            fx.delay_enabled = false;
            fx.reverb_mix = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::ProgramDecoded;
    use crate::fx::Mod1Type;

    #[test]
    fn program_load_is_undoable_and_redoable() {
        let mut e = Stage4Engine::new(48_000.0);
        e.master_clock.bpm = 100.0;
        let mut prog = ProgramDecoded::default();
        prog.global.tempo_bpm = 150.0;
        e.apply_program_undoable(&prog);
        assert_eq!(e.master_clock.bpm, 150.0);
        assert_eq!(e.undo(), Some(UndoOp::LoadPreset));
        assert_eq!(e.master_clock.bpm, 100.0);
        assert_eq!(e.redo(), Some(UndoOp::LoadPreset));
        assert_eq!(e.master_clock.bpm, 150.0);
    }

    #[test]
    fn fx_sharing_changes_pick_the_right_op_and_undo_restores_the_modes() {
        let mut e = Stage4Engine::new(48_000.0);
        assert_eq!(e.set_fx_sharing_undoable(FxSharing::default()), None);
        assert_eq!(e.undo_available(), None);
        let global = FxSharing {
            delay_global: Some(1),
            ..FxSharing::default()
        };
        assert_eq!(e.set_fx_sharing_undoable(global), Some(UndoOp::GlobalFx));
        assert_eq!(e.fx_sharing, global);
        e.undo();
        assert_eq!(e.fx_sharing, FxSharing::default());
        let synth = FxSharing {
            synth_group: Some(4),
            ..global
        };
        assert_eq!(e.set_fx_sharing_undoable(synth), Some(UndoOp::SynthGroup));
        let piano = FxSharing {
            piano_group: Some(2),
            ..synth
        };
        assert_eq!(e.set_fx_sharing_undoable(piano), Some(UndoOp::FxFocusGroup));
    }

    #[test]
    fn all_fx_off_turns_everything_off_and_is_undoable() {
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[3].mod1_type = Mod1Type::Wah;
        e.fx_chains[3].delay_enabled = true;
        e.fx_chains[3].reverb_mix = 0.6;
        e.fx_sharing.reverb_global = Some(3);
        e.all_fx_off();
        assert_eq!(e.fx_chains[3].mod1_type, Mod1Type::Off);
        assert!(!e.fx_chains[3].delay_enabled);
        assert_eq!(e.fx_chains[3].reverb_mix, 0.0);
        assert_eq!(e.fx_sharing, FxSharing::default());
        assert_eq!(e.undo(), Some(UndoOp::AllFxOff));
        assert_eq!(e.fx_chains[3].mod1_type, Mod1Type::Wah);
        assert_eq!(e.fx_chains[3].reverb_mix, 0.6);
        assert_eq!(e.fx_sharing.reverb_global, Some(3));
    }

    #[test]
    fn edit_undoable_wraps_host_owned_operations() {
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[0].base = 0.4;
        e.edit_undoable(UndoOp::SynthGroup, |e| e.layer_levels[0].base = 0.9);
        assert_eq!(e.undo(), Some(UndoOp::SynthGroup));
        assert_eq!(e.layer_levels[0].base, 0.4);
    }
}
