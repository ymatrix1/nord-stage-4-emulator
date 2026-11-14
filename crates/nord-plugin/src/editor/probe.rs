//! One representative `AudioCommand` per mapping arm.
//!
//! The probe is the single source of truth for which host parameter families the editor can
//! write: [`mapped_families`] runs `map_command` over it, so the dimming decision
//! (`editor::coverage`) can never drift from the mapper. If a mapping arm is added without a
//! probe command, its controls are merely dimmed, never wrongly drawn as live; the test
//! `probe_covers_every_fx_float_and_flag` pins the table-driven families.

use super::map::map_command;
use nord_core::engine::FxSharing;
use nord_core::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, Mod1Type, Mod2Type, ReverbColor, ReverbModel,
};
use nord_core::midi::{cc, MidiEvent};
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::{PatternPan, SynthOscMode};
use nord_core::synth_types::{ArpMode, LfoDestination, LfoShape, NotePriority, VibratoMode};
use nord_gui::audio::{
    AudioCommand as C, FxEdit, FxFlag, FxFloat, OrganEdit, PatternParam, PianoEdit, ProgramEdit,
    SettingsEdit, SynthEdit, SynthParam, SynthTarget,
};
use nord_panel::menus::Value;
use std::collections::BTreeSet;

/// Plausible in-range plain value for each FX float (used by the unit round-trip test too).
pub fn probe_fx_value(p: FxFloat) -> f32 {
    match p {
        FxFloat::Mod1Rate => 4.0,
        FxFloat::Mod1Amount => 0.5,
        FxFloat::Mod1PedPos => 0.5,
        FxFloat::Mod2Rate => 1.0,
        FxFloat::Mod2Amount => 0.5,
        FxFloat::Drive => 0.5,
        FxFloat::EqBass => 3.0,
        FxFloat::EqMid => -2.0,
        FxFloat::EqMidFreq => 1000.0,
        FxFloat::EqTreble => 4.0,
        // The host range is 0..1 while the engine accepts 0..10 (`FxFloat::set`); values above 1
        // clamp at the host (known limit of `fx_extra_params`, noted in the editor spec).
        FxFloat::FilterRes => 0.5,
        FxFloat::CompAmount => 0.5,
        FxFloat::DelayTime => 400.0,
        FxFloat::DelayFeedback => 0.4,
        FxFloat::DelayMix => 0.3,
        FxFloat::ReverbMix => 0.3,
    }
}

/// Every probe command.
pub fn probe_commands() -> Vec<C> {
    let sp = |p: SynthParam| C::Synth(SynthEdit::Param(SynthTarget::Layer(1), p));
    let mut v = vec![
        C::SetMasterVolume(0.5),
        C::SetMasterComp(0.5),
        C::SetDrawbars([8, 7, 6, 5, 4, 3, 2, 1, 0]),
        C::SetOrganModel(OrganModel::Vox),
        C::SetPercussion {
            enabled: true,
            soft: false,
            fast: true,
            third: false,
        },
        C::SetRotarySpeed(RotarySpeed::Fast),
        C::SetRotaryDrive(0.5),
        C::SetOrganLevel(0.5),
        C::SetOrganBLevel(0.5),
        C::SetPianoLevel(0.5),
        C::SetPianoBLevel(0.5),
        C::SetPianoCategory(PianoCategory::Electric),
        C::SetPianoTimbre(PianoTimbre::Bright),
        C::SetPianoTouch(VelocityTouch::Heavy),
        C::SetPianoAcoustics {
            string_res: true,
            pedal_noise: false,
        },
        C::SetSynthLevel(0.5),
        C::SetSynthBLevel(0.5),
        C::SetSynthCLevel(0.5),
        C::SetSynthOscMode(SynthOscMode::Fm),
        C::SetSynthCutoff(1200.0),
        C::SetSynthResonance(0.4),
        C::SetSynthDrive(0.3),
        C::SetSynthAttack(0.2),
        C::SetSynthDecay(0.3),
        C::SetSynthRelease(0.4),
        C::Midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::MOD_WHEEL,
            value: 64,
        }),
        C::Organ(OrganEdit::Octave { layer: 0, shift: 1 }),
        C::Organ(OrganEdit::Model {
            model: OrganModel::Farfisa,
            b3_bass: true,
        }),
        C::Organ(OrganEdit::Scanner {
            layer: 1,
            mode: ScannerMode::V2,
        }),
        C::Organ(OrganEdit::Rotary {
            fast: true,
            stop_mode: true,
        }),
        C::Organ(OrganEdit::CloseMic(true)),
        C::Organ(OrganEdit::PercPoly(true)),
        C::Organ(OrganEdit::DrawbarLive(true)),
        C::Organ(OrganEdit::LiveDrawbars([8, 7, 6, 5, 4, 3, 2, 1, 0])),
        C::Organ(OrganEdit::RotaryOrgan(false)),
        C::Organ(OrganEdit::RotaryAngle { wide: true }),
        C::Organ(OrganEdit::RotaryStopAngle(3)),
        C::Organ(OrganEdit::Sustped { layer: 0, on: true }),
        C::Organ(OrganEdit::Pstick { layer: 1, on: true }),
        C::Piano(PianoEdit::Level {
            layer: 0,
            level: 0.5,
        }),
        C::Piano(PianoEdit::LayerActive { layer: 1, on: true }),
        C::Piano(PianoEdit::Category(PianoCategory::Clav)),
        C::Piano(PianoEdit::Timbre(PianoTimbre::Mid)),
        C::Piano(PianoEdit::Touch(VelocityTouch::Light)),
        C::Piano(PianoEdit::StringRes(false)),
        C::Piano(PianoEdit::PedalNoise(true)),
        C::Piano(PianoEdit::SoftRelease { layer: 0, on: true }),
        C::Piano(PianoEdit::Unison {
            layer: 1,
            amount: 2,
        }),
        C::Piano(PianoEdit::DynComp {
            layer: 0,
            amount: 3,
        }),
        C::Piano(PianoEdit::OctaveShift {
            layer: 1,
            octaves: -1,
        }),
        C::Piano(PianoEdit::Sustped(true)),
        C::Piano(PianoEdit::Pstick(true)),
        sp(SynthParam::OscMode(SynthOscMode::Wavetable)),
        sp(SynthParam::FilterDrive(0.3)),
        sp(SynthParam::Cutoff(800.0)),
        sp(SynthParam::Resonance(0.3)),
        sp(SynthParam::AmpAttack(0.1)),
        sp(SynthParam::AmpDecay(0.2)),
        sp(SynthParam::AmpRelease(0.3)),
        sp(SynthParam::KbTrack(2)),
        sp(SynthParam::FilterEnvAmount(0.5)),
        sp(SynthParam::OscEnvAmount(0.4)),
        sp(SynthParam::OscEnvToPitch(true)),
        sp(SynthParam::Voice(1)),
        sp(SynthParam::Glide(0.5)),
        sp(SynthParam::Unison(2)),
        sp(SynthParam::Priority(NotePriority::Hi)),
        sp(SynthParam::LfoRate(5.0)),
        sp(SynthParam::LfoAmount(0.5)),
        sp(SynthParam::ArpRate(120.0)),
        sp(SynthParam::ArpRange(2)),
        sp(SynthParam::VibratoRate(5.0)),
        sp(SynthParam::VibratoDepth(30.0)),
        sp(SynthParam::VibratoDelay(0.2)),
        sp(SynthParam::KbHold(true)),
        sp(SynthParam::LfoDivision(3)),
        sp(SynthParam::ArpDivision(2)),
        sp(SynthParam::GateEnv(0.5)),
        sp(SynthParam::PatternPreset(3)),
        sp(SynthParam::Pattern(PatternParam::Zigzag(true))),
        sp(SynthParam::Pattern(PatternParam::RangeThirds(1))),
        sp(SynthParam::Pattern(PatternParam::Custom(true))),
        sp(SynthParam::Pattern(PatternParam::Len(10))),
        sp(SynthParam::Pattern(PatternParam::Gate {
            step: 4,
            eighths: 6,
        })),
        sp(SynthParam::Pattern(PatternParam::Accent {
            step: 4,
            on: true,
        })),
        sp(SynthParam::Pattern(PatternParam::Pan {
            step: 4,
            pan: PatternPan::Left,
        })),
        C::Synth(SynthEdit::Sustped(true)),
        C::Synth(SynthEdit::Pstick(true)),
        sp(SynthParam::KbHoldExclude(true)),
        sp(SynthParam::KbSync(true)),
        sp(SynthParam::LfoClockSync(true)),
        sp(SynthParam::ArpClockSync(true)),
        sp(SynthParam::PatternOn(true)),
        sp(SynthParam::LfoShape(LfoShape::Square)),
        sp(SynthParam::LfoDest(LfoDestination::OscPitch)),
        sp(SynthParam::ArpMode(ArpMode::UpDown)),
        sp(SynthParam::VibratoMode(VibratoMode::Delay)),
    ];
    for (id, value) in [
        ("aux_kb.1.enable", Value::Bool(true)),
        ("aux_kb.2.organ", Value::Index(1)),
        ("aux_kb.2.piano", Value::Index(2)),
        ("aux_kb.2.synth", Value::Index(4)),
        ("aux_kb.3.curve", Value::Index(2)),
    ] {
        v.push(C::Program(ProgramEdit::Settings(SettingsEdit {
            id,
            value,
        })));
    }
    for i in 0..7 {
        v.push(C::SetLayerActive(i, true));
    }
    let slot = 2u8;
    for p in FxFloat::ALL {
        v.push(C::Fx(FxEdit::Float {
            slot,
            param: p,
            value: probe_fx_value(p),
        }));
    }
    for p in FxFlag::ALL {
        v.push(C::Fx(FxEdit::Flag {
            slot,
            param: p,
            on: true,
        }));
    }
    v.extend([
        C::Fx(FxEdit::Mod1Type {
            slot,
            kind: Mod1Type::Tremolo,
        }),
        C::Fx(FxEdit::Mod2Type {
            slot,
            kind: Mod2Type::Phaser,
        }),
        C::Fx(FxEdit::Cabinet {
            slot,
            cabinet: AmpCabinet::Twin,
        }),
        C::Fx(FxEdit::Mod1Sync {
            slot,
            beats: Some(1.0),
        }),
        C::Fx(FxEdit::Sharing(FxSharing {
            delay_global: Some(2),
            piano_group: Some(2),
            synth_group: Some(4),
            ..FxSharing::default()
        })),
        C::Fx(FxEdit::DelaySync {
            slot,
            beats: Some(0.5),
        }),
        C::Fx(FxEdit::DelayFilter {
            slot,
            mode: DelayFilterMode::Lp,
        }),
        C::Fx(FxEdit::DelayFx {
            slot,
            fx: DelayFeedbackFx::Chorus,
        }),
        C::Fx(FxEdit::ReverbModel {
            slot,
            model: ReverbModel::Hall,
        }),
        C::Fx(FxEdit::ReverbColor {
            slot,
            color: ReverbColor::Bright,
        }),
    ]);
    v
}

/// Host parameter families the editor can write (`fx_drive`, `master_level`, ...).
pub fn mapped_families() -> BTreeSet<&'static str> {
    let mut out = Vec::new();
    let mut fams = BTreeSet::new();
    for cmd in probe_commands() {
        out.clear();
        map_command(&cmd, &mut out);
        fams.extend(out.iter().map(|w| w.family));
    }
    fams
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::index::ParamIndex;
    use crate::params::Stage4Params;

    #[test]
    fn probe_covers_every_fx_float_and_flag() {
        let fams = mapped_families();
        let mut n = 0;
        for p in FxFloat::ALL {
            let mut out = Vec::new();
            map_command(
                &C::Fx(FxEdit::Float {
                    slot: 2,
                    param: p,
                    value: 0.1,
                }),
                &mut out,
            );
            assert_eq!(out.len(), 1, "{p:?}");
            assert!(fams.contains(out[0].family), "{p:?}");
            n += 1;
        }
        for p in FxFlag::ALL {
            let mut out = Vec::new();
            map_command(
                &C::Fx(FxEdit::Flag {
                    slot: 2,
                    param: p,
                    on: true,
                }),
                &mut out,
            );
            assert_eq!(out.len(), 1, "{p:?}");
            assert!(fams.contains(out[0].family), "{p:?}");
            n += 1;
        }
        assert_eq!(n, FxFloat::ALL.len() + FxFlag::ALL.len());
    }

    #[test]
    fn every_probe_command_is_understood_and_every_write_hits_a_real_param() {
        let params = Stage4Params::default();
        let idx = ParamIndex::new(&params);
        for cmd in probe_commands() {
            let mut out = Vec::new();
            assert!(map_command(&cmd, &mut out), "{cmd:?} not understood");
            for w in out {
                assert!(
                    idx.contains(&w.host_id()),
                    "{cmd:?} writes unknown param {}",
                    w.host_id()
                );
            }
        }
    }

    /// Writing a probe value must land on (nearly) the same plain value: catches unit or range
    /// mismatches between the panel's engine units and the host parameter range.
    #[test]
    fn probe_values_survive_the_plain_normalized_plain_round_trip() {
        let params = Stage4Params::default();
        let idx = ParamIndex::new(&params);
        let mut checked = 0;
        for cmd in probe_commands() {
            let mut out = Vec::new();
            map_command(&cmd, &mut out);
            for w in out {
                let ptr = idx.ptr(&w.host_id()).unwrap();
                // SAFETY: `params` outlives `ptr` within this test.
                let back = unsafe { ptr.preview_plain(ptr.preview_normalized(w.plain)) };
                let tol = (0.02_f32).max(0.02 * w.plain.abs());
                assert!(
                    (back - w.plain).abs() <= tol,
                    "{}: wrote {} but the host holds {back} ({cmd:?})",
                    w.host_id(),
                    w.plain
                );
                checked += 1;
            }
        }
        assert!(checked > 120, "only {checked} writes checked");
    }
}
