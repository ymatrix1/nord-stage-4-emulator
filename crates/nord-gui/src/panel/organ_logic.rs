//! Pure Organ panel logic: selector tables, per-layer accessors over the panel state, clamps.
//!
//! Layer A keeps its values in the existing `Stage4PanelState` fields (so `apply_program` keeps
//! working); layer B uses `OrganUi`.

use super::prelude::*;

/// The six ORGAN MODEL LEDs in panel order (manual p.18, p.22): B3, B3 Bass, Vox, Farf, Pipe 1,
/// Pipe 2 as `(engine model, b3_bass_mode)`.
pub(super) const MODELS: [(OrganModel, bool); 6] = [
    (OrganModel::B3, false),
    (OrganModel::B3, true),
    (OrganModel::Vox, false),
    (OrganModel::Farfisa, false),
    (OrganModel::Pipe1, false),
    (OrganModel::Pipe2, false),
];

/// The six vibrato/chorus LEDs in panel order (manual p.20).
pub(super) const VIB_TYPES: [ScannerMode; 6] = [
    ScannerMode::V1,
    ScannerMode::V2,
    ScannerMode::V3,
    ScannerMode::C1,
    ScannerMode::C2,
    ScannerMode::C3,
];

/// Position of `(model, b3_bass)` in [`MODELS`]; `B3 Bass` only exists on the B3 model.
pub(super) fn model_index(model: OrganModel, b3_bass: bool) -> usize {
    MODELS
        .iter()
        .position(|&(m, b)| m == model && (b == b3_bass || model != OrganModel::B3))
        .unwrap_or(0)
}

/// Position of `mode` in [`VIB_TYPES`] (`Off` shows C3, the engine's last-used default).
pub(super) fn vib_index(mode: ScannerMode) -> usize {
    VIB_TYPES.iter().position(|&m| m == mode).unwrap_or(5)
}

/// Engine scanner mode of a layer: its type while ON, `Off` otherwise.
pub(super) fn scanner_for(mode: ScannerMode, on: bool) -> ScannerMode {
    if on {
        mode
    } else {
        ScannerMode::Off
    }
}

/// Octave shift after a press, clamped to the engine range `-2..=2`.
pub(super) fn step_octave(cur: i8, up: bool) -> i8 {
    (cur + if up { 1 } else { -1 }).clamp(-2, 2)
}

impl Stage4PanelState {
    pub(super) fn organ_layer_active(&self, layer: usize) -> bool {
        if layer == 0 {
            self.organ_a_active
        } else {
            self.organ_b_active
        }
    }

    /// Section ON LED: lit while any organ layer is active.
    pub(super) fn organ_section_on(&self) -> bool {
        self.organ_a_active || self.organ_b_active
    }

    /// ON tap (manual p.18): switch the whole Organ section off, or bring back the layers it
    /// had. Returns which layers changed so the caller can send their enable state.
    pub(super) fn organ_on_tap(&mut self) -> [bool; 2] {
        let before = [self.organ_a_active, self.organ_b_active];
        if self.organ_section_on() {
            self.organ_ui.restore_layers = u8::from(before[0]) | (u8::from(before[1]) << 1);
            self.organ_a_active = false;
            self.organ_b_active = false;
        } else {
            let r = self.organ_ui.restore_layers & 0b11;
            let r = if r == 0 { 0b01 } else { r };
            self.organ_a_active = r & 1 != 0;
            self.organ_b_active = r & 2 != 0;
            self.organ_ui.focus_layer = r.trailing_zeros() as usize;
        }
        [
            before[0] != self.organ_a_active,
            before[1] != self.organ_b_active,
        ]
    }

    pub(super) fn organ_layer_level(&self, layer: usize) -> f32 {
        if layer == 0 {
            self.organ_a_level
        } else {
            self.organ_b_level
        }
    }

    pub(super) fn organ_octave_of(&self, layer: usize) -> i8 {
        if layer == 0 {
            self.organ_octave
        } else {
            self.organ_ui.octave_b
        }
    }

    pub(super) fn set_organ_octave(&mut self, layer: usize, v: i8) {
        if layer == 0 {
            self.organ_octave = v;
        } else {
            self.organ_ui.octave_b = v;
        }
    }

    /// `(type, ON)` of a layer's vibrato/chorus.
    pub(super) fn organ_vib_of(&self, layer: usize) -> (ScannerMode, bool) {
        if layer == 0 {
            (self.scanner_mode, self.scanner_on)
        } else {
            (self.organ_ui.vib_type_b, self.organ_ui.vib_on_b)
        }
    }

    pub(super) fn set_organ_vib(&mut self, layer: usize, mode: ScannerMode, on: bool) {
        if layer == 0 {
            self.scanner_mode = mode;
            self.scanner_on = on;
        } else {
            self.organ_ui.vib_type_b = mode;
            self.organ_ui.vib_on_b = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_table_round_trips() {
        for (i, &(m, b)) in MODELS.iter().enumerate() {
            assert_eq!(model_index(m, b), i);
        }
        // Bass flag is ignored off the B3 model.
        assert_eq!(model_index(OrganModel::Vox, true), 2);
    }

    #[test]
    fn vib_index_covers_all_types() {
        for (i, &m) in VIB_TYPES.iter().enumerate() {
            assert_eq!(vib_index(m), i);
        }
        assert_eq!(vib_index(ScannerMode::Off), 5);
        assert_eq!(scanner_for(ScannerMode::V2, false), ScannerMode::Off);
        assert_eq!(scanner_for(ScannerMode::V2, true), ScannerMode::V2);
    }

    #[test]
    fn steps_clamp() {
        assert_eq!(step_octave(2, true), 2);
        assert_eq!(step_octave(-2, false), -2);
        assert_eq!(step_octave(0, true), 1);
    }

    #[test]
    fn layer_accessors_split_a_and_b() {
        let mut s = Stage4PanelState::default();
        s.set_organ_octave(0, 1);
        s.set_organ_octave(1, -2);
        assert_eq!((s.organ_octave_of(0), s.organ_octave_of(1)), (1, -2));
        s.set_organ_vib(1, ScannerMode::V3, true);
        assert_eq!(s.organ_vib_of(1), (ScannerMode::V3, true));
        assert_eq!(s.organ_vib_of(0), (ScannerMode::C3, false));
        assert!(s.organ_layer_active(0) && !s.organ_layer_active(1));
        assert!(s.organ_layer_level(0) > s.organ_layer_level(1));
    }

    #[test]
    fn on_tap_switches_the_section_off_and_restores_the_same_layers() {
        let mut s = Stage4PanelState {
            organ_b_active: true,
            ..Default::default()
        };
        assert!(s.organ_section_on());
        assert_eq!(s.organ_on_tap(), [true, true]);
        assert!(!s.organ_section_on());
        assert_eq!(s.organ_on_tap(), [true, true]);
        assert!(s.organ_a_active && s.organ_b_active);
        s.organ_a_active = false;
        assert_eq!(s.organ_on_tap(), [false, true]);
        assert_eq!(s.organ_on_tap(), [false, true]);
        assert!(!s.organ_a_active && s.organ_b_active);
        assert_eq!(s.organ_ui.focus_layer, 1);
    }

    #[test]
    fn on_tap_with_nothing_remembered_brings_back_layer_a() {
        let mut s = Stage4PanelState {
            organ_a_active: false,
            ..Default::default()
        };
        s.organ_ui.restore_layers = 0;
        assert_eq!(s.organ_on_tap(), [true, false]);
        assert!(s.organ_a_active);
    }
}
