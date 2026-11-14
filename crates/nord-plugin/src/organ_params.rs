//! Per-layer Organ parameters the panel exposes but the host lacked (Phase 6b
//! B9c, `ns4-h7he`; manual p.18-22, p.53): B3 Bass, Octave Shift, Vibrato /
//! Chorus type, plus the Rotary CLOSE MIC and STOP MODE switches.
//!
//! IDs are additive: `organx_<name>_1` (Organ A), `_2` (Organ B), and
//! `organx_close_mic` / `organx_stop_mode` for the single Rotary Speaker.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::organ::ScannerMode;

/// Number of organ layers (A and B).
pub const ORGAN_LAYERS: usize = 2;

/// Parameters of one organ layer.
#[derive(Params)]
pub struct OrganLayerParams {
    /// B3 Bass pickup calibration (manual p.22); only audible on the B3 model.
    #[id = "b3_bass"]
    pub b3_bass: BoolParam,
    /// Octave shift in octaves, `-2..=2` (engine range; manual p.18).
    #[id = "octave"]
    pub octave: IntParam,
    /// Vibrato/Chorus: 0 = Off, 1..=3 = V1..V3, 4..=6 = C1..C3 (manual p.20).
    #[id = "scanner"]
    pub scanner: IntParam,
}

impl Default for OrganLayerParams {
    fn default() -> Self {
        Self {
            b3_bass: BoolParam::new("B3 Bass", false),
            octave: IntParam::new("Octave Shift", 0, IntRange::Linear { min: -2, max: 2 }),
            scanner: IntParam::new("Vibrato/Chorus", 0, IntRange::Linear { min: 0, max: 6 }),
        }
    }
}

/// Both organ layers plus the shared Rotary switches.
#[derive(Params)]
pub struct OrganLayerBank {
    #[nested(array, group = "Organ Layer")]
    pub layers: [OrganLayerParams; ORGAN_LAYERS],
    /// Rotary Close Mic (manual p.53), stored per program.
    #[id = "close_mic"]
    pub close_mic: BoolParam,
    /// Rotary Stop Mode (manual p.53).
    #[id = "stop_mode"]
    pub stop_mode: BoolParam,
}

impl Default for OrganLayerBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| OrganLayerParams::default()),
            close_mic: BoolParam::new("Rotary Close Mic", false),
            stop_mode: BoolParam::new("Rotary Stop Mode", false),
        }
    }
}

/// Map the host scanner index to the engine mode.
pub fn scanner_from_index(i: i32) -> ScannerMode {
    match i {
        1 => ScannerMode::V1,
        2 => ScannerMode::V2,
        3 => ScannerMode::V3,
        4 => ScannerMode::C1,
        5 => ScannerMode::C2,
        6 => ScannerMode::C3,
        _ => ScannerMode::Off,
    }
}

impl OrganLayerBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [&mut engine.organ_a, &mut engine.organ_b];
        for (p, t) in self.layers.iter().zip(targets) {
            t.b3_bass_mode = p.b3_bass.value();
            t.octave_shift = p.octave.value() as i8;
            t.scanner_mode = scanner_from_index(p.scanner.value());
            t.rotary_close_mic = self.close_mic.value();
            t.rotary_stop_mode = self.stop_mode.value();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_index_maps_all_seven_modes() {
        let all = [
            ScannerMode::Off,
            ScannerMode::V1,
            ScannerMode::V2,
            ScannerMode::V3,
            ScannerMode::C1,
            ScannerMode::C2,
            ScannerMode::C3,
        ];
        for (i, m) in all.iter().enumerate() {
            assert_eq!(scanner_from_index(i as i32), *m);
        }
        assert_eq!(scanner_from_index(99), ScannerMode::Off);
    }

    #[test]
    fn defaults_leave_the_engine_unchanged() {
        let mut engine = Stage4Engine::new(44100.0);
        OrganLayerBank::default().sync_to_engine(&mut engine);
        assert!(!engine.organ_a.b3_bass_mode);
        assert_eq!(engine.organ_b.octave_shift, 0);
        assert_eq!(engine.organ_a.scanner_mode, ScannerMode::Off);
        assert!(!engine.organ_a.rotary_close_mic && !engine.organ_a.rotary_stop_mode);
    }

    #[test]
    fn values_reach_the_matching_layer_and_rotary_reaches_both() {
        let mut bank = OrganLayerBank::default();
        bank.layers[1].b3_bass = BoolParam::new("b", true);
        bank.layers[1].octave = IntParam::new("o", 1, IntRange::Linear { min: -2, max: 2 });
        bank.layers[1].scanner = IntParam::new("s", 6, IntRange::Linear { min: 0, max: 6 });
        bank.close_mic = BoolParam::new("c", true);
        bank.stop_mode = BoolParam::new("m", true);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert!(engine.organ_b.b3_bass_mode && !engine.organ_a.b3_bass_mode);
        assert_eq!(engine.organ_b.octave_shift, 1);
        assert_eq!(engine.organ_b.scanner_mode, ScannerMode::C3);
        assert_eq!(engine.organ_a.scanner_mode, ScannerMode::Off);
        assert!(engine.organ_a.rotary_close_mic && engine.organ_b.rotary_close_mic);
        assert!(engine.organ_a.rotary_stop_mode && engine.organ_b.rotary_stop_mode);
    }

    #[test]
    fn scanner_vibrato_changes_rendered_organ_audio() {
        let render = |idx: i32| {
            let mut bank = OrganLayerBank::default();
            bank.layers[0].scanner = IntParam::new("s", idx, IntRange::Linear { min: 0, max: 6 });
            let mut engine = Stage4Engine::new(44100.0);
            bank.sync_to_engine(&mut engine);
            engine.organ_a.note_on(60);
            (0..8192)
                .map(|_| engine.organ_a.render_frame())
                .collect::<Vec<_>>()
        };
        let (off, c3) = (render(0), render(6));
        assert!(off
            .iter()
            .chain(&c3)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(off.iter().any(|f| f[0].abs() > 1e-4), "organ is silent");
        assert!(off != c3, "scanner C3 renders identically to Off");
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let ids: Vec<String> = OrganLayerBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 3 * ORGAN_LAYERS + 2);
        assert!(ids.contains(&"b3_bass_1".to_string()));
        assert!(ids.contains(&"close_mic".to_string()));
    }
}
