//! Nord Triple Pedal assignments and the Sustain Pedal Type menu setting.
//!
//! Spec: `docs/specs/midi-and-control.md` "Triple Pedal" (manual OS v1.6x p.26 and p.60, Pedal
//! menu pages 3 and 4). The right pedal always stays Sustain (`pedals.rs`); this file adds the
//! Left and Middle pedals of a Nord TP-2 (Triple Pedal jack) and of a TP-1 on the Sustain jack.
//!
//! Defaults reproduce the previous behaviour: Left = Una Corda, Middle = Sostenuto, so `CC67`
//! and `CC66` still drive the piano soft and sostenuto pedals exactly as before.

use super::*;

/// Pedal-down threshold shared with the sustain pedal (`value >= 64`).
const DOWN: u8 = 64;
/// Shortest and longest tap interval that sets a tempo, in seconds. ASSUMED: 0.2 s = 300 BPM and
/// 2.0 s = 30 BPM, the Master Clock range of the engine (`MasterClock::advance` clamps to it);
/// a hardware tap-tempo window would confirm it.
const TAP_MIN_SEC: f32 = 0.2;
const TAP_MAX_SEC: f32 = 2.0;

/// Which unit a pedal event comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripleUnit {
    /// Nord TP-1 on the Sustain Pedal jack (Pedal menu page 3).
    Tp1,
    /// Nord TP-2 on the dedicated Triple Pedal jack (Pedal menu page 4).
    Tp2,
}

/// Left or middle pedal of a triple pedal unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriplePedal {
    Left,
    Middle,
}

/// Function of a Left or Middle pedal (Pedal menu "Range Left" / "Range Middle", manual p.60).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripleFunc {
    /// Left default: piano soft pedal (manual p.26).
    UnaCorda,
    /// Middle default: piano sostenuto pedal (manual p.26).
    Sostenuto,
    /// Follows the panel button that is lit, see [`PanelFunc`].
    PanelAssigned,
    LayerSceneHold,
    LayerSceneToggle,
    /// Pump effect in Mod 1 PED mode: the engine only exposes [`PedalState::pump_value`].
    Pump,
    /// Master Clock tempo tap.
    MstClkTap,
    /// Synth vibrato amount in PED vibrato mode.
    SynthVibrato,
    ProgramUp,
    ProgramDown,
    RotorHold,
    RotorToggle,
    /// Morph Control pedal source (TP-2 only, manual p.60).
    Control,
}

/// The panel function a "Panel Assigned" pedal resolves to (manual p.60 list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelFunc {
    /// "Layer Scene II Pedal". ASSUMED to behave as Hold; the manual does not say.
    #[default]
    LayerScene,
    MstClkTap,
    SynthVibrato,
    Pump,
}

/// Sustain Pedal Type (Pedal menu page 3 "TYPE"): the polarity and kind of the pedal on the
/// Sustain jack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SustainPedalType {
    /// Nord SP-1, on/off switch. Hardware default; the engine default is `Sp2` so that continuous
    /// `CC64` half pedaling keeps working.
    Sp1,
    /// Nord SP-2, continuous (half pedaling).
    #[default]
    Sp2,
    /// Nord TP-1 triple pedal, continuous right pedal.
    Tp1,
    /// Generic switch, contact closes when pressed.
    NormallyOpen,
    /// Generic switch, contact opens when pressed: the sustain is inverted.
    NormallyClosed,
}

/// Triple pedal configuration and live state (a field of [`PedalState`]).
#[derive(Debug, Clone)]
pub struct TripleState {
    /// `[unit][pedal]` functions: TP-1 then TP-2, Left then Middle.
    funcs: [[TripleFunc; 2]; 2],
    /// Target of `PanelAssigned`.
    pub panel_assigned: PanelFunc,
    /// Sustain jack pedal type.
    pub sustain_type: SustainPedalType,
    /// A TP-2 is attached: the Left and Middle pedals of a TP-1 are then "Sustain Only" (p.26).
    pub tp2_attached: bool,
    down: [[bool; 2]; 2],
    pump: f32,
    program_step: i8,
    tap_age: u32,
}

impl Default for TripleState {
    fn default() -> Self {
        Self {
            funcs: [[TripleFunc::UnaCorda, TripleFunc::Sostenuto]; 2],
            panel_assigned: PanelFunc::default(),
            sustain_type: SustainPedalType::default(),
            tp2_attached: false,
            down: [[false; 2]; 2],
            pump: 0.0,
            program_step: 0,
            tap_age: u32::MAX,
        }
    }
}

impl TripleState {
    /// Function of `pedal` on `unit`.
    pub fn func(&self, unit: TripleUnit, pedal: TriplePedal) -> TripleFunc {
        self.funcs[unit as usize][pedal as usize]
    }

    /// Assigns `func` to `pedal` on `unit`. `Control` is TP-2 only, so a TP-1 request is ignored.
    pub fn set_func(&mut self, unit: TripleUnit, pedal: TriplePedal, func: TripleFunc) {
        if unit == TripleUnit::Tp1 && func == TripleFunc::Control {
            return;
        }
        self.funcs[unit as usize][pedal as usize] = func;
    }

    /// Advances the tap-tempo clock by one sample (called once per rendered frame).
    pub(super) fn tick(&mut self) {
        self.tap_age = self.tap_age.saturating_add(1);
    }

    pub(super) fn resolve(&self, func: TripleFunc) -> TripleFunc {
        if func != TripleFunc::PanelAssigned {
            return func;
        }
        match self.panel_assigned {
            PanelFunc::LayerScene => TripleFunc::LayerSceneHold,
            PanelFunc::MstClkTap => TripleFunc::MstClkTap,
            PanelFunc::SynthVibrato => TripleFunc::SynthVibrato,
            PanelFunc::Pump => TripleFunc::Pump,
        }
    }

    /// Sustain value as seen by the engine after the pedal type: switch types are binary and
    /// `NormallyClosed` inverts. `Sp2` / `Tp1` pass through (half pedaling).
    pub(super) fn condition_sustain(&self, value: u8) -> u8 {
        match self.sustain_type {
            SustainPedalType::Sp2 | SustainPedalType::Tp1 => value,
            SustainPedalType::Sp1 | SustainPedalType::NormallyOpen => {
                if value >= DOWN {
                    127
                } else {
                    0
                }
            }
            SustainPedalType::NormallyClosed => {
                if value >= DOWN {
                    0
                } else {
                    127
                }
            }
        }
    }
}

impl PedalState {
    /// Pump pedal position `0.0..=1.0` (Left/Middle pedal set to Pump). The Pump FX type does not
    /// exist yet; when it does, feed this value to the Mod 1 PED position of the chains.
    pub fn pump_value(&self) -> f32 {
        self.triple.pump
    }

    /// Program Up/Down presses since the last call (`+1` per Program Up, `-1` per Program Down,
    /// saturating). The host owns the program list: it calls this once per block and loads the
    /// neighbouring program. Allocation free.
    pub fn take_program_step(&mut self) -> i8 {
        std::mem::take(&mut self.triple.program_step)
    }
}

impl Stage4Engine {
    /// Triple pedal input `0..=127` for the Left or Middle pedal of `unit`, acting on its function.
    /// Switch-like functions act on the press edge (`>= 64`); Una Corda, Sostenuto, Pump, Synth
    /// Vibrato and Control follow the continuous value.
    pub fn triple_pedal(&mut self, unit: TripleUnit, pedal: TriplePedal, value: u8) {
        if unit == TripleUnit::Tp1
            && (self.pedals.triple.sustain_type != SustainPedalType::Tp1
                || self.pedals.triple.tp2_attached)
        {
            return; // "Sustain Only": no Left/Middle on this unit.
        }
        let (u, p) = (unit as usize, pedal as usize);
        let value = value.min(127);
        let down = value >= DOWN;
        let was = std::mem::replace(&mut self.pedals.triple.down[u][p], down);
        let func = self.pedals.triple.resolve(self.pedals.triple.funcs[u][p]);
        self.apply_pedal_func(func, value, down && !was, !down && was);
    }

    /// Runs a resolved pedal function for an input of `value` (`0..=127`); `pressed` / `released`
    /// are the edges of that input. Shared by the Triple Pedal and the Foot Switch SW B.
    pub(super) fn apply_pedal_func(
        &mut self,
        func: TripleFunc,
        value: u8,
        pressed: bool,
        released: bool,
    ) {
        let norm = f32::from(value) / 127.0;
        let down = value >= DOWN;
        match func {
            TripleFunc::UnaCorda => {
                self.piano_a.set_soft_pedal(down, value);
                self.piano_b.set_soft_pedal(down, value);
            }
            TripleFunc::Sostenuto => {
                self.piano_a.set_sostenuto_pedal(down, value);
                self.piano_b.set_sostenuto_pedal(down, value);
            }
            TripleFunc::LayerSceneHold if pressed || released => {
                self.scene.scene_2_active = down;
            }
            TripleFunc::LayerSceneToggle if pressed => {
                self.scene.scene_2_active = !self.scene.scene_2_active;
            }
            TripleFunc::RotorHold if pressed || released => self.rotor_hold(down),
            TripleFunc::RotorToggle if pressed => self.rotor_toggle(),
            TripleFunc::Pump => self.pedals.triple.pump = norm,
            TripleFunc::MstClkTap if pressed => self.tap_tempo(),
            TripleFunc::SynthVibrato => {
                for synth in [&mut self.synth_a, &mut self.synth_b, &mut self.synth_c] {
                    synth.vibrato_ctrl = norm;
                }
            }
            TripleFunc::ProgramUp if pressed => {
                let t = &mut self.pedals.triple;
                t.program_step = t.program_step.saturating_add(1);
            }
            TripleFunc::ProgramDown if pressed => {
                let t = &mut self.pedals.triple;
                t.program_step = t.program_step.saturating_sub(1);
            }
            TripleFunc::Control => self.morph_controllers.control_pedal = norm,
            _ => {}
        }
    }

    /// Master Clock tap: two taps `0.2..=2.0` s apart set the tempo to `60 / interval` BPM and
    /// re-snap the synced delay times; a longer gap starts a new tap sequence. Allocation free.
    pub fn tap_tempo(&mut self) {
        let age = self.pedals.triple.tap_age;
        self.pedals.triple.tap_age = 0;
        let interval = age as f32 / self.sample_rate;
        if !(TAP_MIN_SEC..=TAP_MAX_SEC).contains(&interval) {
            return;
        }
        self.master_clock.bpm = (60.0 / interval).clamp(30.0, 300.0);
        for fx in &mut self.fx_chains {
            fx.snap_delay_time(self.master_clock.bpm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.triple.tp2_attached = true;
        e
    }

    fn press(e: &mut Stage4Engine, unit: TripleUnit, pedal: TriplePedal) {
        e.triple_pedal(unit, pedal, 127);
        e.triple_pedal(unit, pedal, 0);
    }

    #[test]
    fn defaults_are_una_corda_and_sostenuto_and_leave_cc_behaviour_alone() {
        let t = TripleState::default();
        for unit in [TripleUnit::Tp1, TripleUnit::Tp2] {
            assert_eq!(t.func(unit, TriplePedal::Left), TripleFunc::UnaCorda);
            assert_eq!(t.func(unit, TriplePedal::Middle), TripleFunc::Sostenuto);
        }
        assert_eq!(t.sustain_type, SustainPedalType::Sp2);
        assert!(!t.tp2_attached);
        assert_eq!(t.condition_sustain(40), 40);
    }

    #[test]
    fn una_corda_and_sostenuto_reach_the_pianos() {
        let mut e = engine();
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 100);
        assert!(e.piano_a.soft_pedal_down && e.piano_b.soft_pedal_down);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 0);
        assert!(!e.piano_a.soft_pedal_down);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Middle, 100);
        assert!(e.piano_a.sostenuto_pedal_down);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Middle, 0);
        assert!(!e.piano_b.sostenuto_pedal_down);
    }

    #[test]
    fn layer_scene_rotor_and_program_functions_act_on_edges() {
        let mut e = engine();
        let t = &mut e.pedals.triple;
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Left,
            TripleFunc::LayerSceneHold,
        );
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Middle,
            TripleFunc::RotorToggle,
        );
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
        assert!(e.scene.scene_2_active);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 0);
        assert!(!e.scene.scene_2_active);
        press(&mut e, TripleUnit::Tp2, TriplePedal::Middle);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast);
        press(&mut e, TripleUnit::Tp2, TriplePedal::Middle);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Slow);

        let t = &mut e.pedals.triple;
        t.set_func(TripleUnit::Tp2, TriplePedal::Left, TripleFunc::ProgramUp);
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Middle,
            TripleFunc::ProgramDown,
        );
        press(&mut e, TripleUnit::Tp2, TriplePedal::Left);
        press(&mut e, TripleUnit::Tp2, TriplePedal::Left);
        press(&mut e, TripleUnit::Tp2, TriplePedal::Middle);
        assert_eq!(e.pedals.take_program_step(), 1);
        assert_eq!(e.pedals.take_program_step(), 0);
    }

    #[test]
    fn control_pump_and_vibrato_follow_the_continuous_value() {
        let mut e = engine();
        let t = &mut e.pedals.triple;
        t.set_func(TripleUnit::Tp2, TriplePedal::Left, TripleFunc::Control);
        t.set_func(TripleUnit::Tp2, TriplePedal::Middle, TripleFunc::Pump);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
        assert_eq!(e.morph_controllers.control_pedal, 1.0);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Middle, 64);
        assert!((e.pedals.pump_value() - 64.0 / 127.0).abs() < 1e-6);
        e.pedals.triple.set_func(
            TripleUnit::Tp2,
            TriplePedal::Middle,
            TripleFunc::SynthVibrato,
        );
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Middle, 127);
        assert_eq!(e.synth_b.vibrato_ctrl, 1.0);
    }

    #[test]
    fn panel_assigned_resolves_to_the_lit_panel_function() {
        let mut e = engine();
        let t = &mut e.pedals.triple;
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Left,
            TripleFunc::PanelAssigned,
        );
        t.panel_assigned = PanelFunc::Pump;
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
        assert_eq!(e.pedals.pump_value(), 1.0);
        e.pedals.triple.panel_assigned = PanelFunc::LayerScene;
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 0);
        e.triple_pedal(TripleUnit::Tp2, TriplePedal::Left, 127);
        assert!(e.scene.scene_2_active);
    }

    #[test]
    fn tp1_is_sustain_only_when_not_selected_or_when_a_tp2_is_attached() {
        let mut e = Stage4Engine::new(48_000.0);
        e.triple_pedal(TripleUnit::Tp1, TriplePedal::Left, 127);
        assert!(!e.piano_a.soft_pedal_down, "type is not TP-1");
        e.pedals.triple.sustain_type = SustainPedalType::Tp1;
        e.triple_pedal(TripleUnit::Tp1, TriplePedal::Left, 127);
        assert!(e.piano_a.soft_pedal_down);
        e.triple_pedal(TripleUnit::Tp1, TriplePedal::Left, 0);
        e.pedals.triple.tp2_attached = true;
        e.triple_pedal(TripleUnit::Tp1, TriplePedal::Left, 127);
        assert!(!e.piano_a.soft_pedal_down, "TP-2 attached");
        // Control is a TP-2 only function.
        let t = &mut e.pedals.triple;
        t.set_func(TripleUnit::Tp1, TriplePedal::Left, TripleFunc::Control);
        assert_eq!(
            t.func(TripleUnit::Tp1, TriplePedal::Left),
            TripleFunc::UnaCorda
        );
    }

    #[test]
    fn tap_tempo_sets_bpm_from_the_interval_and_rejects_long_gaps() {
        let mut e = Stage4Engine::new(48_000.0);
        e.tap_tempo(); // first tap only starts a sequence
        assert_eq!(e.master_clock.bpm, 120.0);
        for _ in 0..24_000 {
            e.pedals.triple.tick();
        }
        e.tap_tempo(); // 0.5 s -> 120 BPM
        assert!((e.master_clock.bpm - 120.0).abs() < 1e-3);
        for _ in 0..36_000 {
            e.pedals.triple.tick();
        }
        e.tap_tempo(); // 0.75 s -> 80 BPM
        assert!((e.master_clock.bpm - 80.0).abs() < 1e-3);
        for _ in 0..200_000 {
            e.pedals.triple.tick();
        }
        e.tap_tempo(); // > 2 s: ignored
        assert!((e.master_clock.bpm - 80.0).abs() < 1e-3);
    }

    #[test]
    fn render_frame_advances_the_tap_clock() {
        let mut e = Stage4Engine::new(48_000.0);
        e.tap_tempo();
        for _ in 0..24_000 {
            e.render_frame();
        }
        e.tap_tempo();
        assert!((e.master_clock.bpm - 120.0).abs() < 1e-3);
    }

    #[test]
    fn sustain_type_conditions_the_jack_value() {
        let mut e = Stage4Engine::new(48_000.0);
        let half = 40u8;
        e.set_sustain_pedal(PedalSource::Global, half);
        assert_eq!(e.pedals.sustain[0], half, "default keeps half pedaling");
        e.pedals.triple.sustain_type = SustainPedalType::Sp1;
        e.set_sustain_pedal(PedalSource::Global, half);
        assert_eq!(e.pedals.sustain[0], 0, "switch types are binary");
        e.set_sustain_pedal(PedalSource::Global, 90);
        assert_eq!(e.pedals.sustain[0], 127);
        e.pedals.triple.sustain_type = SustainPedalType::NormallyClosed;
        e.set_sustain_pedal(PedalSource::Global, 0);
        assert_eq!(e.pedals.sustain[0], 127, "inverted polarity");
        e.set_sustain_pedal(PedalSource::Global, 127);
        assert_eq!(e.pedals.sustain[0], 0);
        // The Aux KB channel is MIDI, not the jack: never conditioned.
        e.set_sustain_pedal(PedalSource::Aux, 40);
        assert_eq!(e.pedals.sustain[1], 40);
    }
}
