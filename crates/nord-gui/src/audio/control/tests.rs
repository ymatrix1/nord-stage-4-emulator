//! Control-thread tests: the shadow engine, the swap hand-off and Undo / Redo, with a second
//! `Stage4Engine` playing the audio thread.

use super::*;
use crate::audio::{CopyItem, ProgramEdit};
use crossbeam_channel::unbounded;
use nord_core::engine::LayerInitTarget;
use nord_core::morph::{LayerId, MorphSource};

/// A control thread plus a live engine that applies what the queue delivers.
struct Rig {
    control: ProgramControl,
    live: Box<Stage4Engine>,
    rx: Receiver<AudioCommand>,
    vol: f32,
    delivered: Vec<&'static str>,
}

impl Rig {
    fn new() -> Self {
        let (tx, rx) = bounded(256);
        Self {
            control: ProgramControl::new(48_000.0, tx),
            live: Box::new(standalone_engine(48_000.0)),
            rx,
            vol: 0.85_f32,
            delivered: Vec::new(),
        }
    }

    /// What `AudioHandle::send_command` does: control ops run here, the rest is forwarded.
    fn send(&mut self, cmd: AudioCommand) {
        if let Some(fwd) = self.control.dispatch(cmd) {
            let _ = self.control.to_audio.try_send(fwd);
        }
        self.pump();
    }

    fn edit(&mut self, e: ProgramEdit) {
        self.send(AudioCommand::Program(e));
    }

    /// The audio callback: apply every queued command.
    fn pump(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.delivered.push(match &cmd {
                AudioCommand::SwapState(_) => "swap",
                AudioCommand::Program(e) if e.is_control_op() => "control-op",
                _ => "other",
            });
            apply_audio_command(cmd, &mut self.live, &mut self.vol);
        }
    }

    fn level(&self, i: usize) -> f32 {
        self.live.layer_levels[i].base
    }
}

#[test]
fn control_ops_never_reach_the_audio_queue() {
    let mut r = Rig::new();
    for e in [
        ProgramEdit::LayerInit(LayerInitTarget::All),
        ProgramEdit::Copy(CopyItem::Layer(LayerId::OrganA)),
        ProgramEdit::Paste(CopyItem::Layer(LayerId::OrganB)),
        ProgramEdit::Swap(LayerId::SynthA, LayerId::SynthB),
        ProgramEdit::ClearMorph(MorphSource::ModWheel),
        ProgramEdit::Undo,
        ProgramEdit::Redo,
    ] {
        r.edit(e);
    }
    assert!(!r.delivered.contains(&"control-op"), "{:?}", r.delivered);
    assert!(r.delivered.contains(&"swap"));
}

#[test]
fn shadow_follows_plain_commands_and_layer_init_undo_redo_round_trips() {
    let mut r = Rig::new();
    r.send(AudioCommand::SetPianoLevel(0.33_f32));
    assert_eq!(r.level(2), 0.33_f32);
    assert_eq!(
        r.control.shadow().layer_levels[2].base,
        0.33_f32,
        "shadow mirrors"
    );

    r.edit(ProgramEdit::LayerInit(LayerInitTarget::PianoA));
    assert_ne!(
        r.level(2),
        0.33_f32,
        "layer init reset the level on the live engine"
    );
    assert_eq!(r.control.undo_status(), (Some(UndoOp::LayerInit), None));
    assert_eq!(
        r.live.undo_available(),
        Some(UndoOp::LayerInit),
        "hint reaches the audio engine"
    );

    r.edit(ProgramEdit::Undo);
    assert_eq!(r.level(2), 0.33, "undo restored the old level");
    assert_eq!(
        r.control.shadow().layer_levels[2].base,
        0.33_f32,
        "shadow follows undo"
    );
    assert_eq!(r.control.undo_status(), (None, Some(UndoOp::LayerInit)));
    assert_eq!(
        (r.live.undo_available(), r.live.redo_available()),
        (None, Some(UndoOp::LayerInit))
    );

    r.edit(ProgramEdit::Redo);
    assert_ne!(r.level(2), 0.33, "redo re-applied the init");
    assert_eq!(r.control.undo_status(), (Some(UndoOp::LayerInit), None));
    // Nothing to redo any more: a second redo changes nothing and sends nothing.
    let before = r.delivered.len();
    r.edit(ProgramEdit::Redo);
    assert_eq!(r.delivered.len(), before);
}

#[test]
fn layer_init_leaves_other_layers_running_untouched() {
    let mut r = Rig::new();
    r.send(AudioCommand::SetOrganLevel(0.5_f32));
    r.send(AudioCommand::Midi(MidiEvent::NoteOn {
        channel: 0,
        note: 64,
        velocity: 100,
    }));
    for _ in 0..256 {
        r.live.render_frame();
    }
    r.edit(ProgramEdit::LayerInit(LayerInitTarget::PianoB));
    assert_eq!(r.level(0), 0.5, "organ level untouched");
    let out = (0..2048)
        .map(|_| r.live.render_frame()[0].abs())
        .sum::<f32>();
    assert!(out > 0.0, "held note on the other layers still sounds");
}

#[test]
fn copy_paste_swap_and_clear_morph_go_through_the_shadow() {
    let mut r = Rig::new();
    r.send(AudioCommand::SetOrganLevel(0.1_f32));
    r.send(AudioCommand::SetOrganBLevel(0.9_f32));
    r.edit(ProgramEdit::Swap(LayerId::OrganA, LayerId::OrganB));
    assert_eq!((r.level(0), r.level(1)), (0.9_f32, 0.1_f32));
    assert_eq!(r.control.undo_status().0, Some(UndoOp::CopyPaste));

    r.edit(ProgramEdit::Copy(CopyItem::Layer(LayerId::OrganA)));
    r.send(AudioCommand::SetOrganBLevel(0.4_f32));
    r.edit(ProgramEdit::Paste(CopyItem::Layer(LayerId::OrganB)));
    assert_eq!(r.level(1), 0.9, "clipboard layer pasted");
    r.edit(ProgramEdit::Undo);
    assert_eq!(r.level(1), 0.4, "undo of the paste");

    r.edit(ProgramEdit::Paste(CopyItem::Layer(LayerId::PianoA)));
    assert_eq!(
        r.control.undo_status().0,
        None,
        "a refused paste (organ clip into a piano layer) records nothing"
    );

    r.edit(ProgramEdit::MorphDepth {
        source: MorphSource::ModWheel,
        target: crate::audio::MorphTarget::LayerLevel(3),
        depth: 0.7_f32,
    });
    assert_eq!(r.live.layer_levels[3].delta_wheel, 0.7_f32);
    r.edit(ProgramEdit::ClearMorph(MorphSource::ModWheel));
    assert_eq!(r.live.layer_levels[3].delta_wheel, 0.0_f32);
    r.edit(ProgramEdit::Undo);
    assert_eq!(
        r.live.layer_levels[3].delta_wheel, 0.7_f32,
        "undo of clear morph"
    );
}

#[test]
fn snapshot_replies_from_the_shadow_and_recall_swaps_the_program_in() {
    let mut r = Rig::new();
    r.send(AudioCommand::SetPianoLevel(0.42_f32));
    r.edit(ProgramEdit::Transpose(3));
    let (tx, rx) = unbounded();
    r.edit(ProgramEdit::Snapshot {
        ticket: 9,
        reply: tx,
    });
    let (ticket, state) = rx.try_recv().expect("reply is immediate");
    assert_eq!(ticket, 9);
    assert!(
        r.delivered.iter().all(|d| *d == "other"),
        "no audio round trip"
    );

    r.send(AudioCommand::SetPianoLevel(0.9_f32));
    r.edit(ProgramEdit::Transpose(-2));
    r.edit(ProgramEdit::Recall(state));
    assert_eq!(r.level(2), 0.42_f32);
    assert_eq!(r.live.transpose(), 3);
    assert_eq!(r.control.shadow().transpose(), 3);
    assert_eq!(r.control.undo_status().0, Some(UndoOp::ProgramChange));
    r.edit(ProgramEdit::Undo);
    assert_eq!(r.level(2), 0.9_f32);
    assert_eq!(r.live.transpose(), -2);
}

#[test]
fn a_stale_answer_is_dropped_and_a_new_operation_clears_redo() {
    let mut r = Rig::new();
    r.send(AudioCommand::SetPianoLevel(0.3_f32));
    r.edit(ProgramEdit::LayerInit(LayerInitTarget::PianoA));
    r.edit(ProgramEdit::Undo);
    assert!(r.control.undo_status().1.is_some());
    r.edit(ProgramEdit::LayerInit(LayerInitTarget::SynthA));
    assert_eq!(
        r.control.undo_status(),
        (Some(UndoOp::LayerInit), None),
        "redo branch cleared"
    );
    assert_eq!(r.live.redo_available(), None);

    // Two operations before the first answer is read: only the newest ticket may fill the slot.
    r.control
        .control_op(ProgramEdit::LayerInit(LayerInitTarget::SynthB));
    r.control
        .control_op(ProgramEdit::LayerInit(LayerInitTarget::SynthC));
    r.pump();
    r.control.drain();
    assert!(matches!(r.control.undo, Slot::Ready { .. }));
    r.edit(ProgramEdit::Undo);
    assert_eq!(r.control.undo_status().0, None);
}

#[test]
fn undo_with_nothing_stored_or_a_lost_answer_gives_up() {
    let mut r = Rig::new();
    r.edit(ProgramEdit::Undo);
    assert!(r.delivered.is_empty(), "no snapshot, no swap");
    // The swap is sent but the audio thread never answers: Undo times out and does nothing.
    r.control
        .control_op(ProgramEdit::LayerInit(LayerInitTarget::PianoA));
    let t0 = std::time::Instant::now();
    r.control.control_op(ProgramEdit::Undo);
    assert!(t0.elapsed() >= SETTLE_TIMEOUT);
    r.pump();
    assert_eq!(r.live.redo_available(), None);
}

#[test]
fn midi_controllers_reach_the_shadow_but_notes_do_not() {
    let mut r = Rig::new();
    r.control
        .observe(&AudioCommand::Midi(MidiEvent::ControlChange {
            channel: 0,
            controller: nord_core::midi::cc::MOD_WHEEL,
            value: 127,
        }));
    assert!(r.control.shadow().morph_controllers.mod_wheel > 0.9_f32);
    r.control.observe(&AudioCommand::Midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 100,
    }));
    let mut shadow = Box::new(r.control.shadow().clone());
    let energy: f32 = (0..512).map(|_| shadow.render_frame()[0].abs()).sum();
    assert_eq!(
        energy, 0.0_f32,
        "a note-on is not replayed, the shadow stays silent"
    );
}

#[test]
fn link_records_before_attach_then_replays_in_order() {
    let link = ControlLink::new();
    let (tx, rx) = bounded(64);
    assert!(!link.is_active());
    assert!(
        link.send(AudioCommand::SetPianoLevel(0.25_f32), &tx),
        "forwarded while waiting"
    );
    assert!(link.send(
        AudioCommand::Program(ProgramEdit::LayerInit(LayerInitTarget::SynthA)),
        &tx
    ));
    link.observe_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: nord_core::midi::cc::MOD_WHEEL,
        value: 64,
    });
    assert_eq!(rx.len(), 1, "the control op is deferred, not forwarded");
    link.attach(48_000.0, tx.clone());
    assert!(link.is_active());
    let kinds: Vec<_> = rx.try_iter().collect();
    assert_eq!(kinds.len(), 2, "plain command plus the replayed swap");
    assert!(matches!(kinds[1], AudioCommand::SwapState(_)));
    assert_eq!(link.undo_status().0, Some(UndoOp::LayerInit));
}

#[test]
fn keyboard_controllers_reach_the_shadow_only_while_local_control_is_on() {
    let wheel = MidiEvent::ControlChange {
        channel: 0,
        controller: nord_core::midi::cc::MOD_WHEEL,
        value: 100,
    };
    let mut r = Rig::new();
    r.control.shadow.midi_menu.local_control = false;
    r.control.observe(&AudioCommand::Keyboard(wheel));
    assert_eq!(r.control.shadow.morph_controllers.mod_wheel, 0.0_f32);
    r.control.observe(&AudioCommand::Midi(wheel));
    assert!(
        r.control.shadow.morph_controllers.mod_wheel > 0.7_f32,
        "MIDI In always applies"
    );
    r.control.shadow.morph_controllers.mod_wheel = 0.0_f32;
    r.control.shadow.midi_menu.local_control = true;
    r.control.observe(&AudioCommand::Keyboard(wheel));
    assert!(r.control.shadow.morph_controllers.mod_wheel > 0.7_f32);
}
