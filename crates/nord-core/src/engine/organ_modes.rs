//! Organ layer sharing rules and the Preset / Drawbar Live SYNC of `Stage4Engine`.
//!
//! Manual p.20-21 (OS v1.6x):
//! - Vibrato/Chorus is switched on per layer ("Layer A and Layer B manuals") for the B3 and Pipe
//!   models, but "the Vox model's Vibrato setting and On/Off state is shared by Layer A and Layer
//!   B" and "the Farf vibrato is common for both Layers".
//! - SYNC (Shift+Preset) copies the physical drawbar positions into the Preset.
//!
//! Defaults are off (`organ_shared_vibrato == false`), so an untouched engine renders as before.

use super::*;
use crate::organ::OrganModel;

/// `true` for the models whose vibrato setting and on/off state are common to both layers.
pub fn model_shares_vibrato(model: OrganModel) -> bool {
    matches!(model, OrganModel::Vox | OrganModel::Farfisa)
}

impl Stage4Engine {
    /// Applies the shared Vox / Farf vibrato rule when `organ_shared_vibrato` is on: if both layers
    /// use the same sharing model, Organ B takes Organ A's vibrato/chorus setting.
    ///
    /// ASSUMED: layer A is the source (the manual says "shared" without naming a source; the panel
    /// edits the focused layer, which the engine does not track). A hardware check would confirm
    /// which layer's value wins after a Program load with different stored values.
    /// Field copy only: allocation free; `process_block` calls it.
    pub fn apply_organ_sharing(&mut self) {
        if self.organ_shared_vibrato
            && self.organ_a.model == self.organ_b.model
            && model_shares_vibrato(self.organ_a.model)
        {
            self.organ_b.scanner_mode = self.organ_a.scanner_mode;
        }
    }

    /// SYNC (manual p.21): copies the physical drawbar positions of `layer` (`OrganA` / `OrganB`)
    /// into its stored Preset. Other layer ids are ignored.
    pub fn sync_organ_preset(&mut self, layer: LayerId) {
        match layer {
            LayerId::OrganA => self.organ_a.sync_preset_to_live(),
            LayerId::OrganB => self.organ_b.sync_preset_to_live(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organ::ScannerMode;

    #[test]
    fn sharing_is_off_by_default() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.model = OrganModel::Vox;
        e.organ_b.model = OrganModel::Vox;
        e.organ_a.scanner_mode = ScannerMode::V3;
        e.apply_organ_sharing();
        assert_eq!(e.organ_b.scanner_mode, ScannerMode::Off);
    }

    #[test]
    fn vox_and_farf_layers_share_the_vibrato_of_layer_a() {
        for model in [OrganModel::Vox, OrganModel::Farfisa] {
            let mut e = Stage4Engine::new(48_000.0);
            e.organ_shared_vibrato = true;
            e.organ_a.model = model;
            e.organ_b.model = model;
            e.organ_a.scanner_mode = ScannerMode::C2;
            e.organ_b.scanner_mode = ScannerMode::V1;
            e.apply_organ_sharing();
            assert_eq!(e.organ_b.scanner_mode, ScannerMode::C2, "{model:?}");
            e.organ_a.scanner_mode = ScannerMode::Off;
            e.apply_organ_sharing();
            assert_eq!(e.organ_b.scanner_mode, ScannerMode::Off);
        }
    }

    #[test]
    fn b3_pipe_and_mixed_models_keep_per_layer_vibrato() {
        for (a, b) in [
            (OrganModel::B3, OrganModel::B3),
            (OrganModel::Pipe1, OrganModel::Pipe1),
            (OrganModel::Vox, OrganModel::Farfisa),
            (OrganModel::Vox, OrganModel::B3),
        ] {
            let mut e = Stage4Engine::new(48_000.0);
            e.organ_shared_vibrato = true;
            e.organ_a.model = a;
            e.organ_b.model = b;
            e.organ_a.scanner_mode = ScannerMode::C2;
            e.organ_b.scanner_mode = ScannerMode::V1;
            e.apply_organ_sharing();
            assert_eq!(e.organ_b.scanner_mode, ScannerMode::V1, "{a:?}/{b:?}");
        }
    }

    #[test]
    fn process_block_applies_the_sharing() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_shared_vibrato = true;
        e.organ_a.model = OrganModel::Vox;
        e.organ_b.model = OrganModel::Vox;
        e.organ_a.scanner_mode = ScannerMode::V3;
        let (mut l, mut r) = ([0.0f32; 16], [0.0f32; 16]);
        e.process_block(&mut l, &mut r);
        assert_eq!(e.organ_b.scanner_mode, ScannerMode::V3);
    }

    #[test]
    fn sync_copies_the_physical_drawbars_into_the_chosen_layer_only() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.live_drawbars = [1, 2, 3, 4, 5, 6, 7, 8, 0];
        e.organ_b.live_drawbars = [8; 9];
        e.sync_organ_preset(LayerId::OrganA);
        assert_eq!(e.organ_a.drawbars, [1, 2, 3, 4, 5, 6, 7, 8, 0]);
        assert_eq!(e.organ_b.drawbars, [8, 8, 8, 0, 0, 0, 0, 0, 0]);
        e.sync_organ_preset(LayerId::PianoA);
        assert_eq!(e.organ_b.drawbars, [8, 8, 8, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn drawbar_cc_tracks_the_physical_bars_and_edits_the_preset_only_when_not_live() {
        let mut e = Stage4Engine::new(48_000.0);
        e.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::DRAWBAR_1_16,
            value: 0,
        });
        assert_eq!(e.organ_a.live_drawbars[0], 0);
        assert_eq!(e.organ_a.drawbars[0], 0);
        e.organ_a.drawbar_live = true;
        e.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::DRAWBAR_1_16,
            value: 127,
        });
        assert_eq!(e.organ_a.live_drawbars[0], 8);
        assert_eq!(
            e.organ_a.drawbars[0], 0,
            "preset must not follow in Live mode"
        );
    }
}
