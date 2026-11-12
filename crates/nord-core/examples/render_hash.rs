//! Deterministic golden-render dump used to prove refactors are behaviour-preserving.
//!
//! Usage: `cargo run --release -p nord-core --example render_hash -- <out.f32le> [--timing]`
//! then `sha256sum <out.f32le>`. The file concatenates little-endian f32 stereo frames of:
//! the full engine on a chord/CC timeline, each synth oscillator mode (incl. a looped synthetic
//! sample at several playback ratios), the organ and piano layers.
//! With `--timing` it also prints ns/frame for the sample oscillator and the full engine.

use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use nord_core::organ::OrganLayer;
use nord_core::piano::PianoLayer;
use nord_core::render::{render_timeline, TimedEvent};
use nord_core::synth::{SampleTimbre, SynthLayer, SynthOscMode, SynthSampleSound};
use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::Instant;

const FS: f32 = 48_000.0;

fn push(out: &mut Vec<u8>, frame: [f32; 2]) {
    out.extend_from_slice(&frame[0].to_le_bytes());
    out.extend_from_slice(&frame[1].to_le_bytes());
}

fn sample_sound() -> Arc<SynthSampleSound> {
    let pcm: Vec<f32> = (0..2048)
        .map(|i| {
            let ph = i as f32 / 40.0;
            (TAU * ph).sin() + 0.4 * (3.0 * TAU * ph).sin() + 0.1 * (11.0 * TAU * ph).sin()
        })
        .collect();
    Arc::new(SynthSampleSound::new_synthetic(
        "HashSample",
        60,
        &pcm,
        FS,
        Some((512, 1536)),
        32,
    ))
}

fn sample_layer(note: u8) -> SynthLayer {
    let mut s = SynthLayer::new(FS);
    s.osc_mode = SynthOscMode::Sample;
    s.sample_timbre = SampleTimbre::Bright;
    s.cutoff_hz = 16_000.0;
    s.load_sample(Some(sample_sound()));
    s.note_on(note, 110);
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("output path");
    let mut out = Vec::new();

    let mut engine = Stage4Engine::new(FS);
    let ev = |frame: usize, e: MidiEvent| TimedEvent { frame, event: e };
    let on = |n: u8| MidiEvent::NoteOn {
        channel: 0,
        note: n,
        velocity: 100,
    };
    let off = |n: u8| MidiEvent::NoteOff {
        channel: 0,
        note: n,
        velocity: 0,
    };
    let cc = |c: u8, v: u8| MidiEvent::ControlChange {
        channel: 0,
        controller: c,
        value: v,
    };
    let events = [
        ev(0, on(48)),
        ev(0, on(60)),
        ev(0, on(64)),
        ev(0, on(67)),
        ev(10_000, cc(1, 90)),
        ev(20_000, off(60)),
        ev(24_000, on(72)),
        ev(30_000, cc(64, 127)),
        ev(40_000, off(64)),
    ];
    let (l, r) = render_timeline(&mut engine, &events, 60_000);
    for (a, b) in l.iter().zip(&r) {
        push(&mut out, [*a, *b]);
    }

    for mode in [
        SynthOscMode::Analog,
        SynthOscMode::Wavetable,
        SynthOscMode::Fm,
    ] {
        let mut s = SynthLayer::new(FS);
        s.osc_mode = mode;
        s.note_on(57, 100);
        s.note_on(64, 100);
        for _ in 0..12_000 {
            push(&mut out, s.render_frame());
        }
    }
    for note in [48u8, 60, 67, 72, 84] {
        let mut s = sample_layer(note);
        for _ in 0..12_000 {
            push(&mut out, s.render_frame());
        }
    }

    let mut organ = OrganLayer::new(FS);
    organ.note_on(60);
    organ.note_on(67);
    for _ in 0..12_000 {
        push(&mut out, organ.render_frame());
    }
    let mut piano = PianoLayer::new(FS);
    piano.note_on(60, 100);
    piano.note_on(72, 80);
    for _ in 0..12_000 {
        push(&mut out, piano.render_frame());
    }

    std::fs::write(path, &out).expect("write");
    println!("wrote {} bytes", out.len());

    if args.iter().any(|a| a == "--timing") {
        for note in [60u8, 72] {
            let mut best = f64::MAX;
            for _ in 0..5 {
                let mut s = sample_layer(note);
                let n = 200_000;
                let t = Instant::now();
                let mut acc = 0.0f32;
                for _ in 0..n {
                    acc += s.render_frame()[0];
                }
                let ns = t.elapsed().as_nanos() as f64 / n as f64;
                std::hint::black_box(acc);
                best = best.min(ns);
            }
            println!("timing sample-osc note {note}: {best:.1} ns/frame (1 voice, whole layer)");
        }
        micro_interp();
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut e = Stage4Engine::new(FS);
            for n in [48u8, 60, 64, 67] {
                e.handle_midi(on(n));
            }
            let mut l = vec![0.0f32; 4096];
            let mut r = vec![0.0f32; 4096];
            let t = Instant::now();
            for _ in 0..50 {
                e.process_block(&mut l, &mut r);
            }
            best = best.min(t.elapsed().as_nanos() as f64 / (50.0 * 4096.0));
        }
        println!("timing engine 4-note chord: {best:.1} ns/frame");
    }
}

/// Reference copy of the legacy 4-point Catmull-Rom (`synth/sample_osc.rs::cubic_pcm`).
fn cubic(pcm: &[f32], pos: f64) -> f32 {
    let max_idx = pcm.len() - 1;
    let idx1 = (pos.floor() as usize).min(max_idx);
    let idx2 = (idx1 + 1).min(max_idx);
    let frac = (pos - idx1 as f64).clamp(0.0, 1.0) as f32;
    let (p0, p1, p2, p3) = (
        pcm[idx1.saturating_sub(1)],
        pcm[idx1],
        pcm[idx2],
        pcm[(idx2 + 1).min(max_idx)],
    );
    let (t2, t3) = (frac * frac, frac * frac * frac);
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * frac
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Interpolator-only cost: legacy cubic vs 12-tap sinc, ns per interpolated sample.
fn micro_interp() {
    let pcm: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.07).sin()).collect();
    let kernels = nord_core::sinc::SincKernelSet::new();
    let kernel = kernels.for_ratio(1.0);
    let n = 2_000_000;
    let (mut c_best, mut s_best) = (f64::MAX, f64::MAX);
    for _ in 0..5 {
        let (mut pos, mut acc) = (4.0f64, 0.0f32);
        let t = Instant::now();
        for _ in 0..n {
            acc += cubic(&pcm, pos);
            pos += 1.37;
            if pos > 4000.0 {
                pos = 4.0;
            }
        }
        c_best = c_best.min(t.elapsed().as_nanos() as f64 / n as f64);
        std::hint::black_box(acc);
        let (mut pos, mut acc) = (4.0f64, 0.0f32);
        let t = Instant::now();
        for _ in 0..n {
            acc += kernel.interpolate(&pcm, pos);
            pos += 1.37;
            if pos > 4000.0 {
                pos = 4.0;
            }
        }
        s_best = s_best.min(t.elapsed().as_nanos() as f64 / n as f64);
        std::hint::black_box(acc);
    }
    println!("timing interpolator only: cubic {c_best:.2} ns, sinc12 {s_best:.2} ns per sample");
}
