//! Per-layer Synth LFO, Arpeggiator and Vibrato parameters (Phase 6b B9a,
//! `ns4-4w9l`; manual p.34-38).
//!
//! IDs are additive: `synthm_<name>_1..3` for Synth A, B and C. Defaults equal
//! `SynthLayer::new`, so an untouched host leaves sounds unchanged.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::synth_types::{ArpMode, LfoDestination, LfoShape, VibratoMode};

use crate::synth_params::SYNTH_LAYERS;

pub(crate) fn shape(i: i32) -> LfoShape {
    match i {
        1 => LfoShape::Saw,
        2 => LfoShape::Square,
        3 => LfoShape::SampleAndHold,
        4 => LfoShape::SawDown,
        _ => LfoShape::Triangle,
    }
}

pub(crate) fn dest(i: i32) -> LfoDestination {
    match i {
        1 => LfoDestination::OscPitch,
        2 => LfoDestination::OscCtrl,
        3 => LfoDestination::Off,
        _ => LfoDestination::FilterCutoff,
    }
}

pub(crate) fn arp(i: i32) -> ArpMode {
    match i {
        1 => ArpMode::Up,
        2 => ArpMode::Down,
        3 => ArpMode::UpDown,
        4 => ArpMode::Random,
        5 => ArpMode::Poly,
        _ => ArpMode::Off,
    }
}

pub(crate) fn vibrato(i: i32) -> VibratoMode {
    match i {
        1 => VibratoMode::On,
        2 => VibratoMode::Delay,
        3 => VibratoMode::Wheel,
        4 => VibratoMode::Aftertouch,
        5 => VibratoMode::Pedal,
        _ => VibratoMode::Off,
    }
}

/// LFO, Arp and Vibrato parameters of one synth layer.
#[derive(Params)]
pub struct SynthModParams {
    /// 0 Triangle, 1 Saw (ramp up), 2 Square, 3 Sample & Hold, 4 Saw down (manual p.35).
    #[id = "lfo_shape"]
    pub lfo_shape: IntParam,
    /// 0 Filter, 1 Pitch, 2 Osc Ctrl, 3 Off (manual p.35).
    #[id = "lfo_dest"]
    pub lfo_dest: IntParam,
    /// LFO rate in Hz (range `ASSUMED`, default equals the engine).
    #[id = "lfo_rate"]
    pub lfo_rate: FloatParam,
    /// LFO amount `0..1`.
    #[id = "lfo_amount"]
    pub lfo_amount: FloatParam,
    /// 0 Off, 1 Up, 2 Down, 3 Up/Down, 4 Random, 5 Poly (manual p.35-36).
    #[id = "arp_mode"]
    pub arp_mode: IntParam,
    /// Arpeggio range in octaves, 1..=4 (manual p.36).
    #[id = "arp_range"]
    pub arp_range: IntParam,
    /// Arpeggiator rate as quarter-note BPM (manual p.37).
    #[id = "arp_rate"]
    pub arp_rate: FloatParam,
    /// 0 Off, 1 On, 2 Delay, 3 Wheel, 4 Aftertouch, 5 Pedal (manual p.37).
    #[id = "vib_mode"]
    pub vib_mode: IntParam,
    /// Vibrato rate in Hz, 2.0-8.0 (manual p.37).
    #[id = "vib_rate"]
    pub vib_rate: FloatParam,
    /// Vibrato depth in cents (manual gives Amount 0-10; the cents scale is `ASSUMED`).
    #[id = "vib_depth"]
    pub vib_depth: FloatParam,
    /// Vibrato delay in seconds for Delay mode (manual: 0.2, 0.7, 1.2, 2.0 s).
    #[id = "vib_delay"]
    pub vib_delay: FloatParam,
}

impl Default for SynthModParams {
    fn default() -> Self {
        let int =
            |name, default, min, max| IntParam::new(name, default, IntRange::Linear { min, max });
        Self {
            lfo_shape: int("LFO Shape", 0, 0, 4),
            lfo_dest: int("LFO Dest", 0, 0, 3),
            lfo_rate: FloatParam::new(
                "LFO Rate",
                3.5,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 40.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz"),
            lfo_amount: FloatParam::new(
                "LFO Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            arp_mode: int("Arp Mode", 0, 0, 5),
            arp_range: int("Arp Range", 1, 1, 4),
            arp_rate: FloatParam::new(
                "Arp Rate",
                120.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 300.0,
                },
            )
            .with_unit(" BPM"),
            vib_mode: int("Vibrato Mode", 0, 0, 5),
            vib_rate: FloatParam::new(
                "Vibrato Rate",
                5.0,
                FloatRange::Linear { min: 2.0, max: 8.0 },
            )
            .with_unit(" Hz"),
            vib_depth: FloatParam::new(
                "Vibrato Depth",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" cents"),
            vib_delay: FloatParam::new(
                "Vibrato Delay",
                0.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" s"),
        }
    }
}

/// All three synth layers.
#[derive(Params)]
pub struct SynthModBank {
    #[nested(array, group = "Synth Mod")]
    pub layers: [SynthModParams; SYNTH_LAYERS],
}

impl Default for SynthModBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| SynthModParams::default()),
        }
    }
}

impl SynthModBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ];
        for (p, t) in self.layers.iter().zip(targets) {
            t.lfo_shape = shape(p.lfo_shape.value());
            t.lfo_dest = dest(p.lfo_dest.value());
            t.lfo_rate_hz = p.lfo_rate.value();
            t.lfo_amount = p.lfo_amount.value();
            t.arp_mode = arp(p.arp_mode.value());
            t.arp_octave_range = p.arp_range.value() as u8;
            t.arp_rate_bpm = p.arp_rate.value();
            t.vibrato_mode = vibrato(p.vib_mode.value());
            t.vibrato_rate_hz = p.vib_rate.value();
            t.vibrato_depth_cents = p.vib_depth.value();
            t.vibrato_delay_sec = p.vib_delay.value();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: i32, max: i32) -> IntParam {
        IntParam::new("t", v, IntRange::Linear { min: 0, max })
    }

    #[test]
    fn index_maps_cover_every_engine_variant() {
        assert_eq!(shape(4), LfoShape::SawDown);
        assert_eq!(shape(3), LfoShape::SampleAndHold);
        assert_eq!(dest(3), LfoDestination::Off);
        assert_eq!(dest(2), LfoDestination::OscCtrl);
        assert_eq!(arp(5), ArpMode::Poly);
        assert_eq!(arp(4), ArpMode::Random);
        assert_eq!(vibrato(5), VibratoMode::Pedal);
        assert_eq!(vibrato(2), VibratoMode::Delay);
        assert_eq!(shape(99), LfoShape::Triangle);
        assert_eq!(arp(0), ArpMode::Off);
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44100.0);
        let mut e = Stage4Engine::new(44100.0);
        SynthModBank::default().sync_to_engine(&mut e);
        for (a, b) in [
            (&e.synth_a, &reference.synth_a),
            (&e.synth_c, &reference.synth_c),
        ] {
            assert_eq!(a.lfo_shape, b.lfo_shape);
            assert_eq!(a.lfo_dest, b.lfo_dest);
            assert_eq!(a.lfo_rate_hz, b.lfo_rate_hz);
            assert_eq!(a.lfo_amount, b.lfo_amount);
            assert_eq!(a.arp_mode, b.arp_mode);
            assert_eq!(a.arp_octave_range, b.arp_octave_range);
            assert_eq!(a.arp_rate_bpm, b.arp_rate_bpm);
            assert_eq!(a.vibrato_mode, b.vibrato_mode);
            assert_eq!(a.vibrato_rate_hz, b.vibrato_rate_hz);
            assert_eq!(a.vibrato_depth_cents, b.vibrato_depth_cents);
            assert_eq!(a.vibrato_delay_sec, b.vibrato_delay_sec);
        }
    }

    #[test]
    fn values_reach_only_their_layer() {
        let mut bank = SynthModBank::default();
        bank.layers[1].arp_mode = int(3, 5);
        bank.layers[1].lfo_shape = int(2, 4);
        bank.layers[1].vib_mode = int(4, 5);
        let mut e = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.synth_b.arp_mode, ArpMode::UpDown);
        assert_eq!(e.synth_b.lfo_shape, LfoShape::Square);
        assert_eq!(e.synth_b.vibrato_mode, VibratoMode::Aftertouch);
        assert_eq!(e.synth_a.arp_mode, ArpMode::Off);
    }

    fn render_with(bank: &SynthModBank) -> Vec<[f32; 2]> {
        let mut e = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut e);
        e.synth_a.note_on(60, 100);
        (0..8192).map(|_| e.synth_a.render_frame()).collect()
    }

    #[test]
    fn lfo_on_pitch_changes_the_rendered_signal() {
        let off = render_with(&SynthModBank::default());
        let mut bank = SynthModBank::default();
        bank.layers[0].lfo_dest = int(1, 3);
        bank.layers[0].lfo_amount =
            FloatParam::new("a", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 });
        let on = render_with(&bank);
        assert!(off
            .iter()
            .chain(&on)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(off.iter().any(|f| f[0].abs() > 1e-4), "synth is silent");
        assert!(off != on, "LFO to pitch renders identically to LFO off");
    }

    #[test]
    fn vibrato_on_changes_the_rendered_signal() {
        let off = render_with(&SynthModBank::default());
        let mut bank = SynthModBank::default();
        bank.layers[0].vib_mode = int(1, 5);
        bank.layers[0].vib_depth = FloatParam::new(
            "d",
            50.0,
            FloatRange::Linear {
                min: 0.0,
                max: 100.0,
            },
        );
        let on = render_with(&bank);
        assert!(off != on, "vibrato renders identically to off");
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = SynthModBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 11 * SYNTH_LAYERS);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
