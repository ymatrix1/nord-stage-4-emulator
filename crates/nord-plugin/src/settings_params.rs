//! System / Sound / Output menu host parameters (`ns4-yfht`; manual OS v1.6x p.57-59).
//!
//! IDs are additive under the `sys` prefix (`sys_transpose`, `sys_fine_tune`, ...). Defaults equal
//! `GlobalSettings::default()`, so an untouched host renders exactly as before. Program Levels
//! (Sound page 1) is the existing `prog_level_db`; the per-program transpose stays `prog_transpose`.

use nih_plug::prelude::*;
use nord_core::engine::{
    GlobalSettings, Level3, Stage4Engine, TonewheelMode, FINE_TUNE_RANGE_CENTS,
    GLOBAL_TRANSPOSE_RANGE, PIANO_LEVEL_DB_RANGE,
};

fn level3(name: &str) -> IntParam {
    IntParam::new(name, 1, IntRange::Linear { min: 0, max: 2 })
}

fn db(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -PIANO_LEVEL_DB_RANGE,
            max: PIANO_LEVEL_DB_RANGE,
        },
    )
    .with_unit(" dB")
    .with_step_size(0.5)
}

fn to_level3(v: i32) -> Level3 {
    match v {
        0 => Level3::Low,
        2 => Level3::High,
        _ => Level3::Normal,
    }
}

/// Global menu settings the engine honours.
#[derive(Params)]
pub struct SettingsParams {
    /// System 2 Global Transpose, semitones.
    #[id = "transpose"]
    pub transpose: IntParam,
    /// System 3 Fine Tune, cents.
    #[id = "fine_tune"]
    pub fine_tune: FloatParam,
    /// Sound 2 Piano Pedal Noise Level, dB.
    #[id = "piano_noise_db"]
    pub piano_noise_db: FloatParam,
    /// Sound 3 Piano String Res Level, dB.
    #[id = "piano_res_db"]
    pub piano_res_db: FloatParam,
    /// Sound 5 B3 Tonewheel Mode: 0 Clean, 1 Vintage 1, 2 Vintage 2.
    #[id = "tonewheel"]
    pub tonewheel: IntParam,
    /// Sound 6 B3 Click Level: off Normal, on High.
    #[id = "click_high"]
    pub click_high: BoolParam,
    /// Sound 7 Rotary Bass/Horn Level: 0 = 70/30, 1 = 60/40, 2 = 50/50, 3 = 40/60, 4 = 30/70.
    #[id = "rotary_balance"]
    pub rotary_balance: IntParam,
    /// Sound 8 Rotary Rotor Speed: 0 Low, 1 Normal, 2 High.
    #[id = "rotor_speed"]
    pub rotor_speed: IntParam,
    /// Sound 9 Rotary Rotor Acceleration: 0 Low, 1 Normal, 2 High.
    #[id = "rotor_acc"]
    pub rotor_acc: IntParam,
    /// Sound 10 Rotary Horn Speed: 0 Low, 1 Normal, 2 High.
    #[id = "horn_speed"]
    pub horn_speed: IntParam,
    /// Sound 11 Rotary Horn Acceleration: 0 Low, 1 Normal, 2 High.
    #[id = "horn_acc"]
    pub horn_acc: IntParam,
    /// Output: mono mix on both channels.
    #[id = "mono"]
    pub mono: BoolParam,
}

impl Default for SettingsParams {
    fn default() -> Self {
        let t = i32::from(GLOBAL_TRANSPOSE_RANGE);
        Self {
            transpose: IntParam::new("Global Transpose", 0, IntRange::Linear { min: -t, max: t })
                .with_unit(" st"),
            fine_tune: FloatParam::new(
                "Fine Tune",
                0.0,
                FloatRange::Linear {
                    min: -FINE_TUNE_RANGE_CENTS,
                    max: FINE_TUNE_RANGE_CENTS,
                },
            )
            .with_unit(" ct")
            .with_step_size(1.0),
            piano_noise_db: db("Piano Pedal Noise Level"),
            piano_res_db: db("Piano String Res Level"),
            tonewheel: IntParam::new("B3 Tonewheel Mode", 1, IntRange::Linear { min: 0, max: 2 }),
            click_high: BoolParam::new("B3 Click High", false),
            rotary_balance: IntParam::new(
                "Rotary Bass/Horn Level",
                2,
                IntRange::Linear { min: 0, max: 4 },
            ),
            rotor_speed: level3("Rotary Rotor Speed"),
            rotor_acc: level3("Rotary Rotor Acc"),
            horn_speed: level3("Rotary Horn Speed"),
            horn_acc: level3("Rotary Horn Acc"),
            mono: BoolParam::new("Mono Output", false),
        }
    }
}

impl SettingsParams {
    /// The engine settings these parameters describe.
    pub fn to_settings(&self) -> GlobalSettings {
        GlobalSettings {
            transpose: self.transpose.value() as i8,
            fine_tune_cents: self.fine_tune.value(),
            piano_pedal_noise_db: self.piano_noise_db.value(),
            piano_string_res_db: self.piano_res_db.value(),
            tonewheel_mode: match self.tonewheel.value() {
                0 => TonewheelMode::Clean,
                2 => TonewheelMode::Vintage2,
                _ => TonewheelMode::Vintage1,
            },
            organ_click_high: self.click_high.value(),
            rotary_bass_percent: (70 - 10 * self.rotary_balance.value().clamp(0, 4)) as u8,
            rotor_speed: to_level3(self.rotor_speed.value()),
            rotor_acc: to_level3(self.rotor_acc.value()),
            horn_speed: to_level3(self.horn_speed.value()),
            horn_acc: to_level3(self.horn_acc.value()),
            mono_output: self.mono.value(),
        }
    }

    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.set_global_settings(self.to_settings());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;

    fn render(p: Option<&SettingsParams>) -> Vec<[f32; 2]> {
        let mut e = Stage4Engine::new(44100.0);
        if let Some(p) = p {
            p.sync_to_engine(&mut e);
        }
        for n in [48u8, 60, 67] {
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: n,
                velocity: 100,
            });
        }
        (0..4096).map(|_| e.render_frame()).collect()
    }

    #[test]
    fn defaults_equal_engine_defaults() {
        assert_eq!(
            SettingsParams::default().to_settings(),
            GlobalSettings::default()
        );
    }

    #[test]
    fn default_host_render_is_bit_identical_to_an_unsynced_engine() {
        assert_eq!(render(None), render(Some(&SettingsParams::default())));
    }

    #[test]
    fn every_value_maps_to_its_engine_field() {
        let p = SettingsParams {
            transpose: IntParam::new("t", -4, IntRange::Linear { min: -6, max: 6 }),
            fine_tune: FloatParam::new(
                "f",
                12.0,
                FloatRange::Linear {
                    min: -50.0,
                    max: 50.0,
                },
            ),
            tonewheel: IntParam::new("w", 0, IntRange::Linear { min: 0, max: 2 }),
            click_high: BoolParam::new("c", true),
            rotary_balance: IntParam::new("b", 0, IntRange::Linear { min: 0, max: 4 }),
            rotor_speed: IntParam::new("rs", 2, IntRange::Linear { min: 0, max: 2 }),
            rotor_acc: IntParam::new("ra", 0, IntRange::Linear { min: 0, max: 2 }),
            horn_speed: IntParam::new("hs", 0, IntRange::Linear { min: 0, max: 2 }),
            horn_acc: IntParam::new("ha", 2, IntRange::Linear { min: 0, max: 2 }),
            mono: BoolParam::new("m", true),
            ..SettingsParams::default()
        };
        let s = p.to_settings();
        assert_eq!(s.transpose, -4);
        assert_eq!(s.fine_tune_cents, 12.0);
        assert_eq!(s.tonewheel_mode, TonewheelMode::Clean);
        assert!(s.organ_click_high && s.mono_output);
        assert_eq!(s.rotary_bass_percent, 70);
        assert_eq!(s.rotor_speed, Level3::High);
        assert_eq!(s.rotor_acc, Level3::Low);
        assert_eq!(s.horn_speed, Level3::Low);
        assert_eq!(s.horn_acc, Level3::High);
        let mut e = Stage4Engine::new(44100.0);
        p.sync_to_engine(&mut e);
        assert_eq!(e.global_settings(), s);
        assert_ne!(render(Some(&p)), render(None));
    }

    #[test]
    fn balance_index_covers_the_manual_choices() {
        let percents: Vec<u8> = (0..5)
            .map(|i| {
                SettingsParams {
                    rotary_balance: IntParam::new("b", i, IntRange::Linear { min: 0, max: 4 }),
                    ..SettingsParams::default()
                }
                .to_settings()
                .rotary_bass_percent
            })
            .collect();
        assert_eq!(percents, vec![70, 60, 50, 40, 30]);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = SettingsParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 12);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
