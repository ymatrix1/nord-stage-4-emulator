//! Pedal, Swell, SUSTPED / PSTICK and Aux KB host parameters (Phase 6b B9d, `ns4-ymci`;
//! manual OS v1.6x p.21-22, p.26-28, p.43-45, p.59-62).
//!
//! IDs are additive: `ped_*` (global pedal setup), `ped_{sustped,pstick,range}_1..7` (per layer
//! in the order Organ A/B, Piano A/B, Synth A/B/C) and `auxkb_*`. Defaults equal the engine
//! defaults, so an untouched host leaves sounds unchanged.

use std::sync::atomic::{AtomicU32, Ordering};

use nih_plug::prelude::*;
use nord_core::engine::{
    AuxVelocityCurve, ControlFunc, FootSwitchFunc, RightPedal, RotorFunc, Stage4Engine, SwellFunc,
};

/// Number of engine layers (Organ A/B, Piano A/B, Synth A/B/C).
pub const PEDAL_LAYERS: usize = 7;

fn int(name: &str, default: i32, min: i32, max: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min, max })
}

fn gain(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        1.0,
        FloatRange::Linear {
            min: 1.0,
            max: 10.0,
        },
    )
    .with_step_size(0.1)
}

/// Per-layer SUSTPED / PSTICK / bend range.
#[derive(Params)]
pub struct PedalLayerParams {
    /// SUSTPED: the layer follows the sustain pedal (Pianos default On, others Off).
    #[id = "sustped"]
    pub sustped: BoolParam,
    /// PSTICK: the layer follows the pitch stick (Synth and Extern layers only).
    #[id = "pstick"]
    pub pstick: BoolParam,
    /// Pitch stick range in semitones (default 2; the 12 ceiling is `ASSUMED`).
    #[id = "range"]
    pub range: IntParam,
}

impl PedalLayerParams {
    fn new(sustped: bool) -> Self {
        Self {
            sustped: BoolParam::new("SUSTPED", sustped),
            pstick: BoolParam::new("PSTICK", false),
            range: int("PSTICK Range", 2, 1, 12),
        }
    }
}

/// Global pedal menu settings plus the per-layer SUSTPED / PSTICK bank.
#[derive(Params)]
pub struct PedalBank {
    /// Swell pedal function: 0 Swell, 1 Volume, 2 Wheel Morph (Pedal menu page 2).
    #[id = "swell_func"]
    pub swell_func: IntParam,
    /// Swell pedal gain 1..10.
    #[id = "swell_gain"]
    pub swell_gain: FloatParam,
    /// Swell pedal position; 1.0 = fully open (no attenuation).
    #[id = "swell"]
    pub swell: FloatParam,
    /// Control pedal function: 0 Control, 1 Ctrl + Swell (Pedal menu page 1).
    #[id = "ctrl_func"]
    pub ctrl_func: IntParam,
    /// Control pedal gain 1..10.
    #[id = "ctrl_gain"]
    pub ctrl_gain: FloatParam,
    /// Sustain jack right pedal: 0 Sustain, 1 Sus+Rotor Hold, 2 Sus+Rotor Toggle.
    #[id = "right"]
    pub right: IntParam,
    /// Rotor pedal function: 0 Toggle, 1 Hold (Pedal menu page 5).
    #[id = "rotor_func"]
    pub rotor_func: IntParam,
    /// Rotor pedal switch state (momentary, on = pressed).
    #[id = "rotor"]
    pub rotor: BoolParam,
    /// Foot switch function: 0 Off, 1 Layer Scene Hold, 2 Layer Scene Toggle, 3 Rotor Hold,
    /// 4 Rotor Toggle (subset of Pedal menu page 6 the engine can act on).
    #[id = "foot_func"]
    pub foot_func: IntParam,
    /// Foot switch state (momentary, on = pressed).
    #[id = "foot"]
    pub foot: BoolParam,
    #[nested(array, group = "Pedal Layers")]
    pub layers: [PedalLayerParams; PEDAL_LAYERS],
    /// Last swell position pushed to the engine, so MIDI `CC4` is not overwritten every block.
    last_swell: AtomicU32,
}

impl Default for PedalBank {
    fn default() -> Self {
        Self {
            swell_func: int("Swell Function", 0, 0, 2),
            swell_gain: gain("Swell Gain"),
            swell: FloatParam::new(
                "Swell Pedal",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            ctrl_func: int("Control Function", 0, 0, 1),
            ctrl_gain: gain("Control Gain"),
            right: int("Sustain Pedal Function", 0, 0, 2),
            rotor_func: int("Rotor Pedal Function", 0, 0, 1),
            rotor: BoolParam::new("Rotor Pedal", false),
            foot_func: int("Foot Switch Function", 0, 0, 4),
            foot: BoolParam::new("Foot Switch", false),
            layers: std::array::from_fn(|i| PedalLayerParams::new((2..=3).contains(&i))),
            last_swell: AtomicU32::new(1.0f32.to_bits()),
        }
    }
}

/// Aux KB menu (Aux KB menu pages 1-3, MIDI menu "Aux Keyboard Channel").
#[derive(Params)]
pub struct AuxKbParams {
    /// Aux KB Enable (default Off).
    #[id = "enable"]
    pub enable: BoolParam,
    /// Aux KB MIDI channel 1..16, 0 = Off (default 16).
    #[id = "channel"]
    pub channel: IntParam,
    /// Keyboard curve: 0 Soft, 1 Medium, 2 Hard.
    #[id = "curve"]
    pub curve: IntParam,
    /// Organ layers on the aux keyboard, bit 0 = A, bit 1 = B (menu `--`, `A-`, `-B`, `AB`).
    #[id = "organ"]
    pub organ: IntParam,
    /// Piano layers on the aux keyboard, same coding as `organ`.
    #[id = "piano"]
    pub piano: IntParam,
    /// Synth layers on the aux keyboard, bit 0 = A, bit 1 = B, bit 2 = C.
    #[id = "synth"]
    pub synth: IntParam,
}

impl Default for AuxKbParams {
    fn default() -> Self {
        Self {
            enable: BoolParam::new("Aux KB Enable", false),
            channel: int("Aux KB Channel", 16, 0, 16),
            curve: int("Aux KB Curve", 1, 0, 2),
            organ: int("Aux KB Organ", 0, 0, 3),
            piano: int("Aux KB Piano", 0, 0, 3),
            synth: int("Aux KB Synth", 0, 0, 7),
        }
    }
}

impl PedalBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let p = &mut engine.pedals;
        p.swell_func = match self.swell_func.value() {
            1 => SwellFunc::Volume,
            2 => SwellFunc::WheelMorph,
            _ => SwellFunc::Swell,
        };
        p.control_func = match self.ctrl_func.value() {
            1 => ControlFunc::CtrlSwell,
            _ => ControlFunc::Control,
        };
        p.right_pedal = match self.right.value() {
            1 => RightPedal::SusRotorHold,
            2 => RightPedal::SusRotorToggle,
            _ => RightPedal::Sustain,
        };
        p.rotor_func = if self.rotor_func.value() == 1 {
            RotorFunc::Hold
        } else {
            RotorFunc::Toggle
        };
        p.foot_switch = match self.foot_func.value() {
            1 => FootSwitchFunc::LayerSceneHold,
            2 => FootSwitchFunc::LayerSceneToggle,
            3 => FootSwitchFunc::RotorHold,
            4 => FootSwitchFunc::RotorToggle,
            _ => FootSwitchFunc::Off,
        };
        p.swell_gain = self.swell_gain.value();
        p.control_gain = self.ctrl_gain.value();
        for (i, l) in self.layers.iter().enumerate() {
            p.sustped[i] = l.sustped.value();
            p.pstick[i] = l.pstick.value();
            p.pstick_range[i] = l.range.value() as u8;
        }
        let swell = self.swell.value();
        if self.last_swell.swap(swell.to_bits(), Ordering::Relaxed) != swell.to_bits() {
            engine.set_swell_pedal(swell);
        }
        engine.rotor_pedal(self.rotor.value());
        engine.foot_switch(self.foot.value());
        engine.refresh_pedal_routing();
    }
}

impl AuxKbParams {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.aux_kb.enabled = self.enable.value();
        match self.channel.value() {
            0 => engine.aux_kb.channel = None,
            c => engine.set_aux_channel(Some(c as u8)),
        }
        engine.aux_kb.curve = match self.curve.value() {
            0 => AuxVelocityCurve::Soft,
            2 => AuxVelocityCurve::Hard,
            _ => AuxVelocityCurve::Medium,
        };
        engine.aux_kb.set_section_masks(
            self.organ.value() as u8,
            self.piano.value() as u8,
            self.synth.value() as u8,
        );
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;
    use nord_core::morph::LayerId;

    fn engine_after(bank: &PedalBank, aux: &AuxKbParams) -> Stage4Engine {
        let mut e = Stage4Engine::new(44_100.0);
        bank.sync_to_engine(&mut e);
        aux.sync_to_engine(&mut e);
        e
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44_100.0);
        let e = engine_after(&PedalBank::default(), &AuxKbParams::default());
        assert_eq!(e.pedals.sustped, reference.pedals.sustped);
        assert_eq!(e.pedals.pstick, reference.pedals.pstick);
        assert_eq!(e.pedals.pstick_range, reference.pedals.pstick_range);
        assert_eq!(e.pedals.swell_func, reference.pedals.swell_func);
        assert_eq!(e.pedals.control_func, reference.pedals.control_func);
        assert_eq!(e.pedals.right_pedal, reference.pedals.right_pedal);
        assert_eq!(e.pedals.rotor_func, reference.pedals.rotor_func);
        assert_eq!(e.pedals.foot_switch, reference.pedals.foot_switch);
        assert_eq!(e.pedals.swell_gain, reference.pedals.swell_gain);
        assert_eq!(e.pedals.volume_gain(), 1.0);
        assert_eq!(e.aux_kb, reference.aux_kb);
        assert_eq!(e.synth_a.bend_cents, 0.0);
    }

    #[test]
    fn default_host_render_is_bit_identical_to_an_unsynced_engine() {
        let render = |sync: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if sync {
                PedalBank::default().sync_to_engine(&mut e);
                AuxKbParams::default().sync_to_engine(&mut e);
            }
            for n in [48u8, 60, 67] {
                e.handle_midi(MidiEvent::NoteOn {
                    channel: 0,
                    note: n,
                    velocity: 100,
                });
            }
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        assert_eq!(render(false), render(true));
    }

    #[test]
    fn midi_cc4_survives_resync_until_the_swell_param_moves() {
        let bank = PedalBank::default();
        let mut e = engine_after(&bank, &AuxKbParams::default());
        e.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: 4,
            value: 0,
        });
        bank.sync_to_engine(&mut e);
        assert_eq!(
            e.pedals
                .organ_swell_gain(nord_core::organ::OrganModel::Farfisa),
            0.0,
            "an unchanged param must not undo the pedal"
        );
        let mut moved = PedalBank::default();
        moved.swell = FloatParam::new("s", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 });
        moved.sync_to_engine(&mut e);
        assert_eq!(
            e.pedals
                .organ_swell_gain(nord_core::organ::OrganModel::Farfisa),
            0.5
        );
    }

    #[test]
    fn per_layer_flags_reach_only_their_layer() {
        let mut bank = PedalBank::default();
        bank.layers[4].pstick = BoolParam::new("p", true);
        bank.layers[4].range = int("r", 12, 1, 12);
        bank.layers[0].sustped = BoolParam::new("s", true);
        let mut e = engine_after(&bank, &AuxKbParams::default());
        assert!(e.pedals.pstick[4] && !e.pedals.pstick[5]);
        assert!(e.pedals.sustped[0] && e.pedals.sustped[2]);
        assert_eq!(e.pedals.pstick_range[4], 12);
        e.handle_midi(MidiEvent::PitchBend {
            channel: 0,
            value: 16383,
        });
        assert!(e.synth_a.bend_cents > 1100.0);
        assert_eq!(e.synth_b.bend_cents, 0.0);
    }

    #[test]
    fn function_selectors_map_to_engine_enums() {
        let mut bank = PedalBank::default();
        bank.swell_func = int("a", 1, 0, 2);
        bank.ctrl_func = int("b", 1, 0, 1);
        bank.right = int("c", 2, 0, 2);
        bank.rotor_func = int("d", 1, 0, 1);
        bank.foot_func = int("e", 2, 0, 4);
        let e = engine_after(&bank, &AuxKbParams::default());
        assert_eq!(e.pedals.swell_func, SwellFunc::Volume);
        assert_eq!(e.pedals.control_func, ControlFunc::CtrlSwell);
        assert_eq!(e.pedals.right_pedal, RightPedal::SusRotorToggle);
        assert_eq!(e.pedals.rotor_func, RotorFunc::Hold);
        assert_eq!(e.pedals.foot_switch, FootSwitchFunc::LayerSceneToggle);
    }

    #[test]
    fn rotor_and_foot_params_drive_the_pedals() {
        let mut bank = PedalBank::default();
        bank.rotor = BoolParam::new("r", true);
        bank.foot_func = int("f", 2, 0, 4);
        bank.foot = BoolParam::new("f", true);
        let e = engine_after(&bank, &AuxKbParams::default());
        assert_eq!(
            e.organ_a.rotary_speed,
            nord_core::organ::RotarySpeed::Fast,
            "rotor toggle latched"
        );
        assert!(e.scene.scene_2_active);
    }

    #[test]
    fn aux_params_route_notes_to_the_selected_layers() {
        let mut aux = AuxKbParams::default();
        aux.enable = BoolParam::new("e", true);
        aux.organ = int("o", 2, 0, 3);
        aux.curve = int("c", 2, 0, 2);
        let mut e = engine_after(&PedalBank::default(), &aux);
        assert_eq!(e.aux_kb.channel, Some(15));
        e.scene.scene_1_mask = 0b0111_1111;
        e.handle_midi(MidiEvent::NoteOn {
            channel: 15,
            note: 60,
            velocity: 90,
        });
        assert!(e.aux_kb.is_aux_layer(LayerId::OrganB));
        assert!(!e.aux_kb.is_aux_layer(LayerId::OrganA));
        let mut off = AuxKbParams::default();
        off.channel = int("ch", 0, 0, 16);
        off.sync_to_engine(&mut e);
        assert_eq!(e.aux_kb.channel, None);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = PedalBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 10 + 3 * PEDAL_LAYERS);
        let aux_ids: Vec<String> = AuxKbParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(aux_ids.len(), 6);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
