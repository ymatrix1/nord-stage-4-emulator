//! Foot Switch SW B (Pedal menu page 6, manual OS v1.6x p.60): the second switch of a dual pedal.
//!
//! SW B offers the same ten functions as SW A: User Assigned, Layer Scene Hold, Layer Scene Toggle,
//! Pump, Mst Clk Tap, Synth Vibrato, Program Up, Program Down, Rotor Hold, Rotor Toggle. They run
//! through the shared Triple Pedal function table (`pedal_assign.rs`), so each behaves exactly as
//! on the Left / Middle pedals. SW A keeps its own five-function enum (`FootSwitchFunc`).
//!
//! Provenance: the ten choices and their order are `VERIFIED` against the manual text (p.60). The
//! manual says SW B "has no effect when a single pedal is connected"; the engine has no pedal
//! type input, so the host calls [`Stage4Engine::foot_switch_b`] only for a dual pedal (`ASSUMED`).
//! "User Assigned" runs the panel-assigned function of the Triple Pedal (`ASSUMED`: the manual does
//! not say which panel function a user assignment resolves to).

use super::*;

/// Foot Switch SW B function and press state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FootSwitchB {
    pub func: TripleFunc,
    down: bool,
}

impl Default for FootSwitchB {
    fn default() -> Self {
        Self {
            func: TripleFunc::PanelAssigned,
            down: false,
        }
    }
}

/// Function of the menu choice at `index` (User Assigned first; out of range gives Rotor Toggle).
pub fn foot_switch_b_func(index: usize) -> TripleFunc {
    match index {
        0 => TripleFunc::PanelAssigned,
        1 => TripleFunc::LayerSceneHold,
        2 => TripleFunc::LayerSceneToggle,
        3 => TripleFunc::Pump,
        4 => TripleFunc::MstClkTap,
        5 => TripleFunc::SynthVibrato,
        6 => TripleFunc::ProgramUp,
        7 => TripleFunc::ProgramDown,
        8 => TripleFunc::RotorHold,
        _ => TripleFunc::RotorToggle,
    }
}

impl Stage4Engine {
    /// Foot Switch SW B edge (`down` = pressed), acting per `pedals.foot_switch_b.func`. Repeated
    /// calls with the same state do nothing.
    pub fn foot_switch_b(&mut self, down: bool) {
        if std::mem::replace(&mut self.pedals.foot_switch_b.down, down) == down {
            return;
        }
        let func = self.pedals.triple.resolve(self.pedals.foot_switch_b.func);
        let value = if down { 127 } else { 0 };
        self.apply_pedal_func(func, value, down, !down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(e: &mut Stage4Engine) {
        e.foot_switch_b(true);
        e.foot_switch_b(false);
    }

    #[test]
    fn choices_follow_the_manual_order() {
        let funcs: Vec<_> = (0..10).map(foot_switch_b_func).collect();
        assert_eq!(funcs[1], TripleFunc::LayerSceneHold);
        assert_eq!(funcs[3], TripleFunc::Pump);
        assert_eq!(funcs[7], TripleFunc::ProgramDown);
        assert_eq!(funcs[9], TripleFunc::RotorToggle);
        assert_eq!(foot_switch_b_func(99), TripleFunc::RotorToggle);
        assert_eq!(FootSwitchB::default().func, funcs[0]);
    }

    #[test]
    fn layer_scene_hold_follows_the_switch_and_toggle_flips_on_press() {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.foot_switch_b.func = foot_switch_b_func(1);
        e.foot_switch_b(true);
        assert!(e.scene.scene_2_active);
        e.foot_switch_b(false);
        assert!(!e.scene.scene_2_active);
        e.pedals.foot_switch_b.func = foot_switch_b_func(2);
        press(&mut e);
        assert!(e.scene.scene_2_active);
        press(&mut e);
        assert!(!e.scene.scene_2_active);
    }

    #[test]
    fn rotor_program_pump_and_vibrato_functions_run() {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.foot_switch_b.func = foot_switch_b_func(9);
        press(&mut e);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast);
        e.pedals.foot_switch_b.func = foot_switch_b_func(8);
        e.foot_switch_b(true);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast);
        e.foot_switch_b(false);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Slow);
        e.pedals.foot_switch_b.func = foot_switch_b_func(6);
        press(&mut e);
        press(&mut e);
        e.pedals.foot_switch_b.func = foot_switch_b_func(7);
        press(&mut e);
        assert_eq!(e.pedals.take_program_step(), 1);
        e.pedals.foot_switch_b.func = foot_switch_b_func(3);
        e.foot_switch_b(true);
        assert_eq!(e.pedals.pump_value(), 1.0);
        e.foot_switch_b(false);
        assert_eq!(e.pedals.pump_value(), 0.0);
        e.pedals.foot_switch_b.func = foot_switch_b_func(5);
        e.foot_switch_b(true);
        assert_eq!(e.synth_a.vibrato_ctrl, 1.0);
    }

    #[test]
    fn user_assigned_resolves_through_the_panel_function_and_repeats_are_ignored() {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.triple.panel_assigned = PanelFunc::Pump;
        e.foot_switch_b(true);
        e.foot_switch_b(true);
        assert_eq!(e.pedals.pump_value(), 1.0);
    }

    #[test]
    fn switch_a_and_the_jack_pedals_are_not_touched() {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.foot_switch_b.func = foot_switch_b_func(1);
        press(&mut e);
        assert_eq!(e.pedals.foot_switch, FootSwitchFunc::Off);
        e.foot_switch(true);
        assert!(!e.scene.scene_2_active, "SW A is Off and independent");
    }
}
