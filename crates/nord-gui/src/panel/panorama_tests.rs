//! Headless tests for the panorama view: keys, strip drag, wheel, click-through to controls.

use super::panorama::{centered, strip_rect, strip_to_panel, STRIP_H};
use super::sim::Sim;
use super::view::{region_size, PANEL_SIZE};
use crate::audio::AudioCommand;
use egui::{pos2, vec2, Event, Key, Modifiers, MouseWheelUnit};

fn key(sim: &mut Sim, key: Key) {
    let ev = |pressed| Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    sim.frame(vec![ev(true)], Modifiers::NONE);
    sim.frame(vec![ev(false)], Modifiers::NONE);
}

fn viewport(sim: &Sim) -> egui::Rect {
    sim.state.view.panorama.expect("panorama on")
}

#[test]
fn p_toggles_the_mode_and_esc_leaves_it() {
    let mut sim = Sim::full_panel();
    assert_eq!(sim.state.view.panorama, None);
    key(&mut sim, Key::P);
    let vp = viewport(&sim);
    assert!((vp.width() - PANEL_SIZE.x * 0.5).abs() < 1.0);
    assert_eq!(sim.visible.size(), region_size(Some(vp)));
    key(&mut sim, Key::Escape);
    assert_eq!(sim.state.view.panorama, None);
    assert_eq!(sim.visible.size(), PANEL_SIZE);
    key(&mut sim, Key::P);
    key(&mut sim, Key::Z);
    assert_eq!(
        sim.state.view.panorama, None,
        "Z leaves the panorama for focus"
    );
    assert!(sim.state.view.focus.is_some());
}

#[test]
fn arrows_pan_and_plus_minus_zoom_like_the_strip() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    let a = viewport(&sim);
    key(&mut sim, Key::ArrowRight);
    let b = viewport(&sim);
    assert!(b.min.x > a.min.x && b.min.y == a.min.y);
    for _ in 0..8 {
        key(&mut sim, Key::ArrowLeft);
    }
    assert_eq!(viewport(&sim).min.x, 0.0, "pan stops at the panel edge");
    key(&mut sim, Key::Plus);
    assert!(viewport(&sim).width() < a.width());
    key(&mut sim, Key::Minus);
    key(&mut sim, Key::Minus);
    assert!(viewport(&sim).width() > a.width());
}

#[test]
fn dragging_the_strip_moves_the_viewport_to_the_pointer() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    let strip = strip_rect(sim.visible);
    let end = strip.min + vec2(strip.width() * 0.8, strip.height() * 0.5);
    sim.drag(strip.center(), end);
    let want = centered(viewport(&sim), strip_to_panel(strip, end));
    let got = viewport(&sim);
    assert!(
        (got.center().x - want.center().x).abs() < 1.0,
        "{got:?} vs {want:?}"
    );
    assert!(got.center().x > PANEL_SIZE.x * 0.6);
    assert!(sim.cmds().is_empty(), "strip drags never reach the audio");
}

#[test]
fn wheel_over_the_strip_zooms_the_viewport() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    let before = viewport(&sim).width();
    let at = strip_rect(sim.visible).center();
    sim.frame(vec![Event::PointerMoved(at)], Modifiers::NONE);
    let wheel = Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 40.0),
        modifiers: Modifiers::NONE,
    };
    sim.frame(vec![wheel], Modifiers::NONE);
    sim.frame(vec![], Modifiers::NONE);
    assert!(viewport(&sim).width() < before);
}

#[test]
fn controls_inside_the_viewport_stay_clickable_and_the_strip_swallows_its_clicks() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    // Viewport at the top-left corner so the rotary speed button is on screen.
    sim.state
        .view
        .adjust_viewport(|r| centered(r, pos2(0.0, 0.0)));
    sim.frame(vec![], Modifiers::NONE);
    let _ = sim.cmds();
    sim.click("rotary.speed");
    assert!(
        sim.cmds()
            .iter()
            .any(|c| matches!(c, AudioCommand::Organ(_))),
        "a control inside the viewport must still reach the audio queue"
    );
    let strip = strip_rect(sim.visible);
    let at = strip.min + vec2(strip.width() * 0.75, STRIP_H * 0.5);
    sim.frame(vec![Event::PointerMoved(at)], Modifiers::NONE);
    sim.press(at, true, Modifiers::NONE);
    sim.press(at, false, Modifiers::NONE);
    assert!(sim.cmds().is_empty());
    assert!(viewport(&sim).center().x > PANEL_SIZE.x * 0.5);
}

#[test]
fn section_keys_focus_a_section_and_clear_the_panorama() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    key(&mut sim, Key::Num3);
    assert_eq!(sim.state.view.panorama, None);
    assert_eq!(sim.state.view.focus, Some(nord_panel::Section::Program));
}

#[test]
fn leaving_the_panorama_restores_the_default_frame() {
    let mut a = Sim::full_panel();
    let mut b = Sim::full_panel();
    key(&mut b, Key::P);
    key(&mut b, Key::P);
    for _ in 0..4 {
        a.frame(vec![], Modifiers::NONE);
    }
    b.frame(vec![], Modifiers::NONE);
    assert_eq!(format!("{:?}", a.shapes), format!("{:?}", b.shapes));
}

/// Same probe as `full_panel_frame_time_probe`, with the panorama on (strip + clipped view).
#[test]
fn panorama_frame_time_probe() {
    let mut sim = Sim::full_panel();
    key(&mut sim, Key::P);
    for _ in 0..3 {
        sim.frame(vec![], Modifiers::NONE);
    }
    let n = 40;
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        sim.frame(vec![], Modifiers::NONE);
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("PANORAMA_FRAME_MS {ms:.2}");
    assert!(ms < 500.0, "panorama frame took {ms:.1} ms");
}
