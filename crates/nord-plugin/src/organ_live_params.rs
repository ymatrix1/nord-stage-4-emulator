//! POLY percussion, Preset / Drawbar Live and shared Vox/Farf vibrato (manual p.20-21).
//!
//! IDs are additive: `organz_<name>_1` (Organ A), `_2` (Organ B) and the single
//! `organz_shared_vibrato`. Defaults equal the engine defaults, so a fresh instance renders as
//! before. `organz_live1..9_N` are the physical drawbar positions used while
//! `organz_drawbar_live_N` is on; `organz_sync_N` is a momentary SYNC trigger (Shift+Preset): while
//! it is on, the physical positions are copied into the stored preset on every sync.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;

/// Number of organ layers (A and B).
pub const ORGAN_LIVE_LAYERS: usize = 2;

/// Physical drawbar default `88 8000 000` (engine default).
const LIVE_DEFAULT: [i32; 9] = [8, 8, 8, 0, 0, 0, 0, 0, 0];

fn bar(name: &'static str, default: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min: 0, max: 8 })
}

/// Parameters of one organ layer.
#[derive(Params)]
pub struct OrganLiveLayerParams {
    /// POLY percussion (Shift+Volume, manual p.20); engine default off.
    #[id = "poly_perc"]
    pub poly_perc: BoolParam,
    /// Drawbar Live mode (Preset off, manual p.21); engine default off.
    #[id = "drawbar_live"]
    pub drawbar_live: BoolParam,
    /// SYNC (Shift+Preset): copy the physical positions into the stored preset while on.
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "live1"]
    pub live1: IntParam,
    #[id = "live2"]
    pub live2: IntParam,
    #[id = "live3"]
    pub live3: IntParam,
    #[id = "live4"]
    pub live4: IntParam,
    #[id = "live5"]
    pub live5: IntParam,
    #[id = "live6"]
    pub live6: IntParam,
    #[id = "live7"]
    pub live7: IntParam,
    #[id = "live8"]
    pub live8: IntParam,
    #[id = "live9"]
    pub live9: IntParam,
}

impl Default for OrganLiveLayerParams {
    fn default() -> Self {
        let d = LIVE_DEFAULT;
        Self {
            poly_perc: BoolParam::new("Poly Percussion", false),
            drawbar_live: BoolParam::new("Drawbar Live", false),
            sync: BoolParam::new("Preset Sync", false),
            live1: bar("Live Drawbar 1", d[0]),
            live2: bar("Live Drawbar 2", d[1]),
            live3: bar("Live Drawbar 3", d[2]),
            live4: bar("Live Drawbar 4", d[3]),
            live5: bar("Live Drawbar 5", d[4]),
            live6: bar("Live Drawbar 6", d[5]),
            live7: bar("Live Drawbar 7", d[6]),
            live8: bar("Live Drawbar 8", d[7]),
            live9: bar("Live Drawbar 9", d[8]),
        }
    }
}

impl OrganLiveLayerParams {
    fn live_positions(&self) -> [u8; 9] {
        [
            &self.live1,
            &self.live2,
            &self.live3,
            &self.live4,
            &self.live5,
            &self.live6,
            &self.live7,
            &self.live8,
            &self.live9,
        ]
        .map(|p| p.value().clamp(0, 8) as u8)
    }
}

/// Both layers plus the shared vibrato switch.
#[derive(Params)]
pub struct OrganLiveBank {
    #[nested(array, group = "Organ Live")]
    pub layers: [OrganLiveLayerParams; ORGAN_LIVE_LAYERS],
    /// Vox / Farf vibrato shared by both layers (manual p.20-21); engine default off.
    #[id = "shared_vibrato"]
    pub shared_vibrato: BoolParam,
}

impl Default for OrganLiveBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| OrganLiveLayerParams::default()),
            shared_vibrato: BoolParam::new("Shared Vox/Farf Vibrato", false),
        }
    }
}

impl OrganLiveBank {
    /// Copy every value into the engine (no allocation; audio-thread safe). SYNC copies the
    /// physical positions into the preset after they were written.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [&mut engine.organ_a, &mut engine.organ_b];
        for (p, t) in self.layers.iter().zip(targets) {
            t.percussion_poly = p.poly_perc.value();
            t.drawbar_live = p.drawbar_live.value();
            t.live_drawbars = p.live_positions();
            if p.sync.value() {
                t.sync_preset_to_live();
            }
        }
        engine.organ_shared_vibrato = self.shared_vibrato.value();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_the_engine_unchanged() {
        let mut engine = Stage4Engine::new(44100.0);
        let before = (engine.organ_a.drawbars, engine.organ_b.live_drawbars);
        OrganLiveBank::default().sync_to_engine(&mut engine);
        assert!(!engine.organ_a.percussion_poly && !engine.organ_b.drawbar_live);
        assert!(!engine.organ_shared_vibrato);
        assert_eq!(
            (engine.organ_a.drawbars, engine.organ_b.live_drawbars),
            before
        );
    }

    #[test]
    fn values_reach_the_matching_layer() {
        let mut bank = OrganLiveBank::default();
        bank.layers[1].poly_perc = BoolParam::new("p", true);
        bank.layers[1].drawbar_live = BoolParam::new("l", true);
        bank.layers[1].live9 = bar("b", 5);
        bank.shared_vibrato = BoolParam::new("s", true);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert!(engine.organ_b.percussion_poly && !engine.organ_a.percussion_poly);
        assert!(engine.organ_b.drawbar_live && !engine.organ_a.drawbar_live);
        assert_eq!(engine.organ_b.live_drawbars[8], 5);
        assert!(engine.organ_shared_vibrato);
    }

    #[test]
    fn sync_copies_the_live_bars_into_the_preset() {
        let mut bank = OrganLiveBank::default();
        bank.layers[0].live1 = bar("b", 2);
        bank.layers[0].sync = BoolParam::new("s", true);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert_eq!(engine.organ_a.drawbars[0], 2);
        assert_eq!(engine.organ_b.drawbars[0], 8, "layer B not synced");
    }

    fn render(poly: bool) -> Vec<[f32; 2]> {
        let mut bank = OrganLiveBank::default();
        bank.layers[0].poly_perc = BoolParam::new("p", poly);
        let mut engine = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut engine);
        engine.organ_a.rotary_enabled = false;
        engine.organ_a.percussion_fast_decay = false;
        engine.organ_a.note_on(60);
        for _ in 0..24_000 {
            engine.organ_a.render_frame();
        }
        engine.organ_a.note_on(67);
        (0..4800).map(|_| engine.organ_a.render_frame()).collect()
    }

    #[test]
    fn poly_percussion_changes_rendered_audio_and_stays_finite() {
        let (single, poly) = (render(false), render(true));
        assert!(single
            .iter()
            .chain(&poly)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(single.iter().any(|f| f[0].abs() > 1e-4), "organ is silent");
        assert!(single != poly, "poly percussion renders identically");
    }

    #[test]
    fn drawbar_live_changes_rendered_audio() {
        let render = |live: bool| {
            let mut bank = OrganLiveBank::default();
            bank.layers[0].drawbar_live = BoolParam::new("l", live);
            bank.layers[0].live1 = bar("b", 0);
            bank.layers[0].live2 = bar("b", 0);
            bank.layers[0].live3 = bar("b", 0);
            bank.layers[0].live6 = bar("b", 8);
            let mut engine = Stage4Engine::new(48_000.0);
            bank.sync_to_engine(&mut engine);
            engine.organ_a.note_on(60);
            (0..4800)
                .map(|_| engine.organ_a.render_frame())
                .collect::<Vec<_>>()
        };
        assert!(render(false) != render(true));
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let ids: Vec<String> = OrganLiveBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 12 * ORGAN_LIVE_LAYERS + 1);
        for id in [
            "poly_perc_1",
            "drawbar_live_2",
            "sync_1",
            "live9_2",
            "shared_vibrato",
        ] {
            assert!(ids.contains(&id.to_string()), "{id}");
        }
    }
}
