//! Headless host simulation of the plugin (B13): activate at host rates and block sizes, sample
//! accurate events, reset and re-activation, hostile events, the parameter map, and an allocation
//! count over a long run.
//!
//! This is the `Plugin` trait driven the way the CLAP and VST3 wrappers of nih-plug drive it
//! (`initialize`, `process` per block, `reset`). It is not a DAW and it does not load the built
//! `.clap` / `.vst3` binary: parameter automation and state save / load go through the wrapper
//! and were checked with `clap-validator` (see `docs/HOST_COMPATIBILITY.md`, which lists exactly
//! what was and was not verified). The file is its own test binary so it can install a counting
//! `#[global_allocator]`; counting is armed per thread and only around `process()`.

use nih_plug::prelude::*;
use nord_plugin::NordStage4Plugin;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::VecDeque;
use std::mem::MaybeUninit;

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

struct Init;
impl InitContext<NordStage4Plugin> for Init {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    fn execute(&self, _task: ()) {}
    fn set_latency_samples(&self, _samples: u32) {}
    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

/// A process context that replays a pre-reserved queue of events (no allocation when popped).
struct Ctx {
    transport: Transport,
    events: VecDeque<NoteEvent<()>>,
}

impl ProcessContext<NordStage4Plugin> for Ctx {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    fn execute_background(&self, _task: ()) {}
    fn execute_gui(&self, _task: ()) {}
    fn transport(&self) -> &Transport {
        &self.transport
    }
    fn next_event(&mut self) -> Option<NoteEvent<()>> {
        self.events.pop_front()
    }
    fn send_event(&mut self, _event: NoteEvent<()>) {}
    fn set_latency_samples(&self, _samples: u32) {}
    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

fn new_ctx(tempo: f64) -> Ctx {
    // SAFETY: `Transport` is plain data (numbers and options); all-zero is a valid value.
    let mut transport: Transport = unsafe { MaybeUninit::zeroed().assume_init() };
    transport.sample_rate = 48_000.0;
    transport.tempo = Some(tempo);
    Ctx {
        transport,
        events: VecDeque::with_capacity(4096),
    }
}

fn activate(sample_rate: f32, max_block: u32) -> NordStage4Plugin {
    let mut plugin = NordStage4Plugin::default();
    let ok = plugin.initialize(
        &NordStage4Plugin::AUDIO_IO_LAYOUTS[0],
        &BufferConfig {
            sample_rate,
            min_buffer_size: None,
            max_buffer_size: max_block,
            process_mode: ProcessMode::Realtime,
        },
        &mut Init,
    );
    assert!(ok, "initialize must succeed at {sample_rate} Hz");
    plugin
}

/// Runs one block of `frames` into `out` (two channels) and returns the process status.
fn process_block(
    plugin: &mut NordStage4Plugin,
    ctx: &mut Ctx,
    out: &mut [Vec<f32>; 2],
    frames: usize,
) -> ProcessStatus {
    let [l, r] = out;
    let slices: Vec<&mut [f32]> = vec![&mut l[..frames], &mut r[..frames]];
    let mut buffer = Buffer::default();
    // SAFETY: the slices outlive `buffer` and are exactly `frames` long.
    unsafe {
        buffer.set_slices(frames, |output| {
            *output = slices;
        });
    }
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut [],
    };
    plugin.process(&mut buffer, &mut aux, ctx)
}

fn note_on(timing: u32, note: u8) -> NoteEvent<()> {
    NoteEvent::NoteOn {
        timing,
        voice_id: None,
        channel: 0,
        note,
        velocity: 0.8,
    }
}

fn note_off(timing: u32, note: u8) -> NoteEvent<()> {
    NoteEvent::NoteOff {
        timing,
        voice_id: None,
        channel: 0,
        note,
        velocity: 0.0,
    }
}

fn all_finite(out: &[Vec<f32>; 2], frames: usize) -> bool {
    out.iter()
        .all(|c| c[..frames].iter().all(|s| s.is_finite()))
}

fn peak(out: &[Vec<f32>; 2], frames: usize) -> f32 {
    out.iter()
        .flat_map(|c| c[..frames].iter())
        .fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn activation_works_at_common_host_sample_rates_and_block_sizes() {
    for &sr in &[44_100.0f32, 48_000.0, 88_200.0, 96_000.0, 192_000.0] {
        let mut plugin = activate(sr, 1024);
        assert_eq!(plugin.engine.sample_rate, sr);
        let mut ctx = new_ctx(120.0);
        ctx.events.push_back(note_on(0, 60));
        let mut out = [vec![0.0f32; 1024], vec![0.0f32; 1024]];
        for &frames in &[1usize, 7, 32, 64, 128, 480, 1024] {
            assert_eq!(
                process_block(&mut plugin, &mut ctx, &mut out, frames),
                ProcessStatus::Normal
            );
            assert!(all_finite(&out, frames), "{sr} Hz, {frames} frames");
        }
        assert!(peak(&out, 1024) > 0.0, "{sr} Hz: held key was silent");
    }
}

#[test]
fn sample_accurate_note_timing_inside_a_block() {
    let mut plugin = activate(48_000.0, 512);
    let mut ctx = new_ctx(120.0);
    ctx.events.push_back(note_on(200, 60));
    let mut out = [vec![1.0f32; 512], vec![1.0f32; 512]];
    process_block(&mut plugin, &mut ctx, &mut out, 512);
    assert!(
        out[0][..200].iter().all(|s| *s == 0.0),
        "audio before the note-on offset must stay silent"
    );
}

#[test]
fn reset_and_reactivation_keep_the_plugin_sound_and_finite() {
    let mut plugin = activate(48_000.0, 256);
    let mut ctx = new_ctx(100.0);
    let mut out = [vec![0.0f32; 256], vec![0.0f32; 256]];
    ctx.events.push_back(note_on(0, 64));
    process_block(&mut plugin, &mut ctx, &mut out, 256);
    plugin.reset();
    plugin.deactivate();
    // Re-activation at another rate, as a host does after a project sample-rate change.
    let ok = plugin.initialize(
        &NordStage4Plugin::AUDIO_IO_LAYOUTS[0],
        &BufferConfig {
            sample_rate: 96_000.0,
            min_buffer_size: None,
            max_buffer_size: 256,
            process_mode: ProcessMode::Offline,
        },
        &mut Init,
    );
    assert!(ok);
    ctx.events.push_back(note_on(0, 64));
    for _ in 0..8 {
        process_block(&mut plugin, &mut ctx, &mut out, 256);
        assert!(all_finite(&out, 256));
    }
    assert!(peak(&out, 256) > 0.0);
}

#[test]
fn hostile_note_events_do_not_panic_and_output_stays_finite() {
    let mut plugin = activate(48_000.0, 128);
    let mut ctx = new_ctx(120.0);
    let mut out = [vec![0.0f32; 128], vec![0.0f32; 128]];
    // Wildcard keys (CLAP -1 arrives as 255), out-of-range channels, NaN-free extremes.
    for &note in &[0u8, 127, 128, 200, 255] {
        for &channel in &[0u8, 6, 15, 16, 255] {
            ctx.events.push_back(NoteEvent::NoteOn {
                timing: 3,
                voice_id: None,
                channel,
                note,
                velocity: 1.0,
            });
            ctx.events.push_back(NoteEvent::NoteOff {
                timing: 9,
                voice_id: None,
                channel,
                note,
                velocity: 1.0,
            });
            ctx.events.push_back(NoteEvent::Choke {
                timing: 11,
                voice_id: None,
                channel,
                note,
            });
        }
    }
    for _ in 0..8 {
        process_block(&mut plugin, &mut ctx, &mut out, 128);
        assert!(all_finite(&out, 128));
    }
}

#[test]
fn parameter_map_ids_are_unique_defaults_are_in_range_and_text_round_trips() {
    let plugin = activate(48_000.0, 128);
    let map = plugin.params.param_map();
    assert!(
        map.len() > 500,
        "expected the full parameter set, got {}",
        map.len()
    );
    let mut seen = std::collections::BTreeSet::new();
    for (id, ptr, _group) in &map {
        assert!(seen.insert(id.clone()), "duplicate parameter id {id}");
        // SAFETY: the pointers belong to `plugin.params`, which is alive for this loop.
        let (default, step) = unsafe { (ptr.default_normalized_value(), ptr.step_count()) };
        assert!((0.0..=1.0).contains(&default), "{id}: default {default}");
        let text = unsafe { ptr.normalized_value_to_string(default, false) };
        let back = unsafe { ptr.string_to_normalized_value(&text) };
        let back = back.unwrap_or_else(|| panic!("{id}: '{text}' did not parse"));
        let tol = match step {
            Some(n) if n > 0 => 0.5 / n as f32 + 1e-3,
            _ => 5e-2,
        };
        assert!(
            (back - default).abs() <= tol,
            "{id}: {default} -> '{text}' -> {back}"
        );
    }
}

#[test]
fn persisted_fields_survive_a_save_load_cycle() {
    let a = activate(48_000.0, 128);
    let fields = a.params.serialize_fields();
    assert!(
        fields.contains_key("editor-state"),
        "editor size must persist"
    );
    let b = activate(48_000.0, 128);
    b.params.deserialize_fields(&fields);
    assert_eq!(b.params.serialize_fields(), fields);
}

#[test]
fn process_never_touches_the_heap_over_a_long_worst_case_run() {
    let mut plugin = activate(48_000.0, 128);
    // Default scene (3 layers) with 88 keys held and churn; the 7-layer worst case is covered at
    // engine level (`nord-core/tests/no_alloc_audio_path.rs`). This run adds the plugin glue:
    // parameter sync of every parameter each block, event routing and the keybed queue.
    let mut ctx = new_ctx(120.0);
    let mut out = [vec![0.0f32; 128], vec![0.0f32; 128]];
    // Warm-up (unarmed): lazy tables and one-time initialisation may allocate here.
    for note in 21..=108u8 {
        ctx.events.push_back(note_on(0, note));
    }
    for _ in 0..40 {
        process_block(&mut plugin, &mut ctx, &mut out, 128);
    }
    // The host owns its buffers: build the buffer once so the count sees only `process()`.
    let [l, r] = &mut out;
    let slices: Vec<&mut [f32]> = vec![&mut l[..], &mut r[..]];
    let mut buffer = Buffer::default();
    // SAFETY: the slices outlive `buffer` and are exactly 128 frames long.
    unsafe {
        buffer.set_slices(128, |output| {
            *output = slices;
        });
    }
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut [],
    };
    // 1500 blocks in release; debug builds are far slower, so a shorter run there.
    let blocks = if cfg!(debug_assertions) {
        150usize
    } else {
        1500
    };
    let mut status = ProcessStatus::Normal;
    let started = std::time::Instant::now();
    let calls = count_allocator_calls(|| {
        for block in 0..blocks {
            let n = 21 + (block % 88) as u8;
            ctx.events.push_back(note_off(10, n));
            ctx.events.push_back(note_on(64, n));
            // The on-screen keybed queue is drained by `process()` too.
            let _ = plugin.note_tx.push(nord_core::midi::MidiEvent::NoteOn {
                channel: 0,
                note: n,
                velocity: 90,
            });
            ctx.transport.tempo = Some(80.0 + (block % 80) as f64);
            status = plugin.process(&mut buffer, &mut aux, &mut ctx);
        }
    });
    let per_block_ms = started.elapsed().as_secs_f64() * 1000.0 / blocks as f64;
    eprintln!("PLUGIN_PROCESS_MS {per_block_ms:.3} per 128-frame block (88 keys, 3 layers, counting allocator armed)");
    assert_eq!(status, ProcessStatus::Normal);
    assert_eq!(calls, 0, "plugin process() made {calls} allocator calls");
    let last = buffer.as_slice();
    assert!(last.iter().all(|c| c.iter().all(|s| s.is_finite())));
    assert!(
        last.iter().flat_map(|c| c.iter()).any(|s| s.abs() > 0.0),
        "the run was silent"
    );
}
