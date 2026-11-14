//! Repeatable screenshots of the panel: `nord-stage4 --screenshot out.png
//! [--screenshot-scale N] [--screenshot-section organ|piano|program|synth|effects|master|keybed]`.
//!
//! The view is set to the section (the same focus mechanism as the Z key), the zoom is fixed at
//! `N` physical px per logical px, the window is resized to fit, and the captured frame is
//! cropped to the section. See `docs/gui-testing.md`.

use crate::panel::view::{content_size, ViewState};
use egui::{vec2, Vec2};
use nord_panel::Section;
use std::path::PathBuf;

/// Frames to wait for the zoom and window size to settle before capturing.
const SETTLE_FRAMES: u32 = 6;
/// Frames after which the capture happens regardless (window smaller than requested).
const GIVE_UP_FRAMES: u32 = 90;

/// Parse a section name (`organ`, `piano`, `program`, `synth`, `effects`, `master`, `keybed`).
pub fn parse_section(s: &str) -> Option<Section> {
    match s.to_ascii_lowercase().as_str() {
        "organ" => Some(Section::Organ),
        "piano" => Some(Section::Piano),
        "program" => Some(Section::Program),
        "synth" => Some(Section::Synth),
        "effects" | "fx" => Some(Section::Effects),
        "master" => Some(Section::Master),
        "keybed" => Some(Section::Keybed),
        _ => None,
    }
}

/// What to capture and where to write it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotPlan {
    pub target: PathBuf,
    pub section: Option<Section>,
    /// Physical px per logical panel px.
    pub scale: f32,
}

impl ShotPlan {
    /// Plan from the CLI values; scale defaults to 1 and is clamped to a sane range.
    pub fn new(target: PathBuf, section: Option<Section>, scale: Option<f32>) -> Self {
        Self {
            target,
            section,
            scale: scale.unwrap_or(1.0).clamp(0.25, 8.0),
        }
    }

    /// View state that shows the planned region at the planned zoom.
    pub fn view(&self) -> ViewState {
        ViewState {
            focus: self.section,
            last: None,
            fixed_zoom: Some(self.scale),
        }
    }

    /// Physical size of the region to capture.
    pub fn region_phys(&self) -> Vec2 {
        content_size(self.section) * self.scale
    }

    /// Window size (OS points at the native scale) that holds the region.
    pub fn window_points(&self, native_ppp: f32) -> Vec2 {
        self.region_phys() / native_ppp.max(0.1)
    }

    /// `true` when the frame may be captured: settled, or out of patience.
    pub fn ready(&self, frame: u32, window_phys: Vec2) -> bool {
        let need = self.region_phys();
        let fits = window_phys.x + 2.0 >= need.x && window_phys.y + 2.0 >= need.y;
        (frame >= SETTLE_FRAMES && fits) || frame >= GIVE_UP_FRAMES
    }

    /// Crop of the captured `image` (physical px, top-left origin) to the region, as RGBA.
    pub fn crop(&self, image: &egui::ColorImage) -> Option<image::RgbaImage> {
        let [w, h] = image.size;
        let r = self.region_phys();
        let (cw, ch) = ((r.x.round() as usize).min(w), (r.y.round() as usize).min(h));
        let mut raw = Vec::with_capacity(cw * ch * 4);
        for y in 0..ch {
            for c in &image.pixels[y * w..y * w + cw] {
                raw.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
            }
        }
        image::RgbaImage::from_raw(cw as u32, ch as u32, raw)
    }
}

/// Size to send as `ViewportCommand::InnerSize` (points) with a small slack.
pub fn inner_size_command(plan: &ShotPlan, native_ppp: f32) -> Vec2 {
    plan.window_points(native_ppp) + vec2(1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_names_parse() {
        for (n, s) in [
            ("organ", Section::Organ),
            ("Piano", Section::Piano),
            ("program", Section::Program),
            ("SYNTH", Section::Synth),
            ("effects", Section::Effects),
            ("master", Section::Master),
            ("keybed", Section::Keybed),
        ] {
            assert_eq!(parse_section(n), Some(s));
        }
        assert_eq!(parse_section("nope"), None);
    }

    #[test]
    fn scale_defaults_and_clamps() {
        let p = |s| ShotPlan::new("a.png".into(), None, s);
        assert_eq!(p(None).scale, 1.0);
        assert_eq!(p(Some(100.0)).scale, 8.0);
        assert_eq!(p(Some(2.0)).scale, 2.0);
    }

    #[test]
    fn region_scales_with_zoom_and_section() {
        let whole = ShotPlan::new("a.png".into(), None, Some(2.0));
        assert_eq!(whole.region_phys(), vec2(3120.0, 840.0));
        let organ = ShotPlan::new("a.png".into(), Some(Section::Organ), Some(2.0));
        assert!(organ.region_phys().x < whole.region_phys().x / 3.0);
        assert_eq!(organ.view().focus, Some(Section::Organ));
        assert_eq!(organ.view().fixed_zoom, Some(2.0));
        assert_eq!(organ.window_points(2.0), organ.region_phys() / 2.0);
    }

    #[test]
    fn ready_waits_for_settling_and_size_but_gives_up() {
        let p = ShotPlan::new("a.png".into(), Some(Section::Synth), Some(2.0));
        let big = p.region_phys() + vec2(10.0, 10.0);
        assert!(!p.ready(1, big));
        assert!(p.ready(SETTLE_FRAMES, big));
        assert!(!p.ready(SETTLE_FRAMES, vec2(100.0, 100.0)));
        assert!(p.ready(GIVE_UP_FRAMES, vec2(100.0, 100.0)));
    }

    #[test]
    fn crop_takes_the_top_left_region() {
        let p = ShotPlan::new("a.png".into(), None, Some(0.25));
        let r = p.region_phys();
        let (w, h) = (r.x as usize + 20, r.y as usize + 20);
        let img = egui::ColorImage::new([w, h], egui::Color32::RED);
        let out = p.crop(&img).expect("crop");
        assert_eq!((out.width(), out.height()), (390, 105));
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
        let small = egui::ColorImage::new([10, 10], egui::Color32::BLUE);
        assert_eq!(p.crop(&small).expect("crop").width(), 10);
    }

    #[test]
    fn inner_size_has_slack() {
        let p = ShotPlan::new("a.png".into(), Some(Section::Piano), Some(1.0));
        assert!(inner_size_command(&p, 1.0).x > p.region_phys().x);
    }
}
