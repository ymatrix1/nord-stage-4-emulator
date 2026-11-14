//! Allocation-free state hand-off between a control thread and the audio thread (TD-12).
//!
//! `capture_state`, `begin_undoable`, `layer_init`, the clipboard and `apply_state` allocate and
//! free whole layers, so they must not run inside the audio callback. A control thread prepares a
//! `Box<ProgramState>` instead; the audio thread only calls [`Stage4Engine::swap_state`], which
//! exchanges the selected parts with `std::mem::swap` (no allocation, no free) and gives the old
//! parts back inside the same box, ready to become the Undo / Redo snapshot or to be dropped on the
//! control thread. Spec: `docs/specs/program-handoff.md`.

use super::*;
use std::mem::swap;

/// Which parts of a [`ProgramState`] a swap exchanges. Bits combine with `|`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateMask(u32);

impl StateMask {
    /// Nothing.
    pub const NONE: Self = Self(0);
    /// Morph depths of the whole program: every layer level, every synth cutoff and every FX chain.
    pub const MORPH: Self = Self(1 << 24);
    /// Layer scene masks and the keybed split router.
    pub const SCENE: Self = Self(1 << 25);
    /// Transpose, Program Level, tempo, master compressor and FX sharing.
    pub const PROGRAM: Self = Self(1 << 26);
    /// Every layer (panel, effects, level) and every program-wide part.
    pub const ALL: Self = Self(0x7F | 0x7F << 8 | 0x7F << 16 | 1 << 24 | 1 << 25 | 1 << 26);

    /// Panel state of one layer (voices and DSP state included).
    pub const fn panel(id: LayerId) -> Self {
        Self(1 << id as u32)
    }

    /// Effect parameters of one layer.
    pub const fn fx(id: LayerId) -> Self {
        Self(1 << (8 + id as u32))
    }

    /// Level and (Synth) cutoff morph of one layer.
    pub const fn level(id: LayerId) -> Self {
        Self(1 << (16 + id as u32))
    }

    /// Everything that belongs to one layer: panel, effects and level.
    pub const fn layer(id: LayerId) -> Self {
        Self(Self::panel(id).0 | Self::fx(id).0 | Self::level(id).0)
    }

    /// `true` when every bit of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for StateMask {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Undo / Redo availability the control thread tells the audio thread (the slots themselves live on
/// the control thread, see `docs/specs/program-handoff.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UndoHint {
    pub undo: Option<UndoOp>,
    pub redo: Option<UndoOp>,
}

impl ProgramState {
    /// Releases every held key in the panel layers `mask` selects, so a stored snapshot never
    /// carries sounding voices. Control thread only (`note_off` may touch every voice).
    pub fn release_voices(&mut self, mask: StateMask) {
        let on = |id| mask.contains(StateMask::panel(id));
        for n in 0..128u8 {
            if on(LayerId::OrganA) {
                self.organ_a.note_off(n);
            }
            if on(LayerId::OrganB) {
                self.organ_b.note_off(n);
            }
            if on(LayerId::PianoA) {
                self.piano_a.note_off(n);
            }
            if on(LayerId::PianoB) {
                self.piano_b.note_off(n);
            }
            if on(LayerId::SynthA) {
                self.synth_a.note_off(n);
            }
            if on(LayerId::SynthB) {
                self.synth_b.note_off(n);
            }
            if on(LayerId::SynthC) {
                self.synth_c.note_off(n);
            }
        }
    }
}

impl Stage4Engine {
    /// Exchanges the parts of the engine that `mask` selects with `state`, using only moves:
    /// **no allocation and no deallocation**, safe inside the audio callback. Afterwards `state`
    /// holds what the engine had, so swapping again with the same mask restores it.
    pub fn swap_state(&mut self, state: &mut ProgramState, mask: StateMask) {
        let s = state;
        for id in LayerId::ALL {
            if mask.contains(StateMask::panel(id)) {
                self.swap_panel(id, s);
            }
        }
        let morph = mask.contains(StateMask::MORPH);
        for id in LayerId::ALL {
            let i = id as usize;
            if morph || mask.contains(StateMask::fx(id)) {
                let current = FxParams::capture(&self.fx_chains[i]);
                s.fx[i].apply(&mut self.fx_chains[i]);
                s.fx[i] = current;
            }
            if morph || mask.contains(StateMask::level(id)) {
                swap(&mut self.layer_levels[i], &mut s.layer_levels[i]);
                if let Some(c) = i.checked_sub(LayerId::SynthA as usize) {
                    swap(
                        &mut self.synth_cutoff_morph[c],
                        &mut s.synth_cutoff_morph[c],
                    );
                }
            }
        }
        if mask.contains(StateMask::SCENE) {
            swap(&mut self.scene, &mut s.scene);
            swap(&mut self.split_router, &mut s.split_router);
        }
        if mask.contains(StateMask::PROGRAM) {
            let transpose = self.transpose;
            self.set_transpose(s.transpose);
            s.transpose = transpose;
            let db = self.program_level_db;
            self.set_program_level_db(s.program_level_db);
            s.program_level_db = db;
            swap(&mut self.master_clock.bpm, &mut s.tempo_bpm);
            swap(
                &mut self.master_bus.compressor_amount,
                &mut s.compressor_amount,
            );
            swap(&mut self.fx_sharing, &mut s.fx_sharing);
        }
        let bpm = self.master_clock.bpm;
        for (i, fx) in self.fx_chains.iter_mut().enumerate() {
            let id = LayerId::ALL[i];
            if morph || mask.contains(StateMask::fx(id)) || mask.contains(StateMask::PROGRAM) {
                fx.snap_delay_time(bpm);
            }
        }
    }

    fn swap_panel(&mut self, id: LayerId, s: &mut ProgramState) {
        match id {
            LayerId::OrganA => swap(&mut self.organ_a, &mut s.organ_a),
            LayerId::OrganB => swap(&mut self.organ_b, &mut s.organ_b),
            LayerId::PianoA => swap(&mut self.piano_a, &mut s.piano_a),
            LayerId::PianoB => swap(&mut self.piano_b, &mut s.piano_b),
            LayerId::SynthA => swap(&mut self.synth_a, &mut s.synth_a),
            LayerId::SynthB => swap(&mut self.synth_b, &mut s.synth_b),
            LayerId::SynthC => swap(&mut self.synth_c, &mut s.synth_c),
        }
    }

    /// Records what the control thread says Undo / Redo would do (standalone: the Undo and Redo
    /// snapshots live on the control thread, so `undo_available` reports this hint).
    pub fn set_undo_hint(&mut self, hint: UndoHint) {
        self.undo_hint = hint;
    }

    /// The last [`UndoHint`] set by the control thread.
    pub fn undo_hint(&self) -> UndoHint {
        self.undo_hint
    }

    /// Counts a box or command the audio thread had to leak instead of free (return queue full, or
    /// a control operation that reached the callback); never free on the audio thread.
    pub fn note_retire_overflow(&mut self) {
        self.retire_overflow = self.retire_overflow.saturating_add(1);
    }

    /// Boxes and commands leaked instead of freed on the audio thread (should stay `0`).
    pub fn retire_overflow(&self) -> u32 {
        self.retire_overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organ::RotarySpeed;

    fn engine() -> Stage4Engine {
        Stage4Engine::new(48_000.0)
    }

    #[test]
    fn mask_bits_compose_and_all_covers_every_part() {
        let m = StateMask::layer(LayerId::PianoA);
        assert!(m.contains(StateMask::panel(LayerId::PianoA)));
        assert!(m.contains(StateMask::fx(LayerId::PianoA)));
        assert!(!m.contains(StateMask::panel(LayerId::PianoB)));
        assert!(!StateMask::NONE.contains(StateMask::MORPH));
        let all = LayerId::ALL.into_iter().fold(
            StateMask::MORPH | StateMask::SCENE | StateMask::PROGRAM,
            |a, id| a | StateMask::layer(id),
        );
        assert_eq!(all, StateMask::ALL);
    }

    #[test]
    fn full_swap_matches_apply_state_and_hands_the_old_state_back() {
        let mut e = engine();
        e.organ_a.drawbars[0] = 3;
        e.layer_levels[2].base = 0.33_f32;
        e.fx_chains[4].delay_mix = 0.9_f32;
        e.set_transpose(-4);
        e.set_program_level_db(-3.5_f32);
        e.master_clock.bpm = 97.0_f32;
        let mut stored = e.capture_state();

        e.organ_a.drawbars[0] = 8;
        e.layer_levels[2].base = 0.9_f32;
        e.fx_chains[4].delay_mix = 0.1_f32;
        e.set_transpose(2);
        e.set_program_level_db(0.0_f32);
        e.master_clock.bpm = 120.0_f32;
        e.swap_state(&mut stored, StateMask::ALL);

        assert_eq!(e.organ_a.drawbars[0], 3);
        assert_eq!(e.layer_levels[2].base, 0.33_f32);
        assert_eq!(e.fx_chains[4].delay_mix, 0.9_f32);
        assert_eq!(e.transpose(), -4);
        assert_eq!(e.program_level_db(), -3.5_f32);
        assert_eq!(e.master_clock.bpm, 97.0_f32);
        // The box now holds the state the engine had before the swap.
        assert_eq!(stored.organ_a.drawbars[0], 8);
        assert_eq!(stored.layer_levels[2].base, 0.9_f32);
        assert_eq!(stored.transpose, 2);
        assert_eq!(stored.tempo_bpm, 120.0_f32);

        e.swap_state(&mut stored, StateMask::ALL);
        assert_eq!(e.organ_a.drawbars[0], 8, "a second swap is the inverse");
        assert_eq!(e.transpose(), 2);
        assert_eq!(e.master_clock.bpm, 120.0_f32);
    }

    #[test]
    fn a_layer_mask_leaves_every_other_part_alone() {
        let mut e = engine();
        e.piano_a.timbre = crate::piano::PianoTimbre::Soft;
        e.layer_levels[2].base = 0.2_f32;
        e.fx_chains[2].reverb_mix = 0.7_f32;
        let mut incoming = e.capture_state();
        e.piano_a.timbre = crate::piano::PianoTimbre::Off;
        e.layer_levels[2].base = 0.6_f32;
        e.fx_chains[2].reverb_mix = 0.1_f32;
        e.organ_a.rotary_speed = RotarySpeed::Fast;
        e.layer_levels[0].base = 0.5_f32;
        e.set_transpose(3);
        e.swap_state(&mut incoming, StateMask::layer(LayerId::PianoA));
        assert_eq!(e.piano_a.timbre, crate::piano::PianoTimbre::Soft);
        assert_eq!(e.layer_levels[2].base, 0.2_f32);
        assert_eq!(e.fx_chains[2].reverb_mix, 0.7_f32);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast, "organ untouched");
        assert_eq!(e.layer_levels[0].base, 0.5_f32);
        assert_eq!(e.transpose(), 3, "program part untouched");
        assert_eq!(incoming.layer_levels[2].base, 0.6_f32);
    }

    #[test]
    fn morph_mask_swaps_depths_everywhere_but_no_panel_layer() {
        let mut e = engine();
        e.layer_levels[1].delta_wheel = 0.4_f32;
        e.synth_cutoff_morph[2].delta_pedal = -0.3_f32;
        e.fx_chains[3].morph.reverb_amount.delta_wheel = 0.25_f32;
        let mut incoming = e.capture_state();
        e.clear_morph(crate::morph::MorphSource::ModWheel);
        e.clear_morph(crate::morph::MorphSource::ControlPedal);
        e.organ_a.drawbars[1] = 1;
        assert_eq!(e.layer_levels[1].delta_wheel, 0.0_f32);
        e.swap_state(&mut incoming, StateMask::MORPH);
        assert_eq!(e.layer_levels[1].delta_wheel, 0.4_f32);
        assert_eq!(e.synth_cutoff_morph[2].delta_pedal, -0.3_f32);
        assert_eq!(e.fx_chains[3].morph.reverb_amount.delta_wheel, 0.25_f32);
        assert_eq!(e.organ_a.drawbars[1], 1, "panel layers keep their state");
    }

    #[test]
    fn scene_and_program_parts_swap_independently() {
        let mut e = engine();
        e.scene.scene_2_active = true;
        e.split_router.splits[0].enabled = true;
        e.set_transpose(5);
        e.fx_sharing.reverb_global = Some(1);
        let mut incoming = e.capture_state();
        e.scene.scene_2_active = false;
        e.split_router.splits[0].enabled = false;
        e.set_transpose(0);
        e.fx_sharing = FxSharing::default();
        e.swap_state(&mut incoming, StateMask::SCENE);
        assert!(e.scene.scene_2_active && e.split_router.splits[0].enabled);
        assert_eq!(e.transpose(), 0, "program part not selected");
        e.swap_state(&mut incoming, StateMask::PROGRAM);
        assert_eq!(e.transpose(), 5);
        assert_eq!(e.fx_sharing.reverb_global, Some(1));
    }

    #[test]
    fn release_voices_silences_a_stored_layer() {
        let mut e = engine();
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        for _ in 0..512 {
            e.render_frame();
        }
        // A raw clone keeps the key down; releasing it lets the tail die out.
        let mut sounding = e.capture_state();
        sounding.release_voices(StateMask::ALL);
        let mut restored = engine();
        restored.swap_state(&mut sounding, StateMask::ALL);
        let tail: Vec<f32> = (0..48_000 * 6)
            .map(|_| restored.render_frame()[0])
            .collect();
        assert!(tail.iter().rev().take(512).all(|x| x.abs() < 1e-3));
    }

    #[test]
    fn undo_hint_is_reported_by_undo_available() {
        let mut e = engine();
        assert_eq!(e.undo_available(), None);
        e.set_undo_hint(UndoHint {
            undo: Some(UndoOp::LayerInit),
            redo: Some(UndoOp::CopyPaste),
        });
        assert_eq!(e.undo_available(), Some(UndoOp::LayerInit));
        assert_eq!(e.redo_available(), Some(UndoOp::CopyPaste));
        assert_eq!(e.undo_hint().undo, Some(UndoOp::LayerInit));
        e.note_retire_overflow();
        assert_eq!(e.retire_overflow(), 1);
    }
}
