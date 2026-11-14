//! The 2-bus layout (main 1-2 plus aux 3-4) driven through `Plugin::process`: the aux port carries
//! what the Output menu routes to outputs 3-4, the main pair and the single-bus fallback stay
//! bit-identical, and `process()` still makes no allocator call with the aux port present.
//!
//! Own test binary because it installs a counting `#[global_allocator]` (same scheme as
//! `host_lifecycle.rs`, armed per thread and only around `process()`).

use nih_plug::prelude::*;
use nord_plugin::params::Stage4Params;
use nord_plugin::NordStage4Plugin;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
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

struct Init;
impl InitContext<NordStage4Plugin> for Init {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    fn execute(&self, _task: ()) {}
    fn set_latency_samples(&self, _samples: u32) {}
    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

struct Ctx {
    transport: Transport,
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
        None
    }
    fn send_event(&mut self, _event: NoteEvent<()>) {}
    fn set_latency_samples(&self, _samples: u32) {}
    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

const FRAMES: usize = 128;

fn ctx() -> Ctx {
    // SAFETY: `Transport` is plain data (numbers and options); all-zero is a valid value.
    let mut transport: Transport = unsafe { MaybeUninit::zeroed().assume_init() };
    transport.sample_rate = 48_000.0;
    transport.tempo = Some(120.0);
    Ctx { transport }
}

/// Activates with layout `layout` (0 = main + aux, 1 = stereo only).
fn activate(layout: usize) -> NordStage4Plugin {
    let mut plugin = NordStage4Plugin::default();
    let ok = plugin.initialize(
        &NordStage4Plugin::AUDIO_IO_LAYOUTS[layout],
        &BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: FRAMES as u32,
            process_mode: ProcessMode::Realtime,
        },
        &mut Init,
    );
    assert!(ok);
    plugin
}

/// Makes Organ, Piano A and Synth A default to output 3-4 (global mode choice index 1). Host
/// parameter values cannot be set from outside nih-plug, so the routed plugin is built with other
/// defaults; `sync_to_engine` then pushes them every block exactly as automation would.
fn route_everything_to_3_4(plugin: &mut NordStage4Plugin) {
    let mut params = Stage4Params::default();
    for i in [0usize, 2, 4] {
        params.output.sources[i].global =
            IntParam::new("test", 1, IntRange::Linear { min: 0, max: 7 });
    }
    plugin.params = std::sync::Arc::new(params);
}

fn hold_chord(plugin: &mut NordStage4Plugin) {
    for note in [48u8, 60, 64, 67] {
        assert!(plugin.note_tx.push(nord_core::midi::MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        }));
    }
}

fn buffer_of<'a>(slices: Vec<&'a mut [f32]>) -> Buffer<'a> {
    let mut buffer = Buffer::default();
    // SAFETY: the slices outlive `buffer` and are exactly `FRAMES` long.
    unsafe {
        buffer.set_slices(FRAMES, |output| {
            *output = slices;
        });
    }
    buffer
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|s| s.to_bits()).collect()
}

/// Renders `blocks` blocks; returns the main pair bits and the aux pair bits (aux empty when
/// `with_aux` is false).
fn render(plugin: &mut NordStage4Plugin, with_aux: bool, blocks: usize) -> [Vec<u32>; 4] {
    let mut c = ctx();
    let mut out: [Vec<u32>; 4] = Default::default();
    for _ in 0..blocks {
        let (mut ml, mut mr) = ([0.0f32; FRAMES], [0.0f32; FRAMES]);
        let (mut al, mut ar) = ([7.0f32; FRAMES], [7.0f32; FRAMES]);
        let mut main = buffer_of(vec![&mut ml[..], &mut mr[..]]);
        let mut host_aux = [buffer_of(vec![&mut al[..], &mut ar[..]])];
        let mut none: [Buffer; 0] = [];
        let mut aux = AuxiliaryBuffers {
            inputs: &mut [],
            outputs: if with_aux { &mut host_aux } else { &mut none },
        };
        plugin.process(&mut main, &mut aux, &mut c);
        let main_out = main.as_slice();
        out[0].extend(bits(main_out[0]));
        out[1].extend(bits(main_out[1]));
        if with_aux {
            let aux_out = aux.outputs[0].as_slice();
            out[2].extend(bits(aux_out[0]));
            out[3].extend(bits(aux_out[1]));
        }
    }
    out
}

#[test]
fn main_pair_is_bit_identical_with_and_without_the_aux_port() {
    for routed in [false, true] {
        let mut single = activate(1);
        let mut dual = activate(0);
        for p in [&mut single, &mut dual] {
            if routed {
                route_everything_to_3_4(p);
            }
            hold_chord(p);
        }
        let a = render(&mut single, false, 40);
        let b = render(&mut dual, true, 40);
        assert_eq!(a[0], b[0], "left differs (routed={routed})");
        assert_eq!(a[1], b[1], "right differs (routed={routed})");
        assert!(
            a[0].iter().any(|&x| f32::from_bits(x) != 0.0) != routed,
            "routing 3-4 must empty or fill the main pair (routed={routed})"
        );
    }
}

#[test]
fn aux_is_silent_by_default_and_carries_what_is_routed_to_3_4() {
    let mut plugin = activate(0);
    hold_chord(&mut plugin);
    let default = render(&mut plugin, true, 40);
    let zeros = |v: &[u32]| v.iter().all(|&b| f32::from_bits(b) == 0.0);
    assert!(zeros(&default[2]) && zeros(&default[3]), "aux not silenced");

    let mut routed = activate(0);
    route_everything_to_3_4(&mut routed);
    hold_chord(&mut routed);
    let out = render(&mut routed, true, 40);
    assert!(!zeros(&out[2]) || !zeros(&out[3]), "aux stayed silent");
    assert!(out[2]
        .iter()
        .chain(&out[3])
        .all(|&b| f32::from_bits(b).is_finite()));
    assert!(zeros(&out[0]) && zeros(&out[1]), "main kept a 3-4 source");
}

#[test]
fn two_bus_process_never_touches_the_heap() {
    let mut plugin = activate(0);
    route_everything_to_3_4(&mut plugin);
    let mut c = ctx();
    let (mut ml, mut mr) = ([0.0f32; FRAMES], [0.0f32; FRAMES]);
    let (mut al, mut ar) = ([0.0f32; FRAMES], [0.0f32; FRAMES]);
    let mut main = buffer_of(vec![&mut ml[..], &mut mr[..]]);
    let mut outputs = [buffer_of(vec![&mut al[..], &mut ar[..]])];
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut outputs,
    };
    for note in 21..=108u8 {
        let _ = plugin.note_tx.push(nord_core::midi::MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 90,
        });
    }
    // Warm-up (unarmed): lazy tables may allocate here.
    for _ in 0..40 {
        plugin.process(&mut main, &mut aux, &mut c);
    }
    let blocks = if cfg!(debug_assertions) { 150 } else { 1500 };
    EVENTS.with(|n| n.set(0));
    let mut aux_peak = 0.0f32;
    ARMED.with(|a| a.set(true));
    for block in 0..blocks {
        let n = 21 + (block % 88) as u8;
        let _ = plugin.note_tx.push(nord_core::midi::MidiEvent::NoteOn {
            channel: 0,
            note: n,
            velocity: 90,
        });
        plugin.process(&mut main, &mut aux, &mut c);
        for ch in aux.outputs[0].as_slice().iter() {
            aux_peak = ch.iter().fold(aux_peak, |m, s| m.max(s.abs()));
        }
    }
    ARMED.with(|a| a.set(false));
    let calls = EVENTS.with(Cell::get);
    assert_eq!(calls, 0, "2-bus process() made {calls} allocator calls");
    assert!(
        aux_peak > 0.0,
        "aux was silent with everything routed to 3-4"
    );
}
