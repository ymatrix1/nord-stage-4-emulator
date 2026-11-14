//! Proves the audio path performs no heap allocation while publishing telemetry.
//!
//! Approach (docs/specs/telemetry.md, "Allocation proof"): this file is its own test binary, so it
//! can install a counting `#[global_allocator]` without touching the other tests. The allocator
//! delegates to `System` and, only on a thread that has armed `GUARD`, counts every `alloc`,
//! `alloc_zeroed`, `realloc` and `dealloc`. The thread-locals are `const`-initialised `Cell`s
//! (no lazy init, no destructor), so reading them inside the allocator cannot allocate. Setup
//! (engine creation, lazy tables, the first warm-up blocks) runs unarmed; the measured region is
//! exactly the audio callback body: MIDI handling, `process_block` and the telemetry publish.

use nord_core::engine::{Stage4Engine, TelemetryCell};
use nord_core::midi::MidiEvent;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static EVENTS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

fn note_event() {
    // `try_with` so an allocation during thread teardown never panics inside the allocator.
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = EVENTS.try_with(|n| n.set(n.get() + 1));
        }
    });
}

// SAFETY: every method forwards unchanged arguments to `System`, which upholds the contract;
// the counting only touches `const`-initialised thread-local `Cell`s.
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

/// Runs `f` with counting armed on this thread; returns the number of allocator calls it made.
fn count_allocator_calls(f: impl FnOnce()) -> usize {
    EVENTS.with(|n| n.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    EVENTS.with(Cell::get)
}

#[test]
fn the_counter_sees_allocations_and_frees() {
    let n = count_allocator_calls(|| {
        let v: Vec<u8> = Vec::with_capacity(64);
        std::hint::black_box(&v);
        drop(v);
    });
    assert_eq!(n, 2, "one alloc and one dealloc must be counted");
    assert_eq!(count_allocator_calls(|| {}), 0);
}

fn note(on: bool, note: u8) -> MidiEvent {
    if on {
        MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        }
    } else {
        MidiEvent::NoteOff {
            channel: 0,
            note,
            velocity: 0,
        }
    }
}

#[test]
fn process_block_with_telemetry_publishing_never_touches_the_heap() {
    let mut e = Stage4Engine::new(48_000.0);
    // Sound on the three engines at once: Organ A, Piano A and Synth A are the default layers.
    let cell = Arc::new(TelemetryCell::new());
    e.attach_telemetry(Arc::clone(&cell));
    e.layer_levels[0].delta_wheel = 0.5;
    e.extern_out.layers[0].enabled = true;

    let mut left = vec![0.0f32; 256];
    let mut right = vec![0.0f32; 256];
    // Warm-up (unarmed): lazy tables and one-time initialisation may allocate here.
    e.handle_midi(note(true, 60));
    for _ in 0..4 {
        e.process_block(&mut left, &mut right);
    }
    e.handle_midi(note(false, 60));
    e.process_block(&mut left, &mut right);

    let calls = count_allocator_calls(|| {
        for block in 0..400u32 {
            let n = 36 + (block % 48) as u8;
            e.handle_midi(note(true, n));
            e.handle_midi(note(true, n + 7));
            e.handle_midi(MidiEvent::ControlChange {
                channel: 0,
                controller: nord_core::midi::cc::MOD_WHEEL,
                value: (block % 128) as u8,
            });
            e.process_block(&mut left, &mut right);
            e.handle_midi(note(false, n));
            e.handle_midi(note(false, n + 7));
            // The GUI-side read is lock-free too; include it on the same thread as the writer.
            assert!(cell.read().is_some());
        }
    });
    assert_eq!(calls, 0, "audio path made {calls} allocator calls");

    let t = cell.read().expect("telemetry published");
    assert!(t.midi_in_count >= 400 * 5, "counted {}", t.midi_in_count);
    assert!(left.iter().chain(&right).all(|s| s.is_finite()));
    assert!(left.iter().any(|s| s.abs() > 0.0), "the block was silent");
}

#[test]
fn publishing_alone_is_allocation_free() {
    let mut e = Stage4Engine::new(48_000.0);
    let cell = Arc::new(TelemetryCell::new());
    e.attach_telemetry(Arc::clone(&cell));
    e.publish_telemetry();
    let calls = count_allocator_calls(|| {
        for _ in 0..1000 {
            e.publish_telemetry();
        }
    });
    assert_eq!(calls, 0);
    assert_eq!(cell.publishes(), 1001);
}

/// B13: the worst-case benchmark workload (7 layers, 88 keys held, every FX rack on, four-output
/// routing) over a long run with key churn, a sustain pedal, tempo changes and both block entry
/// points. Any allocator call (alloc, free or realloc) on the audio thread fails the test.
#[test]
fn worst_case_polyphony_with_fx_and_multi_out_never_touches_the_heap() {
    use nord_core::bench::{press_all_keys, worst_case_engine, BLOCK_FRAMES, KEY_RANGE};
    let mut e = worst_case_engine(48_000.0);
    press_all_keys(&mut e);
    let mut bufs = [[0.0f32; BLOCK_FRAMES]; 4];
    // Warm-up (unarmed): lazy tables and one-time initialisation may allocate here.
    for _ in 0..50 {
        let [a, b, c, d] = &mut bufs;
        e.process_block_multi([&mut a[..], &mut b[..], &mut c[..], &mut d[..]]);
    }
    let keys: Vec<u8> = KEY_RANGE.collect();
    // 1500 blocks in release; debug builds are about 50x slower, so a shorter run there.
    let blocks = if cfg!(debug_assertions) {
        150usize
    } else {
        1500
    };
    let calls = count_allocator_calls(|| {
        for block in 0..blocks {
            let n = keys[block % keys.len()];
            // Retrigger: release and press again so voice stealing and note-off paths run too.
            e.handle_midi(note(false, n));
            e.handle_midi(note(true, n));
            e.handle_midi(MidiEvent::ControlChange {
                channel: 0,
                controller: nord_core::midi::cc::SUSTAIN_PEDAL,
                value: if block % 200 < 100 { 127 } else { 0 },
            });
            e.master_clock.bpm = 90.0 + (block % 60) as f32;
            let [a, b, c, d] = &mut bufs;
            if block % 2 == 0 {
                e.process_block_multi([&mut a[..], &mut b[..], &mut c[..], &mut d[..]]);
            } else {
                e.process_block(&mut a[..], &mut b[..]);
            }
        }
    });
    assert_eq!(
        calls, 0,
        "worst-case audio path made {calls} allocator calls"
    );
    assert!(bufs.iter().flatten().all(|s| s.is_finite()));
    assert!(
        bufs[0].iter().any(|s| s.abs() > 0.0),
        "the block was silent"
    );
}
