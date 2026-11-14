//! CPU frame cost of the full panel at 1x and 2x scale, with and without the panorama (B13).
//!
//! `egui::Context::run` builds the shapes (layout, label fitting, group frames, legibility
//! frames) and `Context::tessellate` turns them into meshes at the given pixels-per-point; both
//! run on the CPU of every real frame, so their sum is the CPU part of the "2x-scale frame"
//! budget in `docs/ROADMAP.md` B13. GPU rasterisation is not measured (no GPU in headless
//! tests). Run `cargo test --release -p nord-gui frame_cost -- --nocapture` for the real numbers;
//! the assertion is a generous regression guard only, so it holds on slow CI machines and in
//! debug builds.

use super::sim::Sim;
use egui::{Event, Key, Modifiers};
use std::time::Instant;

/// Mean milliseconds of `run` (shape building) and `tessellate` (meshing), and the vertex count.
struct Cost {
    run_ms: f64,
    tess_ms: f64,
    vertices: usize,
    shapes: usize,
}

fn press_p(sim: &mut Sim) {
    let ev = |pressed| Event::Key {
        key: Key::P,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    sim.frame(vec![ev(true)], Modifiers::NONE);
    sim.frame(vec![ev(false)], Modifiers::NONE);
}

fn measure(sim: &mut Sim, ppp: f32, frames: u32) -> Cost {
    sim.ctx.set_pixels_per_point(ppp);
    for _ in 0..3 {
        sim.frame(vec![], Modifiers::NONE);
        let _ = sim.ctx.tessellate(sim.shapes.clone(), ppp);
    }
    let (mut run, mut tess) = (0.0, 0.0);
    let mut vertices = 0;
    for _ in 0..frames {
        let t = Instant::now();
        sim.frame(vec![], Modifiers::NONE);
        run += t.elapsed().as_secs_f64();
        let shapes = sim.shapes.clone();
        let t = Instant::now();
        let prims = sim.ctx.tessellate(shapes, ppp);
        tess += t.elapsed().as_secs_f64();
        vertices = prims
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                egui::epaint::Primitive::Callback(_) => 0,
            })
            .sum();
    }
    let n = f64::from(frames);
    Cost {
        run_ms: run * 1000.0 / n,
        tess_ms: tess * 1000.0 / n,
        vertices,
        shapes: sim.shapes.len(),
    }
}

#[test]
fn frame_cost_full_panel_at_1x_and_2x_with_and_without_panorama() {
    let frames = if cfg!(debug_assertions) { 3 } else { 40 };
    let mut sim = Sim::full_panel();
    let mut worst = 0.0f64;
    for (label, panorama) in [("full panel", false), ("panorama", true)] {
        if panorama {
            press_p(&mut sim);
        }
        for ppp in [1.0f32, 2.0] {
            let c = measure(&mut sim, ppp, frames);
            eprintln!(
                "FRAME_COST {label} {ppp}x: run {:.2} ms + tessellate {:.2} ms = {:.2} ms, \
                 {} clipped shapes, {} vertices",
                c.run_ms,
                c.tess_ms,
                c.run_ms + c.tess_ms,
                c.shapes,
                c.vertices
            );
            assert!(
                c.vertices > 1000 && c.shapes > 100,
                "{label} painted nothing"
            );
            worst = worst.max(c.run_ms + c.tess_ms);
        }
    }
    let limit = if cfg!(debug_assertions) { 2000.0 } else { 40.0 };
    assert!(
        worst < limit,
        "worst frame {worst:.1} ms exceeds {limit} ms"
    );
}
