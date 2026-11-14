//! Deterministic golden-render dump used to prove refactors are behaviour-preserving.
//!
//! Usage: `cargo run --release -p nord-core --example render_hash -- <out.f32le> [--timing]`
//! then `sha256sum <out.f32le>`. The file concatenates little-endian f32 stereo frames of:
//! the full engine on a chord/CC timeline, each synth oscillator mode (incl. a looped synthetic
//! sample at several playback ratios), the organ and piano layers and the FX sweep. The same
//! segments come from `tests/support/render_segments.rs`; the example also prints the SHA-256 of
//! each segment (`engine`, `synth`, `organ`, `piano`, `fx`), the values `tests/golden_render.rs`
//! pins. With `--timing` it also prints ns/frame for the sample oscillator and the full engine.

#[path = "../tests/support/render_segments.rs"]
mod render_segments;

use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use render_segments::{sample_layer, sha256_hex, FS};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("output path");
    let mut out = Vec::new();
    for (name, bytes) in render_segments::segments() {
        println!(
            "segment {name}: {} bytes sha256 {}",
            bytes.len(),
            sha256_hex(&bytes)
        );
        out.extend_from_slice(&bytes);
    }
    std::fs::write(path, &out).expect("write");
    println!("wrote {} bytes", out.len());
    println!("sha256 {}", sha256_hex(&out));

    let on = |n: u8| MidiEvent::NoteOn {
        channel: 0,
        note: n,
        velocity: 100,
    };
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
