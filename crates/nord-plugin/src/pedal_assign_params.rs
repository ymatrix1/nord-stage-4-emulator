//! Triple Pedal assignments and Sustain Pedal Type host parameters (id prefix `pedx`; manual
//! OS v1.6x p.26 and p.60, Pedal menu pages 3 and 4).
//!
//! IDs are additive. Defaults equal the engine defaults (Left = Una Corda, Middle = Sostenuto,
//! type `Sp2`), so an untouched host leaves sounds unchanged. The `*_pos` pedal positions are
//! pushed to the engine only when they move, so MIDI `CC66` / `CC67` keep working next to them.

use std::sync::atomic::{AtomicU32, Ordering};

use nih_plug::prelude::*;
use nord_core::engine::{
    PanelFunc, Stage4Engine, SustainPedalType, TripleFunc, TriplePedal, TripleUnit,
};

/// Function order of the `*_left` / `*_mid` selectors (Pedal menu "Range" lists).
pub const FUNCS: [TripleFunc; 13] = [
    TripleFunc::UnaCorda,
    TripleFunc::Sostenuto,
    TripleFunc::PanelAssigned,
    TripleFunc::LayerSceneHold,
    TripleFunc::LayerSceneToggle,
    TripleFunc::Pump,
    TripleFunc::MstClkTap,
    TripleFunc::SynthVibrato,
    TripleFunc::ProgramUp,
    TripleFunc::ProgramDown,
    TripleFunc::RotorHold,
    TripleFunc::RotorToggle,
    TripleFunc::Control,
];

const SUSTAIN_TYPES: [SustainPedalType; 5] = [
    SustainPedalType::Sp1,
    SustainPedalType::Sp2,
    SustainPedalType::Tp1,
    SustainPedalType::NormallyOpen,
    SustainPedalType::NormallyClosed,
];

const PANEL: [PanelFunc; 4] = [
    PanelFunc::LayerScene,
    PanelFunc::MstClkTap,
    PanelFunc::SynthVibrato,
    PanelFunc::Pump,
];

fn int(name: &str, default: i32, max: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min: 0, max })
}

fn pos(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01)
}

/// Triple pedal and Sustain Pedal Type settings.
#[derive(Params)]
pub struct PedalAssignParams {
    /// Sustain pedal type: 0 SP-1, 1 SP-2 (default, continuous), 2 TP-1, 3 Normally Open,
    /// 4 Normally Closed.
    #[id = "sustain_type"]
    pub sustain_type: IntParam,
    /// A Nord TP-2 is attached (then a TP-1 is "Sustain Only").
    #[id = "tp2"]
    pub tp2_attached: BoolParam,
    /// "Panel Assigned" target: 0 Layer Scene II, 1 Mst Clk Tap, 2 Synth Vibrato, 3 Pump.
    #[id = "panel"]
    pub panel: IntParam,
    /// TP-1 left / middle function, index into [`FUNCS`] (`Control` is not allowed on a TP-1).
    #[id = "tp1_left"]
    pub tp1_left: IntParam,
    #[id = "tp1_mid"]
    pub tp1_mid: IntParam,
    /// TP-2 left / middle function, index into [`FUNCS`].
    #[id = "tp2_left"]
    pub tp2_left: IntParam,
    #[id = "tp2_mid"]
    pub tp2_mid: IntParam,
    /// Pedal positions `0..1` (momentary).
    #[id = "tp1_left_pos"]
    pub tp1_left_pos: FloatParam,
    #[id = "tp1_mid_pos"]
    pub tp1_mid_pos: FloatParam,
    #[id = "tp2_left_pos"]
    pub tp2_left_pos: FloatParam,
    #[id = "tp2_mid_pos"]
    pub tp2_mid_pos: FloatParam,
    /// Last position pushed per pedal, so unchanged params do not overwrite MIDI input.
    last_pos: [AtomicU32; 4],
}

impl Default for PedalAssignParams {
    fn default() -> Self {
        let last = || AtomicU32::new(0.0f32.to_bits());
        Self {
            sustain_type: int("Sustain Pedal Type", 1, 4),
            tp2_attached: BoolParam::new("Triple Pedal 2 Attached", false),
            panel: int("Panel Assigned Function", 0, 3),
            tp1_left: int("TP-1 Left Function", 0, 12),
            tp1_mid: int("TP-1 Middle Function", 1, 12),
            tp2_left: int("TP-2 Left Function", 0, 12),
            tp2_mid: int("TP-2 Middle Function", 1, 12),
            tp1_left_pos: pos("TP-1 Left Pedal"),
            tp1_mid_pos: pos("TP-1 Middle Pedal"),
            tp2_left_pos: pos("TP-2 Left Pedal"),
            tp2_mid_pos: pos("TP-2 Middle Pedal"),
            last_pos: [last(), last(), last(), last()],
        }
    }
}

fn func(index: i32) -> TripleFunc {
    FUNCS[(index.max(0) as usize).min(FUNCS.len() - 1)]
}

impl PedalAssignParams {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let t = &mut engine.pedals.triple;
        t.sustain_type = SUSTAIN_TYPES[(self.sustain_type.value() as usize).min(4)];
        t.tp2_attached = self.tp2_attached.value();
        t.panel_assigned = PANEL[(self.panel.value() as usize).min(3)];
        t.set_func(
            TripleUnit::Tp1,
            TriplePedal::Left,
            func(self.tp1_left.value()),
        );
        t.set_func(
            TripleUnit::Tp1,
            TriplePedal::Middle,
            func(self.tp1_mid.value()),
        );
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Left,
            func(self.tp2_left.value()),
        );
        t.set_func(
            TripleUnit::Tp2,
            TriplePedal::Middle,
            func(self.tp2_mid.value()),
        );
        let pedals = [
            (TripleUnit::Tp1, TriplePedal::Left, &self.tp1_left_pos),
            (TripleUnit::Tp1, TriplePedal::Middle, &self.tp1_mid_pos),
            (TripleUnit::Tp2, TriplePedal::Left, &self.tp2_left_pos),
            (TripleUnit::Tp2, TriplePedal::Middle, &self.tp2_mid_pos),
        ];
        for (last, (unit, pedal, param)) in self.last_pos.iter().zip(pedals) {
            let v = param.value();
            if last.swap(v.to_bits(), Ordering::Relaxed) != v.to_bits() {
                engine.triple_pedal(unit, pedal, (v * 127.0).round() as u8);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn moved(v: f32) -> FloatParam {
        FloatParam::new("p", v, FloatRange::Linear { min: 0.0, max: 1.0 })
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44_100.0);
        let mut e = Stage4Engine::new(44_100.0);
        PedalAssignParams::default().sync_to_engine(&mut e);
        let (a, b) = (&e.pedals.triple, &reference.pedals.triple);
        assert_eq!(a.sustain_type, b.sustain_type);
        assert_eq!(a.tp2_attached, b.tp2_attached);
        assert_eq!(a.panel_assigned, b.panel_assigned);
        for unit in [TripleUnit::Tp1, TripleUnit::Tp2] {
            for pedal in [TriplePedal::Left, TriplePedal::Middle] {
                assert_eq!(a.func(unit, pedal), b.func(unit, pedal));
            }
        }
    }

    #[test]
    fn default_host_render_is_bit_identical_to_an_unsynced_engine() {
        let render = |sync: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if sync {
                PedalAssignParams::default().sync_to_engine(&mut e);
            }
            e.handle_midi(nord_core::midi::MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        assert_eq!(render(false), render(true));
    }

    #[test]
    fn selectors_and_positions_reach_the_engine() {
        let mut p = PedalAssignParams::default();
        p.tp2_left = int("l", 12, 12); // Control
        p.tp2_mid = int("m", 5, 12); // Pump
        p.sustain_type = int("t", 4, 4);
        p.tp1_left = int("x", 12, 12); // Control is rejected on a TP-1
        p.tp2_left_pos = moved(1.0);
        p.tp2_mid_pos = moved(0.5);
        let mut e = Stage4Engine::new(44_100.0);
        p.sync_to_engine(&mut e);
        assert_eq!(
            e.pedals.triple.sustain_type,
            SustainPedalType::NormallyClosed
        );
        assert_eq!(
            e.pedals.triple.func(TripleUnit::Tp1, TriplePedal::Left),
            TripleFunc::UnaCorda
        );
        assert_eq!(e.morph_controllers.control_pedal, 1.0);
        assert!((e.pedals.pump_value() - 64.0 / 127.0).abs() < 1e-6);
        // Unchanged positions do not re-fire (MIDI keeps control).
        e.morph_controllers.control_pedal = 0.25;
        p.sync_to_engine(&mut e);
        assert_eq!(e.morph_controllers.control_pedal, 0.25);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = PedalAssignParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 11);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
