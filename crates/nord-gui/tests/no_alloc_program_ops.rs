//! TD-12 evidence: the audio-thread side of every allocating program edit performs no heap
//! allocation and no free.
//!
//! Same approach as `crates/nord-core/tests/no_alloc_audio_path.rs`: this file is its own test
//! binary with a counting `#[global_allocator]` that counts `alloc`, `alloc_zeroed`, `realloc` and
//! `dealloc` only on a thread that armed a `const` thread-local flag. The control thread (this test
//! body, unarmed) sends the edits through `AudioHandle::send_command`; the "audio callback" is the
//! armed region that drains the queue with `apply_audio_command` exactly like `io.rs` does.

use nord_core::engine::{LayerInitTarget, Stage4Engine};
use nord_core::morph::{LayerId, MorphSource};
use nord_gui::audio::{
    apply_audio_command, standalone_engine, AudioCommand, AudioHandle, CopyItem, ProgramEdit,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static EVENTS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

fn note_event() {
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = EVENTS.try_with(|n| n.set(n.get() + 1));
        }
    });
}

// SAFETY: forwards unchanged arguments to `System`; counting touches only const thread-locals.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_event();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note_event();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_event();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note_event();
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn count_allocator_calls(f: impl FnOnce()) -> usize {
    EVENTS.with(|n| n.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    EVENTS.with(Cell::get)
}

/// A standalone handle with its shadow attached, plus the engine the audio thread would own.
fn rig() -> (
    AudioHandle,
    crossbeam_channel::Receiver<AudioCommand>,
    Stage4Engine,
) {
    let (handle, rx) = AudioHandle::standalone();
    handle
        .control()
        .expect("standalone handle has a control link")
        .attach(48_000.0, handle.cmd_sender.clone());
    (handle, rx, standalone_engine(48_000.0))
}

/// The audio callback's command loop (as in `io.rs`), counted.
fn audio_callback(
    rx: &crossbeam_channel::Receiver<AudioCommand>,
    engine: &mut Stage4Engine,
) -> usize {
    let mut vol = 0.85_f32;
    count_allocator_calls(|| {
        while let Ok(cmd) = rx.try_recv() {
            apply_audio_command(cmd, engine, &mut vol);
        }
        let mut l = [0.0f32; 64];
        let mut r = [0.0f32; 64];
        engine.process_block(&mut l, &mut r);
    })
}

#[test]
fn the_old_path_allocated_on_the_audio_thread() {
    // The pre-fix behaviour: applying the edit directly on the engine (what the callback used to
    // do) allocates and frees whole layers. This proves the counter would catch a regression.
    let mut e = standalone_engine(48_000.0);
    let calls =
        count_allocator_calls(|| ProgramEdit::LayerInit(LayerInitTarget::All).apply(&mut e));
    eprintln!("legacy LayerInit(All) on the audio thread: {calls} allocator calls");
    assert!(calls > 50, "legacy apply made only {calls} allocator calls");
}

#[test]
fn every_control_op_is_allocation_free_on_the_audio_thread() {
    let (handle, rx, mut engine) = rig();
    let ops = [
        ProgramEdit::LayerInit(LayerInitTarget::All),
        ProgramEdit::LayerInit(LayerInitTarget::OrganAb),
        ProgramEdit::LayerInit(LayerInitTarget::SynthB),
        ProgramEdit::Undo,
        ProgramEdit::Redo,
        ProgramEdit::Copy(CopyItem::Layer(LayerId::OrganA)),
        ProgramEdit::Paste(CopyItem::Layer(LayerId::OrganB)),
        ProgramEdit::Undo,
        ProgramEdit::Copy(CopyItem::Fx(LayerId::PianoA)),
        ProgramEdit::Paste(CopyItem::Fx(LayerId::SynthC)),
        ProgramEdit::Swap(LayerId::SynthA, LayerId::SynthB),
        ProgramEdit::Copy(CopyItem::Morph(MorphSource::ModWheel)),
        ProgramEdit::Paste(CopyItem::Morph(MorphSource::Aftertouch)),
        ProgramEdit::ClearMorph(MorphSource::ModWheel),
        ProgramEdit::Copy(CopyItem::Program),
        ProgramEdit::Paste(CopyItem::Program),
        ProgramEdit::Undo,
    ];
    // Warm-up: first use of lazily built tables happens outside the measured region.
    engine.process_block(&mut [0.0_f32; 64], &mut [0.0_f32; 64]);
    let mut swaps = 0;
    for op in ops {
        assert!(handle.send_command(AudioCommand::Program(op.clone())));
        // Snapshots are answered by the control thread; the callback never sees them.
        let queued = rx.len();
        let calls = audio_callback(&rx, &mut engine);
        assert_eq!(
            calls, 0,
            "{op:?}: audio callback made {calls} allocator calls"
        );
        swaps += queued;
    }
    assert!(swaps >= 10, "only {swaps} swaps reached the audio queue");
    assert_eq!(engine.retire_overflow(), 0, "no box was leaked");
}

#[test]
fn recall_and_snapshot_are_allocation_free_on_the_audio_thread() {
    let (handle, rx, mut engine) = rig();
    engine.process_block(&mut [0.0_f32; 64], &mut [0.0_f32; 64]);
    handle.send_command(AudioCommand::SetPianoLevel(0.42_f32));
    let (tx, reply) = crossbeam_channel::unbounded();
    handle.send_command(AudioCommand::Program(ProgramEdit::Snapshot {
        ticket: 1,
        reply: tx,
    }));
    let (_, state) = reply
        .try_recv()
        .expect("snapshot answered by the control thread");
    handle.send_command(AudioCommand::SetPianoLevel(0.9_f32));
    handle.send_command(AudioCommand::Program(ProgramEdit::Recall(state)));
    let calls = audio_callback(&rx, &mut engine);
    assert_eq!(calls, 0, "recall made {calls} allocator calls");
    assert_eq!(
        engine.layer_levels[2].base, 0.42_f32,
        "the recalled level is live"
    );
    assert!(
        engine.undo_available().is_some(),
        "undo is available through the hint"
    );
}
