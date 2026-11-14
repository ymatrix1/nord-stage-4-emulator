//! The Octave Shift popup: shown on the home screen for a moment after an octave button moved the
//! focused Synth layer's octave.
//!
//! VERIFIED (ARM frame, research note s17.4): home header kept, label `Octave Shift` (y = 45), value
//! `-1` at the left (y = 55) and a slider graphic at the right edge. ASSUMED: it stays up for
//! [`SHOW_MS`] (the firmware timeout was not measured), and the value is printed without a `+`.

use crate::panel::ui_state::ProgramUi;

/// How long the popup stays after the last octave change.
///
/// ASSUMED: 1.5 s, chosen like the other transient messages of the display; confirm against a
/// frame sequence of the unit.
pub const SHOW_MS: u32 = 1500;

/// Octave popup state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Popups {
    /// Layer and octave seen on the last frame (changes of the same layer open the popup).
    seen: Option<(usize, i8)>,
    /// Value shown and the time it goes away.
    shown: Option<(i8, u32)>,
}

impl Popups {
    /// Called once per frame with the focused Synth layer and its octave shift.
    pub fn watch_octave(&mut self, layer: usize, octave: i8, now_ms: u32) {
        if let Some((l, o)) = self.seen {
            if l == layer && o != octave {
                self.shown = Some((octave, now_ms.saturating_add(SHOW_MS)));
            }
        }
        self.seen = Some((layer, octave));
    }

    /// Octave value while the popup is up at `now_ms`.
    pub fn octave(&self, now_ms: u32) -> Option<i8> {
        self.shown
            .filter(|&(_, until)| now_ms < until)
            .map(|(v, _)| v)
    }

    /// The popup needs a repaint to go away.
    pub fn is_showing(&self, now_ms: u32) -> bool {
        self.octave(now_ms).is_some()
    }
}

impl ProgramUi {
    /// The `(label, value)` of the Octave Shift popup when it is up on the home screen.
    pub fn octave_popup(&self) -> Option<(String, i8)> {
        self.popups
            .octave(self.now_ms)
            .map(|v| ("Octave Shift".to_string(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_of_the_same_layer_opens_the_popup_and_it_times_out() {
        let mut p = Popups::default();
        p.watch_octave(0, 0, 100);
        assert_eq!(p.octave(100), None, "the first sight is no change");
        p.watch_octave(0, -1, 200);
        assert_eq!(p.octave(300), Some(-1));
        assert!(p.is_showing(300));
        assert_eq!(p.octave(200 + SHOW_MS), None);
        p.watch_octave(0, 0, 3000);
        assert_eq!(p.octave(3001), Some(0), "the next press shows 0");
    }

    #[test]
    fn switching_layers_is_not_an_octave_press() {
        let mut p = Popups::default();
        p.watch_octave(0, 0, 0);
        p.watch_octave(1, 2, 10);
        assert_eq!(p.octave(20), None);
        p.watch_octave(1, 2, 30);
        assert_eq!(p.octave(40), None);
    }

    #[test]
    fn the_program_area_names_the_popup() {
        let mut u = ProgramUi::default();
        u.now_ms = 10;
        u.popups.watch_octave(0, 0, 10);
        u.popups.watch_octave(0, 1, 11);
        assert_eq!(u.octave_popup(), Some(("Octave Shift".into(), 1)));
        u.now_ms = 5000;
        assert_eq!(u.octave_popup(), None);
    }
}
