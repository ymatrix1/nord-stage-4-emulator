//! Organ and Rotary edits applied on the DSP thread (one variant per engine-affecting panel
//! control that has no older `AudioCommand`).
//!
//! Handled in [`OrganEdit::apply`]; `AudioCommand::Organ` carries them. Layer indices are
//! `0 = Organ A`, anything else `Organ B`.

use nord_core::engine::Stage4Engine;
use nord_core::morph::LayerId;
use nord_core::organ::{clamp_stop_angle, OrganLayer, OrganModel, RotarySpeed, ScannerMode};

/// Panel edits for the Organ area.
#[derive(Debug, Clone, PartialEq)]
pub enum OrganEdit {
    /// Octave shift of one layer (`-2..=2`, manual p.18); host `organx_octave_{n}`.
    Octave { layer: u8, shift: i8 },
    /// ORGAN MODEL selector (six LEDs): model plus the B3 Bass flavour, both layers.
    Model { model: OrganModel, b3_bass: bool },
    /// Vibrato/Chorus type of one layer; `Off` when the layer's VIB/CHORUS ON is off.
    Scanner { layer: u8, mode: ScannerMode },
    /// Rotary speed button. With STOP MODE the slow position is the brake (manual p.53), so the
    /// engine brake flag is set only while the button sits on the slow/stop side.
    Rotary { fast: bool, stop_mode: bool },
    /// Rotary CLOSE MIC (both layers share the one speaker).
    CloseMic(bool),
    /// SUSTPED of one organ layer (Shift + Layer A button); host `ped_sustped_{n}`.
    Sustped { layer: u8, on: bool },
    /// PSTICK of one organ layer (Shift + Layer B button); host `ped_pstick_{n}`.
    Pstick { layer: u8, on: bool },
    /// POLY percussion (Shift + percussion VOLUME); both layers, host `organz_poly_perc_{n}`.
    PercPoly(bool),
    /// Preset (`false`) / Drawbar Live (`true`) of both layers; host `organz_drawbar_live_{n}`.
    DrawbarLive(bool),
    /// Physical drawbar positions (`0..=8` each) of both layers; host `organz_live1..9_{n}`.
    LiveDrawbars([u8; 9]),
    /// SYNC (Shift + Preset): copy the physical positions into the stored preset of both layers.
    SyncPreset,
    /// Rotary ORGAN button: route the organ through the rotary speaker; host `organy_rotary_on_{n}`.
    RotaryOrgan(bool),
    /// Rotary ANGLE (Shift + STOP MODE): wide or narrow microphone separation; host
    /// `organy_mic_angle_{n}`.
    RotaryAngle { wide: bool },
    /// Rotary stop ANGLE picker (Shift + STOP MODE, manual p.53): `0` = Free, `1..=6` = rest
    /// position of horn and rotor in Stop Mode; host `organy_stop_angle_{n}`.
    RotaryStopAngle(u8),
    /// KB ZONE of one layer as a set of the four zone LEDs (bit `u` = zone `u + 1`).
    Zone { layer: u8, leds: u8 },
    /// Aux KB LED of one layer: the layer is played by the aux keyboard.
    AuxKb { layer: u8, on: bool },
}

/// Rotary mic separation of the wide ANGLE position.
/// ASSUMED: the engine default `120.0` (`OrganLayer::new`, spec `organ-engine.md`); the manual
/// does not give degrees. A hardware measurement of the two ANGLE positions would confirm.
pub const ANGLE_WIDE_DEG: f32 = 120.0;
/// Rotary mic separation of the narrow ANGLE position. ASSUMED: half of the wide value.
pub const ANGLE_NARROW_DEG: f32 = 60.0;

fn layer_id(layer: u8) -> LayerId {
    if layer == 0 {
        LayerId::OrganA
    } else {
        LayerId::OrganB
    }
}

fn layer_mut(engine: &mut Stage4Engine, layer: u8) -> &mut OrganLayer {
    if layer == 0 {
        &mut engine.organ_a
    } else {
        &mut engine.organ_b
    }
}

impl OrganEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            OrganEdit::Octave { layer, shift } => {
                layer_mut(engine, layer).octave_shift = shift.clamp(-2, 2);
            }
            OrganEdit::Model { model, b3_bass } => {
                for l in [&mut engine.organ_a, &mut engine.organ_b] {
                    l.model = model;
                    l.b3_bass_mode = b3_bass;
                }
            }
            OrganEdit::Scanner { layer, mode } => {
                layer_mut(engine, layer).scanner_mode = mode;
            }
            OrganEdit::Rotary { fast, stop_mode } => {
                let speed = if fast {
                    RotarySpeed::Fast
                } else {
                    RotarySpeed::Slow
                };
                for l in [&mut engine.organ_a, &mut engine.organ_b] {
                    l.rotary_speed = speed;
                    l.rotary_stop_mode = stop_mode && !fast;
                }
            }
            OrganEdit::CloseMic(on) => {
                engine.organ_a.rotary_close_mic = on;
                engine.organ_b.rotary_close_mic = on;
            }
            OrganEdit::Sustped { layer, on } => {
                engine.pedals.sustped[usize::from(layer.min(1))] = on;
            }
            OrganEdit::Pstick { layer, on } => {
                engine.pedals.pstick[usize::from(layer.min(1))] = on;
            }
            OrganEdit::PercPoly(on) => {
                engine.organ_a.percussion_poly = on;
                engine.organ_b.percussion_poly = on;
            }
            OrganEdit::DrawbarLive(on) => {
                engine.organ_a.drawbar_live = on;
                engine.organ_b.drawbar_live = on;
            }
            OrganEdit::LiveDrawbars(bars) => {
                let bars = bars.map(|b| b.min(8));
                engine.organ_a.live_drawbars = bars;
                engine.organ_b.live_drawbars = bars;
            }
            OrganEdit::SyncPreset => {
                engine.sync_organ_preset(LayerId::OrganA);
                engine.sync_organ_preset(LayerId::OrganB);
            }
            OrganEdit::RotaryOrgan(on) => {
                engine.organ_a.rotary_enabled = on;
                engine.organ_b.rotary_enabled = on;
            }
            OrganEdit::RotaryStopAngle(angle) => {
                let angle = clamp_stop_angle(angle);
                engine.organ_a.rotary_stop_angle = angle;
                engine.organ_b.rotary_stop_angle = angle;
            }
            OrganEdit::RotaryAngle { wide } => {
                let deg = if wide {
                    ANGLE_WIDE_DEG
                } else {
                    ANGLE_NARROW_DEG
                };
                engine.organ_a.rotary_mic_angle_deg = deg;
                engine.organ_b.rotary_mic_angle_deg = deg;
            }
            OrganEdit::Zone { layer, leds } => engine.set_layer_zone_leds(layer_id(layer), leds),
            OrganEdit::AuxKb { layer, on } => engine.set_layer_aux(layer_id(layer), on),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        Stage4Engine::new(44_100.0)
    }

    #[test]
    fn octave_targets_one_layer_and_clamps() {
        let mut e = engine();
        OrganEdit::Octave { layer: 1, shift: 5 }.apply(&mut e);
        assert_eq!((e.organ_a.octave_shift, e.organ_b.octave_shift), (0, 2));
        OrganEdit::Octave {
            layer: 0,
            shift: -9,
        }
        .apply(&mut e);
        assert_eq!(e.organ_a.octave_shift, -2);
    }

    #[test]
    fn model_sets_both_layers_and_b3_bass() {
        let mut e = engine();
        OrganEdit::Model {
            model: OrganModel::Vox,
            b3_bass: true,
        }
        .apply(&mut e);
        assert_eq!(e.organ_a.model, OrganModel::Vox);
        assert!(e.organ_b.b3_bass_mode);
    }

    #[test]
    fn scanner_targets_one_layer() {
        let mut e = engine();
        OrganEdit::Scanner {
            layer: 1,
            mode: ScannerMode::V2,
        }
        .apply(&mut e);
        assert_eq!(e.organ_b.scanner_mode, ScannerMode::V2);
        assert_ne!(e.organ_a.scanner_mode, ScannerMode::V2);
    }

    #[test]
    fn rotary_brake_only_on_the_slow_side_in_stop_mode() {
        let mut e = engine();
        OrganEdit::Rotary {
            fast: false,
            stop_mode: true,
        }
        .apply(&mut e);
        assert!(e.organ_a.rotary_stop_mode && e.organ_b.rotary_stop_mode);
        assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Slow);
        OrganEdit::Rotary {
            fast: true,
            stop_mode: true,
        }
        .apply(&mut e);
        assert!(!e.organ_a.rotary_stop_mode);
        assert_eq!(e.organ_b.rotary_speed, RotarySpeed::Fast);
    }

    #[test]
    fn pedal_edits_target_one_organ_layer() {
        let mut e = engine();
        OrganEdit::Sustped { layer: 1, on: true }.apply(&mut e);
        OrganEdit::Pstick { layer: 0, on: true }.apply(&mut e);
        assert_eq!(e.pedals.sustped[..2], [false, true]);
        assert_eq!(e.pedals.pstick[..2], [true, false]);
    }

    #[test]
    fn close_mic_sets_both_layers() {
        let mut e = engine();
        OrganEdit::CloseMic(true).apply(&mut e);
        assert!(e.organ_a.rotary_close_mic && e.organ_b.rotary_close_mic);
    }

    #[test]
    fn poly_percussion_and_rotary_switches_reach_both_layers() {
        let mut e = engine();
        OrganEdit::PercPoly(true).apply(&mut e);
        assert!(e.organ_a.percussion_poly && e.organ_b.percussion_poly);
        OrganEdit::RotaryOrgan(false).apply(&mut e);
        assert!(!e.organ_a.rotary_enabled && !e.organ_b.rotary_enabled);
        OrganEdit::RotaryAngle { wide: false }.apply(&mut e);
        assert_eq!(e.organ_b.rotary_mic_angle_deg, ANGLE_NARROW_DEG);
        OrganEdit::RotaryAngle { wide: true }.apply(&mut e);
        assert_eq!(e.organ_a.rotary_mic_angle_deg, 120.0, "engine default");
    }

    #[test]
    fn stop_angle_reaches_both_layers_and_clamps() {
        let mut e = engine();
        OrganEdit::RotaryStopAngle(4).apply(&mut e);
        assert_eq!(
            (e.organ_a.rotary_stop_angle, e.organ_b.rotary_stop_angle),
            (4, 4)
        );
        OrganEdit::RotaryStopAngle(99).apply(&mut e);
        assert_eq!(e.organ_a.rotary_stop_angle, 6);
        OrganEdit::RotaryStopAngle(0).apply(&mut e);
        assert_eq!(e.organ_b.rotary_stop_angle, 0);
    }

    #[test]
    fn preset_live_and_sync_follow_the_engine_rules() {
        let mut e = engine();
        OrganEdit::DrawbarLive(true).apply(&mut e);
        assert!(e.organ_a.drawbar_live && e.organ_b.drawbar_live);
        OrganEdit::LiveDrawbars([1, 2, 3, 4, 5, 6, 7, 8, 99]).apply(&mut e);
        assert_eq!(
            e.organ_a.live_drawbars,
            [1, 2, 3, 4, 5, 6, 7, 8, 8],
            "clamped"
        );
        assert_eq!(e.organ_a.drawbars[0], 8, "live bars leave the preset alone");
        OrganEdit::SyncPreset.apply(&mut e);
        assert_eq!(e.organ_a.drawbars, e.organ_a.live_drawbars);
        assert_eq!(e.organ_b.drawbars, e.organ_b.live_drawbars);
    }

    #[test]
    fn zone_and_aux_edits_target_one_layer() {
        let mut e = engine();
        OrganEdit::Zone {
            layer: 1,
            leds: 0b1100,
        }
        .apply(&mut e);
        assert_eq!(e.layer_zone_leds(LayerId::OrganB), 0b1100);
        assert_eq!(e.layer_zone_leds(LayerId::OrganA), 0b1111);
        OrganEdit::AuxKb { layer: 0, on: true }.apply(&mut e);
        assert!(e.aux_kb.is_aux_layer(LayerId::OrganA));
        assert!(!e.aux_kb.is_aux_layer(LayerId::OrganB));
    }
}
