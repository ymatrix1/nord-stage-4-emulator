//! Layer A/B/C buttons (manual p.27): focus, turn on / off, SUSTPED and PSTICK.

use super::SynthUi;
use nord_panel::behaviour::{resolve, Gesture, Trigger};

/// What a Layer button press asks the bay to do with the layer's ON state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerAction {
    None,
    Activate,
    Deactivate,
}

impl SynthUi {
    /// Layer A/B/C button (manual p.27). Tap focuses the layer (and turns it on, as a mouse cannot
    /// press two buttons at once); hold turns it off; Shift+A is SUSTPED, Shift+B is PSTICK.
    pub fn layer_button(
        &mut self,
        i: usize,
        g: Gesture,
        shift_held: bool,
        active: bool,
    ) -> LayerAction {
        let i = i.min(2);
        let t = if g == Gesture::Hold {
            Trigger::Hold
        } else {
            resolve(g, shift_held, i < 2)
        };
        match t {
            Trigger::Shifted if i == 0 => {
                self.sustped = !self.sustped;
                LayerAction::None
            }
            Trigger::Shifted => {
                self.layers[1].pstick = !self.layers[1].pstick;
                LayerAction::None
            }
            Trigger::Hold if active => LayerAction::Deactivate,
            Trigger::Hold => LayerAction::None,
            _ => {
                self.focus_layer(i);
                if active {
                    LayerAction::None
                } else {
                    LayerAction::Activate
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_tap_focuses_and_activates() {
        let mut s = SynthUi::new();
        let a = s.layer_button(1, Gesture::Tap, false, false);
        assert_eq!((a, s.focus), (LayerAction::Activate, 1));
        let a = s.layer_button(2, Gesture::Tap, false, true);
        assert_eq!((a, s.focus), (LayerAction::None, 2));
    }

    #[test]
    fn layer_hold_turns_off_and_shift_selects_sustped_and_pstick() {
        let mut s = SynthUi::new();
        assert_eq!(
            s.layer_button(0, Gesture::Hold, false, true),
            LayerAction::Deactivate
        );
        assert_eq!(
            s.layer_button(0, Gesture::Hold, false, false),
            LayerAction::None
        );
        s.layer_button(0, Gesture::Tap, true, true);
        assert!(s.sustped);
        s.layer_button(1, Gesture::Tap, true, true);
        assert!(s.layers[1].pstick);
        assert_eq!(s.focus, 0, "shifted taps do not move the focus");
        // Layer C has no shifted function: a shifted tap is a plain tap.
        assert_eq!(
            s.layer_button(2, Gesture::Tap, true, true),
            LayerAction::None
        );
        assert_eq!(s.focus, 2);
    }
}
