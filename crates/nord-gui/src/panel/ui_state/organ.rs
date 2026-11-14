//! UI-only state of the Organ area (widgets that have no engine field of their own) and of the
//! Master/Rotary box, which shares the behaviour trackers.
//!
//! `Stage4PanelState::organ_ui` holds the struct. Per-layer arrays are indexed `0 = A`, `1 = B`.

use crate::panel::ctl::Trackers;
use nord_core::organ::ScannerMode;

/// UI state for the Organ area and the Rotary box.
#[derive(Debug, Clone)]
pub struct OrganUi {
    /// Layer the strip buttons (OCTAVE, KB ZONE, VIB type, ...) act on; its LED blinks.
    pub focus_layer: usize,
    /// Layer bit mask (bit 0 = A) to bring back when ON is pressed again (`organ.on`).
    pub restore_layers: u8,
    /// Temporary SOLO monitor mode (manual p.43); never stored in a program.
    pub solo: bool,
    /// Drawbar Live: ladders are unlit, the physical drawbars play (manual p.21).
    pub drawbar_live: bool,
    /// KB ZONE position per layer (`kb_zone::POSITIONS`, `0` = every zone, the engine default).
    pub kb_zone: [u8; 2],
    /// Aux KB flag per layer (the LED); the UI value until telemetry reads it back.
    pub aux: [bool; 2],
    /// Physical drawbar positions (the sliders). They follow the stored preset while Preset mode
    /// is on and move alone in Drawbar Live (manual p.21).
    pub live_bars: [u8; 9],
    /// SUSTPED per layer (Shift function of the layer buttons).
    pub sustped: [bool; 2],
    /// PSTICK per layer.
    pub pstick: [bool; 2],
    /// POLY (Shift + percussion volume).
    pub perc_poly: bool,
    /// B3 Bass flavour of the model selector (the sixth model LED).
    pub b3_bass: bool,
    /// Octave shift of layer B (layer A lives in `Stage4PanelState::organ_octave`).
    pub octave_b: i8,
    /// Vibrato/chorus type of layer B (layer A lives in `Stage4PanelState::scanner_mode`).
    pub vib_type_b: ScannerMode,
    /// Vibrato/chorus ON of layer B (layer A lives in `Stage4PanelState::scanner_on`).
    pub vib_on_b: bool,
    /// Rotary CLOSE MIC (Shift function of the ORGAN routing button).
    pub close_mic: bool,
    /// Rotary ANGLE wide/narrow (Shift function of STOP MODE); wide is the engine default.
    pub rotary_angle_wide: bool,
    /// Rotary ORGAN routing button: the organ goes through the rotary speaker (engine default on).
    pub rotary_organ: bool,
    /// Per-button gesture state machines, keyed by control id.
    pub trackers: Trackers,
}

impl Default for OrganUi {
    fn default() -> Self {
        Self {
            focus_layer: 0,
            restore_layers: 0b01,
            solo: false,
            drawbar_live: false,
            kb_zone: [0; 2],
            aux: [false; 2],
            live_bars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            sustped: [false; 2],
            pstick: [false; 2],
            perc_poly: false,
            b3_bass: false,
            octave_b: 0,
            vib_type_b: ScannerMode::C3,
            vib_on_b: false,
            close_mic: false,
            rotary_angle_wide: true,
            rotary_organ: true,
            trackers: Trackers::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_starts_on_layer_a_in_preset_mode() {
        let u = OrganUi::default();
        assert_eq!(u.focus_layer, 0);
        assert!(!u.solo && !u.drawbar_live);
        assert_eq!(u.restore_layers, 0b01);
        assert!(u.rotary_angle_wide && u.rotary_organ, "engine defaults");
        assert_eq!(u.kb_zone, [0, 0]);
        assert_eq!(u.vib_type_b, ScannerMode::C3);
        assert!(u.trackers.is_empty());
    }
}
