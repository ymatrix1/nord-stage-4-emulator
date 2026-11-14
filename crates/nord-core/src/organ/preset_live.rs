//! PRESET vs DRAWBAR LIVE model of one organ layer.
//!
//! Manual p.21 (OS v1.6x): by default PRESET is on and the values stored in the Program are used
//! for the drawbars, Percussion and Chorus/Vibrato On/Off. With PRESET off the organ is in
//! Drawbar Live mode and uses the actual positions of the physical drawbars (LEDs unlit). SYNC
//! (Shift+Preset, or press-hold PRESET) synchronizes the Preset settings with the physical drawbar
//! positions. Morph assignments act on the stored drawbar values, so they do not apply in Live
//! mode (ASSUMED reading of the manual's "Preset values are used").
//!
//! Scope: this module models the drawbar part (`drawbars` = stored preset, `live_drawbars` =
//! physical). Percussion and vibrato on/off keep one stored value (live buttons are not modelled,
//! see `docs/specs/organ-engine.md`).

use super::OrganLayer;

impl OrganLayer {
    /// Drawbar positions in force: the physical ones in Live mode, else the stored preset.
    pub fn active_drawbars(&self) -> &[u8; 9] {
        if self.drawbar_live {
            &self.live_drawbars
        } else {
            &self.drawbars
        }
    }

    /// Moves physical drawbar `index` (`0..9`) to `value` (clamped to `0..=8`).
    pub fn set_live_drawbar(&mut self, index: usize, value: u8) {
        if let Some(slot) = self.live_drawbars.get_mut(index) {
            *slot = value.min(8);
        }
    }

    /// SYNC: copies the physical drawbar positions into the stored preset.
    pub fn sync_preset_to_live(&mut self) {
        self.drawbars = self.live_drawbars;
    }

    /// `true` when drawbar morph assignments act (Preset mode only).
    pub fn drawbar_morph_applicable(&self) -> bool {
        !self.drawbar_live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(o: &mut OrganLayer, frames: usize) -> Vec<f32> {
        o.rotary_enabled = false;
        o.key_click_enabled = false;
        o.percussion_enabled = false;
        (0..frames).map(|_| o.render_frame()[0]).collect()
    }

    #[test]
    fn preset_mode_ignores_the_live_positions() {
        let mut a = OrganLayer::new(48_000.0);
        let mut b = OrganLayer::new(48_000.0);
        b.live_drawbars = [0, 0, 0, 0, 0, 0, 0, 0, 8];
        a.note_on(60);
        b.note_on(60);
        assert_eq!(render(&mut a, 2000), render(&mut b, 2000));
        assert_eq!(b.active_drawbars(), &b.drawbars);
    }

    #[test]
    fn live_mode_uses_the_physical_positions() {
        let mut preset = OrganLayer::new(48_000.0);
        let mut live = OrganLayer::new(48_000.0);
        live.drawbar_live = true;
        live.live_drawbars = [0, 0, 0, 0, 8, 0, 0, 0, 0];
        preset.note_on(60);
        live.note_on(60);
        let (p, l) = (render(&mut preset, 2000), render(&mut live, 2000));
        assert!(p != l);
        assert!(l.iter().any(|v| v.abs() > 1e-4) && l.iter().all(|v| v.is_finite()));
        assert_eq!(live.active_drawbars(), &live.live_drawbars);
        assert!(!live.drawbar_morph_applicable() && preset.drawbar_morph_applicable());
    }

    #[test]
    fn live_equal_to_the_preset_renders_identically() {
        let mut preset = OrganLayer::new(48_000.0);
        let mut live = OrganLayer::new(48_000.0);
        preset.drawbars = [0, 8, 0, 4, 0, 0, 2, 0, 0];
        live.live_drawbars = preset.drawbars;
        live.drawbar_live = true;
        preset.note_on(55);
        live.note_on(55);
        assert_eq!(render(&mut preset, 2000), render(&mut live, 2000));
    }

    #[test]
    fn sync_copies_live_into_the_preset_and_set_clamps() {
        let mut o = OrganLayer::new(48_000.0);
        o.set_live_drawbar(3, 200);
        o.set_live_drawbar(99, 5);
        assert_eq!(o.live_drawbars[3], 8);
        o.sync_preset_to_live();
        assert_eq!(o.drawbars, o.live_drawbars);
        assert_eq!(o.drawbars[3], 8);
    }

    #[test]
    fn live_mode_works_for_b3_bass_and_firmware_weights() {
        use super::super::{OrganModel, WeightMode};
        for (bass, weights) in [(true, WeightMode::Legacy), (false, WeightMode::Firmware)] {
            let mut o = OrganLayer::new(48_000.0);
            o.model = OrganModel::B3;
            o.b3_bass_mode = bass;
            o.weight_mode = weights;
            o.drawbar_live = true;
            o.live_drawbars = [8, 8, 0, 0, 0, 0, 0, 0, 0];
            o.note_on(36);
            let out = render(&mut o, 3000);
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(out.iter().any(|v| v.abs() > 1e-4), "bass {bass}: silent");
        }
    }
}
