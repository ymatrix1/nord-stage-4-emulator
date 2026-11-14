//! Sound menu Organ Keyboard Trigger Point and Pedal menu Foot Switch SW B host parameters
//! (manual OS v1.6x p.58 and p.60).
//!
//! IDs are additive under the `menx` prefix: `menx_trigger`, `menx_fsb_func` and `menx_fsb`.
//! Defaults equal the engine defaults (`TriggerPoint::High`, SW B = User Assigned, released), so
//! an untouched host renders exactly as before.

use nih_plug::prelude::*;
use nord_core::engine::{foot_switch_b_func, Stage4Engine, TriggerPoint};

/// Organ trigger point and Foot Switch SW B.
#[derive(Params)]
pub struct MenuExtraParams {
    /// Organ Keyboard Trigger Point: 0 High (default), 1 Low.
    #[id = "trigger"]
    pub trigger: IntParam,
    /// SW B function, the ten choices of Pedal menu page 6 (0 User Assigned .. 9 Rotor Toggle).
    #[id = "fsb_func"]
    pub fsb_func: IntParam,
    /// SW B state (momentary, on = pressed).
    #[id = "fsb"]
    pub fsb: BoolParam,
}

impl Default for MenuExtraParams {
    fn default() -> Self {
        Self {
            trigger: IntParam::new(
                "Organ Trigger Point",
                0,
                IntRange::Linear { min: 0, max: 1 },
            ),
            fsb_func: IntParam::new("Foot Switch SW B", 0, IntRange::Linear { min: 0, max: 9 }),
            fsb: BoolParam::new("Foot Switch SW B Pressed", false),
        }
    }
}

impl MenuExtraParams {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.organ_trigger.point = if self.trigger.value() == 1 {
            TriggerPoint::Low
        } else {
            TriggerPoint::High
        };
        engine.pedals.foot_switch_b.func =
            foot_switch_b_func(self.fsb_func.value().max(0) as usize);
        engine.foot_switch_b(self.fsb.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::engine::TripleFunc;

    #[test]
    fn defaults_equal_engine_defaults() {
        let mut e = Stage4Engine::new(44_100.0);
        let reference = Stage4Engine::new(44_100.0);
        MenuExtraParams::default().sync_to_engine(&mut e);
        assert_eq!(e.organ_trigger.point, reference.organ_trigger.point);
        assert_eq!(
            e.pedals.foot_switch_b.func,
            reference.pedals.foot_switch_b.func
        );
        assert!(!e.scene.scene_2_active);
    }

    #[test]
    fn values_reach_the_engine_and_the_switch_acts_on_its_edges() {
        let p = MenuExtraParams {
            trigger: IntParam::new("t", 1, IntRange::Linear { min: 0, max: 1 }),
            fsb_func: IntParam::new("f", 1, IntRange::Linear { min: 0, max: 9 }),
            fsb: BoolParam::new("s", true),
        };
        let mut e = Stage4Engine::new(44_100.0);
        p.sync_to_engine(&mut e);
        assert_eq!(e.organ_trigger.point, TriggerPoint::Low);
        assert_eq!(e.pedals.foot_switch_b.func, TripleFunc::LayerSceneHold);
        assert!(e.scene.scene_2_active, "SW B pressed holds Layer Scene II");
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = MenuExtraParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 3);
    }
}
