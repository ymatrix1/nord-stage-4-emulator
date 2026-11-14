//! Mapping of panel `AudioCommand`s to host parameter writes (the plugin side of the panel sink).
//!
//! `map_command` is pure: it never touches the engine or the host, so the tests drive it with
//! every command shape and check the resulting ids and plain values. Commands without a host
//! parameter map to nothing; the matching controls are dimmed in the editor
//! (`editor::coverage`).

use super::map_area::{map_organ, map_piano};
use super::map_fx::map_fx;
use super::map_synth::map_synth;
use super::write::ParamWrite as W;
use nih_plug::prelude::Enum;
use nord_core::midi::{cc, MidiEvent};
use nord_core::organ::{OrganModel, RotarySpeed};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::SynthOscMode;
use nord_gui::audio::AudioCommand;
use std::fmt::Debug;

/// Drawbar host ids, 16' first (same order as `AudioCommand::SetDrawbars`).
pub const DRAWBARS_A: [&str; 9] = [
    "drawbar_1_16",
    "drawbar_2_5_1_3",
    "drawbar_3_8",
    "drawbar_4_4",
    "drawbar_5_2_2_3",
    "drawbar_6_2",
    "drawbar_7_1_3_5",
    "drawbar_8_1_1_3",
    "drawbar_9_1",
];

/// Host ids of the seven layer enable switches (Organ A/B, Piano A/B, Synth A/B/C).
pub const LAYER_ENABLED: [&str; 7] = [
    "organ_a_enabled",
    "organ_b_enabled",
    "piano_a_enabled",
    "piano_b_enabled",
    "synth_a_enabled",
    "synth_b_enabled",
    "synth_c_enabled",
];

pub(super) fn flag(on: bool) -> f32 {
    if on {
        1.0
    } else {
        0.0
    }
}

/// Index of the variant of host enum `S` whose `Debug` name equals that of `core`.
///
/// The host mirror enums (`fx_selectors`) keep the core variant names, so a name match is exact.
pub(super) fn enum_index<S: Enum + Debug>(core: &impl Debug) -> Option<f32> {
    let name = format!("{core:?}");
    (0..S::variants().len())
        .find(|&i| format!("{:?}", S::from_index(i)) == name)
        .map(|i| i as f32)
}

pub(super) fn organ_model_index(m: OrganModel) -> f32 {
    match m {
        OrganModel::B3 => 0.0,
        OrganModel::Vox => 1.0,
        OrganModel::Farfisa => 2.0,
        OrganModel::Pipe1 => 3.0,
        OrganModel::Pipe2 => 4.0,
    }
}

pub(super) fn piano_category_index(c: PianoCategory) -> f32 {
    match c {
        PianoCategory::Grand => 0.0,
        PianoCategory::Upright => 1.0,
        PianoCategory::Electric => 2.0,
        PianoCategory::Clav => 3.0,
        PianoCategory::Digital => 4.0,
        PianoCategory::Misc => 5.0,
    }
}

/// Host timbre index (Soft 0 .. Dyno2 4); `Off` has no host value.
pub(super) fn piano_timbre_index(t: PianoTimbre) -> Option<f32> {
    match t {
        PianoTimbre::Off => None,
        PianoTimbre::Soft => Some(0.0),
        PianoTimbre::Mid => Some(1.0),
        PianoTimbre::Bright => Some(2.0),
        PianoTimbre::Dyno1 => Some(3.0),
        PianoTimbre::Dyno2 => Some(4.0),
    }
}

pub(super) fn piano_touch_index(t: VelocityTouch) -> f32 {
    match t {
        VelocityTouch::Light => 1.0,
        VelocityTouch::Medium => 0.0,
        VelocityTouch::Heavy => 2.0,
    }
}

pub(super) fn osc_mode_index(m: SynthOscMode) -> f32 {
    match m {
        SynthOscMode::Analog => 0.0,
        SynthOscMode::Wavetable => 1.0,
        SynthOscMode::Fm => 2.0,
        SynthOscMode::Sample => 3.0,
    }
}

/// Translate one panel command into host parameter writes appended to `out`.
///
/// Returns `true` when the command is understood (even if it produced no write, such as
/// `Midi`, which the sink routes to the note queue).
pub fn map_command(cmd: &AudioCommand, out: &mut Vec<W>) -> bool {
    use AudioCommand as C;
    match cmd {
        C::Organ(e) => map_organ(e, out),
        C::Piano(e) => map_piano(e, out),
        C::Synth(e) => map_synth(e, out),
        C::Fx(e) => map_fx(e, out),
        C::Midi(MidiEvent::ControlChange {
            controller, value, ..
        }) if *controller == cc::MOD_WHEEL => {
            out.push(W::global("mod_wheel", f32::from(*value) / 127.0));
        }
        C::SetMasterVolume(v) => out.push(W::global("master_level", *v)),
        C::SetMasterComp(v) => out.push(W::global("master_compressor", *v)),
        C::SetDrawbars(d) => {
            for (id, v) in DRAWBARS_A.iter().zip(d) {
                out.push(W::global(id, f32::from(*v)));
            }
        }
        C::SetOrganModel(m) => out.push(W::global("organ_model", organ_model_index(*m))),
        C::SetPercussion {
            enabled,
            soft,
            fast,
            third,
        } => {
            out.push(W::global("organ_perc_enable", flag(*enabled)));
            out.push(W::global("organ_perc_soft", flag(*soft)));
            out.push(W::global("organ_perc_fast", flag(*fast)));
            out.push(W::global("organ_perc_third", flag(*third)));
        }
        C::SetRotarySpeed(s) => out.push(W::global(
            "rotary_speed_fast",
            flag(matches!(s, RotarySpeed::Fast)),
        )),
        C::SetRotaryDrive(v) => out.push(W::global("rotary_drive", *v)),
        C::SetOrganLevel(v) => out.push(W::global("organ_a_level", *v)),
        C::SetOrganBLevel(v) => out.push(W::global("organ_b_level", *v)),
        C::SetPianoLevel(v) => out.push(W::global("piano_a_level", *v)),
        C::SetPianoBLevel(v) => out.push(W::global("piano_b_level", *v)),
        C::SetPianoCategory(c) => out.push(W::global("piano_category", piano_category_index(*c))),
        C::SetPianoTimbre(t) => {
            if let Some(i) = piano_timbre_index(*t) {
                out.push(W::global("piano_timbre", i));
            }
        }
        C::SetPianoTouch(t) => out.push(W::global("piano_touch", piano_touch_index(*t))),
        C::SetPianoAcoustics {
            string_res,
            pedal_noise,
        } => {
            out.push(W::global("piano_string_res", flag(*string_res)));
            out.push(W::global("piano_pedal_noise", flag(*pedal_noise)));
        }
        C::SetSynthLevel(v) => out.push(W::global("synth_a_level", *v)),
        C::SetSynthBLevel(v) => out.push(W::global("synth_b_level", *v)),
        C::SetSynthCLevel(v) => out.push(W::global("synth_c_level", *v)),
        C::SetSynthOscMode(m) => out.push(W::global("synth_osc_mode", osc_mode_index(*m))),
        C::SetSynthCutoff(v) => out.push(W::global("synth_filter_cutoff", *v)),
        C::SetSynthResonance(v) => out.push(W::global("synth_filter_resonance", *v)),
        C::SetSynthDrive(v) => out.push(W::global("synth_filter_drive", *v)),
        C::SetSynthAttack(v) => out.push(W::global("synth_attack", *v)),
        C::SetSynthDecay(v) => out.push(W::global("synth_decay", *v)),
        C::SetSynthRelease(v) => out.push(W::global("synth_release", *v)),
        C::SetLayerActive(i, on) => {
            if let Some(id) = LAYER_ENABLED.get(*i) {
                out.push(W::global(id, flag(*on)));
            }
        }
        // Raw MIDI goes to the note queue, not to a parameter.
        C::Midi(_) => {}
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(cmd: AudioCommand) -> Vec<W> {
        let mut out = Vec::new();
        map_command(&cmd, &mut out);
        out
    }

    #[test]
    fn master_volume_maps_to_master_level() {
        assert_eq!(
            run(AudioCommand::SetMasterVolume(0.4)),
            vec![W::global("master_level", 0.4)]
        );
    }

    #[test]
    fn drawbars_map_to_nine_ints() {
        let w = run(AudioCommand::SetDrawbars([8, 7, 6, 5, 4, 3, 2, 1, 0]));
        assert_eq!(w.len(), 9);
        assert_eq!(w[0], W::global("drawbar_1_16", 8.0));
        assert_eq!(w[8], W::global("drawbar_9_1", 0.0));
    }

    #[test]
    fn mod_wheel_cc_maps_to_the_morph_param_and_other_midi_does_not() {
        let w = run(AudioCommand::Midi(MidiEvent::ControlChange {
            channel: 0,
            controller: cc::MOD_WHEEL,
            value: 127,
        }));
        assert_eq!(w, vec![W::global("mod_wheel", 1.0)]);
        let note = AudioCommand::Midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 90,
        });
        let mut out = Vec::new();
        assert!(map_command(&note, &mut out));
        assert!(out.is_empty());
    }

    #[test]
    fn enum_indices_follow_the_host_conventions() {
        assert_eq!(organ_model_index(OrganModel::Pipe2), 4.0);
        assert_eq!(piano_category_index(PianoCategory::Misc), 5.0);
        assert_eq!(piano_timbre_index(PianoTimbre::Off), None);
        assert_eq!(piano_timbre_index(PianoTimbre::Dyno2), Some(4.0));
        assert_eq!(piano_touch_index(VelocityTouch::Light), 1.0);
        assert_eq!(osc_mode_index(SynthOscMode::Fm), 2.0);
        assert_eq!(
            enum_index::<crate::fx_selectors::Mod1Sel>(&nord_core::fx::Mod1Type::Wah),
            Some(4.0)
        );
    }

    #[test]
    fn layer_active_maps_to_the_enable_switches_and_ignores_bad_indices() {
        assert_eq!(
            run(AudioCommand::SetLayerActive(5, true)),
            vec![W::global("synth_b_enabled", 1.0)]
        );
        assert!(run(AudioCommand::SetLayerActive(9, true)).is_empty());
    }

    #[test]
    fn unmapped_commands_are_reported() {
        let mut out = Vec::new();
        assert!(!map_command(
            &AudioCommand::SetRotaryEnabled(true),
            &mut out
        ));
        assert!(out.is_empty());
    }
}
