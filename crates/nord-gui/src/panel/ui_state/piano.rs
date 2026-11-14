//! UI-only state of the Piano area (widgets that have no engine field of their own).
//!
//! Engine-visible values (category, timbre, touch, string res, pedal noise, layer level and
//! active flags) stay in `Stage4PanelState`; per-layer extras and interaction state live here.

use nord_core::piano::PianoCategory;
use nord_panel::behaviour::ButtonTracker;
use std::sync::OnceLock;

static UNSUPPORTED: OnceLock<Vec<String>> = OnceLock::new();

/// The inventory still lists `id` as `Unsupported` (no engine feature behind it).
pub fn inventory_unsupported(id: &str) -> bool {
    UNSUPPORTED
        .get_or_init(|| {
            nord_panel::all_controls()
                .iter()
                .filter(|c| matches!(c.binding, nord_panel::Binding::Unsupported(_)))
                .map(|c| c.id.clone())
                .collect()
        })
        .iter()
        .any(|x| x == id)
}

/// Number of piano layers (A, B).
pub const LAYERS: usize = 2;

/// Gesture trackers, one per button that distinguishes tap / hold.
pub mod slot {
    pub const ON: usize = 0;
    pub const LAYER_A: usize = 1;
    pub const LAYER_B: usize = 2;
    pub const OCT_DOWN: usize = 3;
    pub const OCT_UP: usize = 4;
    pub const TYPE: usize = 5;
    pub const SOFT_RELEASE: usize = 6;
    pub const STRING_RES: usize = 7;
    pub const KB_TOUCH: usize = 8;
    pub const DYN_COMP: usize = 9;
    pub const TIMBRE: usize = 10;
    pub const UNISON: usize = 11;
    pub const DIAL: usize = 12;
    pub const COUNT: usize = 13;
}

/// Which model overlay (small screen over the bay) is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PianoOverlay {
    #[default]
    None,
    /// LIST view (Shift + PIANO SELECT dial, manual p.25).
    List,
    /// INFO view (Shift + TYPE, manual p.25).
    Info,
}

/// UI state for the Piano area.
#[derive(Debug, Clone)]
pub struct PianoUi {
    /// Focused layer (0 = A, 1 = B); the per-layer buttons act on it.
    pub focus: usize,
    /// SOFT RELEASE per layer.
    pub soft_release: [bool; LAYERS],
    /// UNISON 0 (off) ..= 3 per layer.
    pub unison: [u8; LAYERS],
    /// DYN COMP 0 (off) ..= 3 per layer.
    pub dyn_comp: [u8; LAYERS],
    /// OCTAVE SHIFT -2..=2 per layer.
    pub octave: [i8; LAYERS],
    /// KB ZONE position per layer (`kb_zone::POSITIONS`, `0` = every zone, the engine default).
    pub kb_zone: [u8; LAYERS],
    /// Aux KB flag per layer (the LED); the UI value until telemetry reads it back.
    pub aux: [bool; LAYERS],
    /// SUSTPED (Shift + Layer A); engine default is on for the pianos.
    pub sustped: bool,
    /// PSTICK (Shift + Layer B); engine default is off.
    pub pstick: bool,
    /// Clav TIMBRE rocker bits (1 soft, 2 treble, 4 brilliant; 0 off). UI only.
    pub clav_timbre: u8,
    /// Layer bit mask to bring back when ON is pressed again.
    pub restore_layers: u8,
    /// Scene mask saved while SOLO is held.
    pub solo_saved: Option<u8>,
    /// On-panel SHIFT latch (the keyboard Shift key works too).
    pub shift_latch: bool,
    /// Model names of the loaded library (kept in sync by `library/piano_bridge.rs`).
    pub models: Vec<String>,
    /// Piano type of each entry of `models` (parallel). Empty or shorter than `models` for a flat
    /// list (dropped files): then the TYPE button does not filter the models.
    pub model_types: Vec<PianoCategory>,
    /// Index into `models` of the selected model.
    pub model_idx: usize,
    /// Last model used per type (index into `models`), for TYPE to come back to it.
    pub last_model: [Option<usize>; 6],
    /// Loaded Piano A name the dial last followed (so a pending load does not snap it back).
    pub followed: Option<String>,
    /// Model the dial asked for; the library takes it each frame.
    pub model_request: Option<String>,
    /// Open overlay.
    pub overlay: PianoOverlay,
    /// Gesture trackers, indexed by [`slot`].
    pub trackers: [ButtonTracker; slot::COUNT],
    /// Fractional dial drag accumulated between steps (px).
    pub dial_accum: f32,
    /// Ids painted in the last frame (headless tests check completeness).
    pub drawn: Vec<&'static str>,
    /// Ids drawn dimmed in the last frame and why.
    pub dimmed: Vec<(&'static str, &'static str)>,
}

impl Default for PianoUi {
    fn default() -> Self {
        Self {
            focus: 0,
            soft_release: [false; LAYERS],
            unison: [0; LAYERS],
            dyn_comp: [0; LAYERS],
            octave: [0; LAYERS],
            kb_zone: [0; LAYERS],
            aux: [false; LAYERS],
            sustped: true,
            pstick: false,
            clav_timbre: 0,
            restore_layers: 0b01,
            solo_saved: None,
            shift_latch: false,
            models: Vec::new(),
            model_types: Vec::new(),
            model_idx: 0,
            last_model: [None; 6],
            followed: None,
            model_request: None,
            overlay: PianoOverlay::None,
            trackers: Default::default(),
            dial_accum: 0.0,
            drawn: Vec::new(),
            dimmed: Vec::new(),
        }
    }
}

impl PianoUi {
    /// Start of a frame: forget which ids were painted.
    pub fn begin_frame(&mut self) {
        self.drawn.clear();
        self.dimmed.clear();
    }

    /// Record that the widget of inventory id `id` was painted.
    pub fn mark(&mut self, id: &'static str) {
        self.drawn.push(id);
    }

    /// Record that `id` was painted and is dimmed because of `reason`.
    pub fn mark_dim(&mut self, id: &'static str, reason: &'static str) {
        self.drawn.push(id);
        self.dimmed.push((id, reason));
    }

    /// Like [`mark`](Self::mark), but dims the id when the inventory still lists it as
    /// `Unsupported`; returns whether it is dimmed.
    pub fn mark_inv(&mut self, id: &'static str, reason: &'static str) -> bool {
        if inventory_unsupported(id) {
            self.mark_dim(id, reason);
            true
        } else {
            self.mark(id);
            false
        }
    }

    /// The layer index (0 or 1) the per-layer buttons act on.
    pub fn layer(&self) -> usize {
        self.focus.min(LAYERS - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_focus_layer_a_with_everything_off() {
        let u = PianoUi::default();
        assert_eq!(u.layer(), 0);
        assert_eq!(u.unison, [0, 0]);
        assert!(u.models.is_empty() && u.model_request.is_none());
        assert_eq!(u.overlay, PianoOverlay::None);
        assert_eq!(u.trackers.len(), slot::COUNT);
    }

    #[test]
    fn mark_and_begin_frame_track_ids() {
        let mut u = PianoUi::default();
        u.mark("piano.on");
        u.mark_dim("piano.sustped", "no engine support");
        assert_eq!(u.drawn, ["piano.on", "piano.sustped"]);
        assert_eq!(u.dimmed.len(), 1);
        u.begin_frame();
        assert!(u.drawn.is_empty() && u.dimmed.is_empty());
    }

    #[test]
    fn inventory_lookup_matches_the_binding() {
        assert!(!inventory_unsupported("piano.kb_zone_up"));
        assert!(!inventory_unsupported("piano.select_dial"));
        assert!(!inventory_unsupported("piano.sustped"));
        assert!(!inventory_unsupported("piano.unison"));
        let mut u = PianoUi::default();
        assert!(!u.mark_inv("piano.select_dial", "r"));
        assert!(!u.mark_inv("piano.zone_leds", "r"));
        assert!(!u.mark_inv("piano.unison", "r"));
    }

    #[test]
    fn focus_is_clamped_to_a_valid_layer() {
        let u = PianoUi {
            focus: 9,
            ..PianoUi::default()
        };
        assert_eq!(u.layer(), 1);
    }
}
