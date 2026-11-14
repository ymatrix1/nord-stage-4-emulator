//! Pedals and pitch stick: sustain (with half pedaling), swell, control pedal, rotor pedal,
//! foot switch, and the per-layer SUSTPED / PSTICK routing.
//!
//! Spec: `docs/specs/midi-and-control.md` (manual OS v1.6x p.18, p.21-22, p.26-28, p.43-45, p.60-61).

use super::*;
use crate::organ::OrganModel;

/// Sustain pedal "down" threshold (`CC64 >= 64`), the MIDI convention the engine already used.
const SUSTAIN_DOWN: u8 = 64;
/// Half-pedal band lower edge as a pedal depth. ASSUMED: dampers start lifting at 20 % travel.
pub const HALF_PEDAL_MIN: f32 = 0.2;
/// Lowest Swell gain for B3 models (the swell pedal never reaches zero, manual p.22).
/// ASSUMED: -20 dB; the manual only says "not all the way to zero".
pub const B3_SWELL_FLOOR: f32 = 0.1;

/// Swell pedal destination (Pedal menu page 2 "FUNC").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwellFunc {
    #[default]
    Swell,
    Volume,
    WheelMorph,
}

/// Control pedal function (Pedal menu page 1 "FUNC").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlFunc {
    #[default]
    Control,
    CtrlSwell,
}

/// Right pedal / sustain jack function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightPedal {
    #[default]
    Sustain,
    SusRotorHold,
    SusRotorToggle,
}

/// Rotor pedal jack function (Pedal menu page 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RotorFunc {
    #[default]
    Toggle,
    Hold,
}

/// Foot switch assignment (Pedal menu page 6, the subset the engine can act on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FootSwitchFunc {
    #[default]
    Off,
    LayerSceneHold,
    LayerSceneToggle,
    RotorHold,
    RotorToggle,
}

/// Which input a sustain or bend message arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PedalSource {
    /// Global channel (the internal pedal jack).
    Global,
    /// Aux KB channel.
    Aux,
}

/// Pedal configuration and live state owned by `Stage4Engine`.
#[derive(Debug, Clone)]
pub struct PedalState {
    pub swell_func: SwellFunc,
    pub control_func: ControlFunc,
    pub right_pedal: RightPedal,
    pub rotor_func: RotorFunc,
    pub foot_switch: FootSwitchFunc,
    /// Pedal gain `1.0..=10.0` (Pedal menu "GAIN").
    pub swell_gain: f32,
    pub control_gain: f32,
    /// Per-layer SUSTPED. Defaults keep the prior behaviour: only the pianos follow the pedal.
    pub sustped: [bool; 7],
    /// Per-layer PSTICK (pitch stick). Honoured by Synth and Extern layers.
    pub pstick: [bool; 7],
    /// Per-layer bend range in semitones (default 2, PSTICK/RNG list, p.28).
    pub pstick_range: [u8; 7],
    pub(super) swell: f32,
    pub(super) volume: f32,
    pub(super) sustain: [u8; 2],
    pub(super) bend: [f32; 2],
    pub(super) held: [u128; 7],
    sustain_was_down: bool,
    rotor_pedal_down: bool,
    foot_switch_down: bool,
}

impl Default for PedalState {
    fn default() -> Self {
        Self {
            swell_func: SwellFunc::default(),
            control_func: ControlFunc::default(),
            right_pedal: RightPedal::default(),
            rotor_func: RotorFunc::default(),
            foot_switch: FootSwitchFunc::default(),
            swell_gain: 1.0,
            control_gain: 1.0,
            sustped: [false, false, true, true, false, false, false],
            pstick: [false; 7],
            pstick_range: [2; 7],
            swell: 1.0,
            volume: 1.0,
            sustain: [0; 2],
            bend: [0.0; 2],
            held: [0; 7],
            sustain_was_down: false,
            rotor_pedal_down: false,
            foot_switch_down: false,
        }
    }
}

impl PedalState {
    /// Organ output gain for the current swell position; exactly `1.0` at full swell.
    pub fn organ_swell_gain(&self, model: OrganModel) -> f32 {
        if self.swell_func != SwellFunc::Swell || self.swell >= 1.0 {
            return 1.0;
        }
        match model {
            OrganModel::B3 => B3_SWELL_FLOOR + (1.0 - B3_SWELL_FLOOR) * self.swell.max(0.0),
            _ => self.swell.max(0.0),
        }
    }

    /// Master gain from a swell pedal set to Volume; exactly `1.0` by default.
    pub fn volume_gain(&self) -> f32 {
        self.volume
    }
}

fn scaled(value: f32, gain: f32) -> f32 {
    (value * gain.clamp(1.0, 10.0)).clamp(0.0, 1.0)
}

impl Stage4Engine {
    /// Organ Swell input (`CC4`), `0.0..=1.0`. Routed by the swell pedal function.
    pub fn set_swell_pedal(&mut self, norm: f32) {
        let v = scaled(norm, self.pedals.swell_gain);
        match self.pedals.swell_func {
            SwellFunc::Swell => self.pedals.swell = v,
            SwellFunc::Volume => self.pedals.volume = v,
            SwellFunc::WheelMorph => self.morph_controllers.mod_wheel = v,
        }
    }

    /// Control pedal input (`CC11`): the Morph source, and Swell too for "Ctrl + Swell".
    pub fn set_control_pedal(&mut self, norm: f32) {
        let v = scaled(norm, self.pedals.control_gain);
        self.morph_controllers.control_pedal = v;
        if self.pedals.control_func == ControlFunc::CtrlSwell {
            self.pedals.swell = v;
        }
    }

    /// Sustain pedal value `0..=127` from the Global (internal jack) or Aux KB channel.
    pub fn set_sustain_pedal(&mut self, source: PedalSource, value: u8) {
        self.pedals.sustain[source as usize] = value.min(127);
        if source == PedalSource::Global {
            let down = value >= SUSTAIN_DOWN;
            let was_down = std::mem::replace(&mut self.pedals.sustain_was_down, down);
            if down != was_down {
                match self.pedals.right_pedal {
                    RightPedal::Sustain => {}
                    RightPedal::SusRotorHold => self.rotor_hold(down),
                    RightPedal::SusRotorToggle => {
                        if down {
                            self.rotor_toggle();
                        }
                    }
                }
            }
        }
        self.apply_sustain();
    }

    /// Pitch bend `0..=16383` (centre 8192) from the Global or Aux KB channel.
    pub fn set_pitch_bend(&mut self, source: PedalSource, value: u16) {
        self.pedals.bend[source as usize] =
            ((f32::from(value.min(16383)) - 8192.0) / 8192.0).clamp(-1.0, 1.0);
        self.apply_bend();
    }

    /// Rotor pedal jack edge (`down` = pressed), acting per `rotor_func`.
    pub fn rotor_pedal(&mut self, down: bool) {
        if std::mem::replace(&mut self.pedals.rotor_pedal_down, down) == down {
            return;
        }
        match self.pedals.rotor_func {
            RotorFunc::Hold => self.rotor_hold(down),
            RotorFunc::Toggle => {
                if down {
                    self.rotor_toggle();
                }
            }
        }
    }

    /// Foot switch edge (`down` = pressed), acting per `foot_switch`.
    pub fn foot_switch(&mut self, down: bool) {
        if std::mem::replace(&mut self.pedals.foot_switch_down, down) == down {
            return;
        }
        match self.pedals.foot_switch {
            FootSwitchFunc::Off => {}
            FootSwitchFunc::LayerSceneHold => self.scene.scene_2_active = down,
            FootSwitchFunc::LayerSceneToggle => {
                if down {
                    self.scene.scene_2_active = !self.scene.scene_2_active;
                }
            }
            FootSwitchFunc::RotorHold => self.rotor_hold(down),
            FootSwitchFunc::RotorToggle => {
                if down {
                    self.rotor_toggle();
                }
            }
        }
    }

    fn set_rotor_speed(&mut self, speed: RotarySpeed) {
        self.organ_a.rotary_speed = speed;
        self.organ_b.rotary_speed = speed;
    }

    fn rotor_hold(&mut self, down: bool) {
        self.set_rotor_speed(if down {
            RotarySpeed::Fast
        } else {
            RotarySpeed::Slow
        });
    }

    fn rotor_toggle(&mut self) {
        let fast = self.organ_a.rotary_speed == RotarySpeed::Fast;
        self.set_rotor_speed(if fast {
            RotarySpeed::Slow
        } else {
            RotarySpeed::Fast
        });
    }

    /// Re-derives per-layer sustain and bend after SUSTPED / PSTICK / range / Aux changes.
    /// Idempotent and allocation-free; backends call it after syncing host parameters.
    pub fn refresh_pedal_routing(&mut self) {
        self.apply_sustain();
        self.apply_bend();
    }

    /// Effective sustain value of `layer`: the internal pedal when SUSTPED is on, plus the Aux KB
    /// channel for layers in Aux KB mode (which respond to it regardless of SUSTPED).
    fn layer_sustain_value(&self, layer: LayerId) -> u8 {
        let idx = layer as usize;
        let internal = if self.pedals.sustped[idx] {
            self.pedals.sustain[PedalSource::Global as usize]
        } else {
            0
        };
        if self.aux_kb.is_aux_layer(layer) {
            internal.max(self.pedals.sustain[PedalSource::Aux as usize])
        } else {
            internal
        }
    }

    /// Effective pitch stick position `-1.0..=1.0` of `layer` (same Aux rule as sustain).
    fn layer_bend_norm(&self, layer: LayerId) -> f32 {
        let idx = layer as usize;
        let internal = if self.pedals.pstick[idx] {
            self.pedals.bend[PedalSource::Global as usize]
        } else {
            0.0
        };
        let aux = if self.aux_kb.is_aux_layer(layer) {
            self.pedals.bend[PedalSource::Aux as usize]
        } else {
            0.0
        };
        (internal + aux).clamp(-1.0, 1.0)
    }

    fn apply_sustain(&mut self) {
        for layer in LayerId::ALL {
            let value = self.layer_sustain_value(layer);
            let down = value >= SUSTAIN_DOWN;
            if let Some(i) = self.extern_index(layer) {
                self.extern_forward_sustain(i, value);
                continue;
            }
            let half = !down && f32::from(value) / 127.0 >= HALF_PEDAL_MIN;
            match layer {
                LayerId::PianoA | LayerId::PianoB => {
                    let piano = if layer == LayerId::PianoA {
                        &mut self.piano_a
                    } else {
                        &mut self.piano_b
                    };
                    piano.set_damper_pedal(down, value);
                    piano.damper_half = half;
                }
                _ if !down => self.release_held(layer),
                _ => {}
            }
        }
    }

    fn apply_bend(&mut self) {
        for layer in LayerId::ALL {
            let norm = self.layer_bend_norm(layer);
            if let Some(i) = self.extern_index(layer) {
                self.extern_forward_bend(i, norm);
                continue;
            }
            let cents = norm * f32::from(self.pedals.pstick_range[layer as usize]) * 100.0;
            match layer {
                LayerId::SynthA => self.synth_a.bend_cents = cents,
                LayerId::SynthB => self.synth_b.bend_cents = cents,
                LayerId::SynthC => self.synth_c.bend_cents = cents,
                _ => {}
            }
        }
    }

    /// Note-off honouring the sustain pedal: Organ and Synth layers defer the release while their
    /// pedal is down (Pianos hold internally); Extern layers send the Note Off immediately.
    pub(super) fn release_or_hold(&mut self, layer: LayerId, note: u8) {
        if let Some(i) = self.extern_index(layer) {
            self.extern_note_off(i, note);
            return;
        }
        let holds = !matches!(layer, LayerId::PianoA | LayerId::PianoB)
            && self.layer_sustain_value(layer) >= SUSTAIN_DOWN;
        if holds {
            self.pedals.held[layer as usize] |= 1u128 << note.min(127);
        } else {
            self.layer_note_off(layer, note);
        }
    }

    /// A struck key cancels a deferred release of the same note.
    pub(super) fn clear_held(&mut self, layer: LayerId, note: u8) {
        self.pedals.held[layer as usize] &= !(1u128 << note.min(127));
    }

    fn release_held(&mut self, layer: LayerId) {
        let mask = std::mem::take(&mut self.pedals.held[layer as usize]);
        if mask == 0 {
            return;
        }
        for note in 0..128u8 {
            if mask & (1u128 << note) != 0 {
                self.layer_note_off(layer, note);
            }
        }
    }

    /// Applies Swell to the two Organ layer outputs; a no-op at the default full-swell position.
    pub(super) fn apply_organ_swell(&self, raw: &mut [[f32; 2]; 7]) {
        let gains = [
            self.pedals.organ_swell_gain(self.organ_a.model),
            self.pedals.organ_swell_gain(self.organ_b.model),
        ];
        for (frame, gain) in raw.iter_mut().zip(gains) {
            if gain != 1.0 {
                frame[0] *= gain;
                frame[1] *= gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_neutral() {
        let p = PedalState::default();
        assert_eq!(p.organ_swell_gain(OrganModel::B3), 1.0);
        assert_eq!(p.organ_swell_gain(OrganModel::Vox), 1.0);
        assert_eq!(p.volume_gain(), 1.0);
        assert_eq!(p.sustped, [false, false, true, true, false, false, false]);
        assert_eq!(p.pstick_range, [2; 7]);
        let e = Stage4Engine::new(48_000.0);
        assert_eq!(e.synth_a.bend_cents, 0.0);
    }

    #[test]
    fn swell_floors_b3_but_silences_other_models() {
        let mut p = PedalState {
            swell: 0.0,
            ..PedalState::default()
        };
        assert_eq!(p.organ_swell_gain(OrganModel::B3), B3_SWELL_FLOOR);
        assert_eq!(p.organ_swell_gain(OrganModel::Farfisa), 0.0);
        p.swell = 0.5;
        assert!(p.organ_swell_gain(OrganModel::B3) > 0.5);
    }
}
