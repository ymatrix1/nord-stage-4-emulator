//! Program edits applied on the DSP thread (one variant per panel control).
//!
//! `AudioCommand::Program` carries them. Every variant holds plain values (enums, numbers) except
//! [`ProgramEdit::Snapshot`] / [`ProgramEdit::Recall`], which move a boxed program state.
//!
//! Real-time rule (TD-12, resolved): the standalone engine is owned by the audio callback, so the
//! allocating operations of `nord_core::engine` (layer init, copy/paste/swap, undo, snapshot,
//! recall; see [`ProgramEdit::is_control_op`]) never run there. `AudioHandle::send_command` hands
//! them to `audio::control::ProgramControl`, which runs them on a shadow engine and sends the audio
//! thread one allocation-free `SwapState`. [`ProgramEdit::apply`] still implements them for a
//! control-thread engine (tests, offline hosts); the audio callback drops any that reach it.

use crossbeam_channel::Sender;
use nord_core::engine::{LayerInitTarget, PanelFunc, ProgramState, Stage4Engine};
use nord_core::morph::{CrossfadeWidth, LayerId, MorphParameter, MorphSource};

/// What a clipboard operation acts on (manual p.43-44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyItem {
    /// One layer (panel state, effect chain and level).
    Layer(LayerId),
    /// The effect chain of one layer.
    Fx(LayerId),
    /// One morph source across the whole program.
    Morph(MorphSource),
    /// The whole program.
    Program,
}

/// A morph destination the Program area can edit without the other bays: layer levels and the
/// synth cutoffs (the two target families the engine exposes as `MorphParameter` arrays).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphTarget {
    /// Layer level `0..7` (Organ A/B, Piano A/B, Synth A/B/C).
    LayerLevel(u8),
    /// Synth cutoff `0..3` (Synth A/B/C).
    SynthCutoff(u8),
}

impl MorphTarget {
    /// Number of targets reachable from the Program area (7 levels + 3 cutoffs).
    pub const COUNT: usize = 10;

    /// Target `i` of `0..COUNT` (levels first); out of range clamps to the last one.
    pub fn from_index(i: usize) -> Self {
        match i {
            0..=6 => Self::LayerLevel(i as u8),
            n => Self::SynthCutoff((n - 7).min(2) as u8),
        }
    }

    /// Short label for the display.
    pub fn label(self) -> String {
        const LAYERS: [&str; 7] = [
            "Org A", "Org B", "Pno A", "Pno B", "Syn A", "Syn B", "Syn C",
        ];
        match self {
            Self::LayerLevel(i) => format!("{} Lvl", LAYERS[(i as usize).min(6)]),
            Self::SynthCutoff(i) => format!("{} Cut", LAYERS[4 + (i as usize).min(2)]),
        }
    }
}

/// The keyboard split as the panel edits it (applied together so the router is never half-set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitCfg {
    /// Split ON/SET state.
    pub on: bool,
    /// Per split point (Low, Mid, High): active.
    pub enabled: [bool; 3],
    /// Per split point: MIDI note.
    pub notes: [u8; 3],
    /// Per split point: crossfade width.
    pub xfade: [CrossfadeWidth; 3],
}

/// Panel edits for the Program area.
#[derive(Debug, Clone)]
pub enum ProgramEdit {
    /// TRANSP: semitones, clamped to +/-6 by the engine.
    Transpose(i8),
    /// PANIC (Shift+Transp).
    Panic,
    /// LAYER INIT (Shift+Section Edit).
    LayerInit(LayerInitTarget),
    /// UNDO (Shift+Solo): single level.
    Undo,
    /// Re-apply the edit the last UNDO reverted.
    Redo,
    /// MON/COPY.
    Copy(CopyItem),
    /// PASTE (Shift+Mon/Copy).
    Paste(CopyItem),
    /// PASTE then SWAP of two layers of one section.
    Swap(LayerId, LayerId),
    /// CLEAR (Shift+Morph Assign).
    ClearMorph(MorphSource),
    /// Morph depth `-1.0..=1.0` of `target` for `source`.
    MorphDepth {
        source: MorphSource,
        target: MorphTarget,
        depth: f32,
    },
    /// SPLIT ON/SET, SET KEY and the Keyboard Split screen.
    Split(SplitCfg),
    /// MST CLK tempo in BPM (clamped to 30..=300).
    MasterBpm(f32),
    /// LAYER SCENE II.
    LayerScene2(bool),
    /// PEDAL TAP (Shift+Mst Clk, manual p.40): a pedal set to "Panel Assigned" taps the Master
    /// Clock tempo while on; off returns the panel-assigned function to Layer Scene II.
    PedalTap(bool),
    /// STORE / Live auto-store: capture the program and send it back as `(ticket, state)`.
    Snapshot {
        ticket: u32,
        reply: Sender<(u32, Box<ProgramState>)>,
    },
    /// Recall a stored program (undoable).
    Recall(Box<ProgramState>),
    /// One menu item write (System, Sound, Output, Aux KB, Pedal, MIDI, Extern menus).
    Settings(super::SettingsEdit),
}

fn set_delta(p: &mut MorphParameter, source: MorphSource, depth: f32) {
    let d = if depth.is_finite() {
        depth.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    match source {
        MorphSource::ModWheel => p.delta_wheel = d,
        MorphSource::ControlPedal => p.delta_pedal = d,
        MorphSource::Aftertouch => p.delta_aftertouch = d,
    }
}

impl ProgramEdit {
    /// `true` for the edits that allocate or free layer-sized state (layer init, undo / redo,
    /// clipboard, clear morph, snapshot, recall). The standalone runs them on the control thread.
    pub fn is_control_op(&self) -> bool {
        matches!(
            self,
            Self::LayerInit(_)
                | Self::Undo
                | Self::Redo
                | Self::Copy(_)
                | Self::Paste(_)
                | Self::Swap(..)
                | Self::ClearMorph(_)
                | Self::Snapshot { .. }
                | Self::Recall(_)
        )
    }

    /// Apply the edit to a control-thread engine (the control operations allocate, see the module
    /// note); never call it for those from the audio callback.
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match self {
            Self::Transpose(s) => engine.set_transpose(*s),
            Self::Panic => engine.panic(),
            Self::LayerInit(t) => engine.layer_init(*t),
            Self::Undo => {
                engine.undo();
            }
            Self::Redo => {
                engine.redo();
            }
            Self::Copy(item) => match *item {
                CopyItem::Layer(id) => engine.copy_layer(id),
                CopyItem::Fx(id) => engine.copy_fx(id),
                CopyItem::Morph(s) => engine.copy_morph(s),
                CopyItem::Program => engine.copy_program(),
            },
            Self::Paste(item) => {
                // A refused paste (empty or mismatching clipboard) leaves the engine untouched.
                let _ = match *item {
                    CopyItem::Layer(id) => engine.paste_layer(id),
                    CopyItem::Fx(id) => engine.paste_fx(id),
                    CopyItem::Morph(s) => engine.paste_morph(s),
                    CopyItem::Program => engine.paste_program(),
                };
            }
            Self::Swap(a, b) => {
                let _ = engine.swap_layers(*a, *b);
            }
            Self::ClearMorph(s) => engine.clear_morph(*s),
            Self::MorphDepth {
                source,
                target,
                depth,
            } => {
                let p = match *target {
                    MorphTarget::LayerLevel(i) => engine.layer_levels.get_mut(i as usize),
                    MorphTarget::SynthCutoff(i) => engine.synth_cutoff_morph.get_mut(i as usize),
                };
                if let Some(p) = p {
                    set_delta(p, *source, *depth);
                }
            }
            Self::Split(cfg) => {
                for (i, s) in engine.split_router.splits.iter_mut().enumerate() {
                    s.enabled = cfg.on && cfg.enabled[i];
                    s.midi_note = cfg.notes[i].min(127);
                    s.crossfade = cfg.xfade[i];
                }
            }
            Self::MasterBpm(bpm) => {
                if bpm.is_finite() {
                    engine.master_clock.bpm = bpm.clamp(30.0, 300.0);
                }
            }
            Self::LayerScene2(on) => engine.scene.scene_2_active = *on,
            Self::PedalTap(on) => {
                engine.pedals.triple.panel_assigned = if *on {
                    PanelFunc::MstClkTap
                } else {
                    PanelFunc::LayerScene
                };
            }
            Self::Snapshot { ticket, reply } => {
                let _ = reply.send((*ticket, engine.capture_state()));
            }
            Self::Recall(state) => {
                engine.begin_undoable(nord_core::engine::UndoOp::ProgramChange);
                engine.apply_state(state.clone());
            }
            Self::Settings(edit) => {
                // An id no engine field backs is ignored (the menu only sends Bound items).
                let _ = edit.apply(engine);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        Stage4Engine::new(48_000.0)
    }

    #[test]
    fn transpose_panic_and_bpm_reach_the_engine() {
        let mut e = engine();
        ProgramEdit::Transpose(9).apply(&mut e);
        assert_eq!(e.transpose(), 6, "engine clamps to +/-6");
        ProgramEdit::MasterBpm(1000.0).apply(&mut e);
        assert_eq!(e.master_clock.bpm, 300.0);
        ProgramEdit::MasterBpm(f32::NAN).apply(&mut e);
        assert_eq!(e.master_clock.bpm, 300.0, "non-finite ignored");
        ProgramEdit::Panic.apply(&mut e);
    }

    #[test]
    fn pedal_tap_makes_a_panel_assigned_pedal_tap_the_master_clock() {
        use nord_core::engine::{TripleFunc, TriplePedal, TripleUnit};
        let mut e = engine();
        e.pedals.triple.tp2_attached = true;
        e.pedals.triple.set_func(
            TripleUnit::Tp2,
            TriplePedal::Left,
            TripleFunc::PanelAssigned,
        );
        let press = |e: &mut Stage4Engine| {
            e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
            e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 0);
        };
        // Off (default): the panel-assigned pedal is Layer Scene II, not a tap.
        press(&mut e);
        for _ in 0..30_000 {
            e.render_frame_multi();
        }
        press(&mut e);
        assert_eq!(e.master_clock.bpm, 120.0, "no tap while PEDAL TAP is off");
        ProgramEdit::PedalTap(true).apply(&mut e);
        press(&mut e);
        for _ in 0..30_000 {
            e.render_frame_multi();
        }
        press(&mut e); // 30000 frames at 48 kHz = 0.625 s -> 96 BPM
        assert!(
            (e.master_clock.bpm - 96.0).abs() < 0.5,
            "{}",
            e.master_clock.bpm
        );
        ProgramEdit::PedalTap(false).apply(&mut e);
        assert_eq!(e.pedals.triple.panel_assigned, PanelFunc::LayerScene);
    }

    #[test]
    fn layer_init_then_undo_restores_the_level() {
        let mut e = engine();
        e.layer_levels[2].base = 0.33;
        ProgramEdit::LayerInit(LayerInitTarget::All).apply(&mut e);
        assert_ne!(e.layer_levels[2].base, 0.33);
        ProgramEdit::Undo.apply(&mut e);
        assert_eq!(e.layer_levels[2].base, 0.33);
    }

    #[test]
    fn copy_paste_and_swap_move_layer_levels() {
        let mut e = engine();
        e.layer_levels[0].base = 0.1;
        e.layer_levels[1].base = 0.9;
        ProgramEdit::Swap(LayerId::OrganA, LayerId::OrganB).apply(&mut e);
        assert_eq!(e.layer_levels[0].base, 0.9);
        ProgramEdit::Copy(CopyItem::Layer(LayerId::OrganA)).apply(&mut e);
        ProgramEdit::Paste(CopyItem::Layer(LayerId::OrganB)).apply(&mut e);
        assert_eq!(e.layer_levels[1].base, 0.9);
        // Mismatching paste (organ clip into a piano layer) is refused without a panic.
        ProgramEdit::Paste(CopyItem::Layer(LayerId::PianoA)).apply(&mut e);
    }

    #[test]
    fn morph_depth_clear_and_targets() {
        let mut e = engine();
        let edit = ProgramEdit::MorphDepth {
            source: MorphSource::ModWheel,
            target: MorphTarget::LayerLevel(3),
            depth: 5.0,
        };
        edit.apply(&mut e);
        assert_eq!(e.layer_levels[3].delta_wheel, 1.0, "depth clamps");
        ProgramEdit::MorphDepth {
            source: MorphSource::Aftertouch,
            target: MorphTarget::SynthCutoff(2),
            depth: -0.5,
        }
        .apply(&mut e);
        assert_eq!(e.synth_cutoff_morph[2].delta_aftertouch, -0.5);
        ProgramEdit::ClearMorph(MorphSource::ModWheel).apply(&mut e);
        assert_eq!(e.layer_levels[3].delta_wheel, 0.0);
        assert_eq!(MorphTarget::from_index(7), MorphTarget::SynthCutoff(0));
        assert_eq!(MorphTarget::from_index(99), MorphTarget::SynthCutoff(2));
        assert_eq!(MorphTarget::from_index(2).label(), "Pno A Lvl");
    }

    #[test]
    fn split_scene_and_snapshot_round_trip() {
        let mut e = engine();
        let cfg = SplitCfg {
            on: true,
            enabled: [true, false, true],
            notes: [42, 60, 90],
            xfade: [
                CrossfadeWidth::Large,
                CrossfadeWidth::Off,
                CrossfadeWidth::Small,
            ],
        };
        ProgramEdit::Split(cfg).apply(&mut e);
        assert!(e.split_router.splits[0].enabled && !e.split_router.splits[1].enabled);
        assert_eq!(e.split_router.splits[2].midi_note, 90);
        ProgramEdit::LayerScene2(true).apply(&mut e);
        assert!(e.scene.scene_2_active);

        let (tx, rx) = crossbeam_channel::unbounded();
        ProgramEdit::Snapshot {
            ticket: 7,
            reply: tx,
        }
        .apply(&mut e);
        let (ticket, state) = rx.try_recv().expect("snapshot reply");
        assert_eq!(ticket, 7);
        ProgramEdit::LayerScene2(false).apply(&mut e);
        ProgramEdit::Recall(state).apply(&mut e);
        assert!(e.scene.scene_2_active, "recall restores the captured scene");
    }
}
