//! Organ and Piano edits
//! to host parameter writes (Synth: `map_synth`, FX: `map_fx`). Bank numbering: `fx_*` and `ped_*` count all seven layers (Organ A/B
//! = 1..2, Piano A/B = 3..4, Synth A/B/C = 5..7); `organx_*`, `pianox_*`, `synthx_*`, `synthm_*`
//! count within their area (Organ 1..2, Piano 1..2, Synth 1..3).

use super::map::{
    flag, organ_model_index, piano_category_index, piano_timbre_index, piano_touch_index,
    LAYER_ENABLED,
};
use super::write::ParamWrite as W;
use nord_core::organ::{ScannerMode, STOP_ANGLE_COUNT};
use nord_gui::audio::{OrganEdit, PianoEdit, ANGLE_NARROW_DEG, ANGLE_WIDE_DEG};

/// friends number only their own area (Organ 1..2, Piano 1..2, Synth 1..3).
const ORGAN_FIRST: u8 = 1;
const PIANO_PEDAL_FIRST: u8 = 3;
const PIANO_FIRST: u8 = 1;

/// Host scanner index (`organ_params::scanner_from_index`): Off, V1..V3, C1..C3.
fn scanner_index(m: ScannerMode) -> f32 {
    match m {
        ScannerMode::Off => 0.0,
        ScannerMode::V1 => 1.0,
        ScannerMode::V2 => 2.0,
        ScannerMode::V3 => 3.0,
        ScannerMode::C1 => 4.0,
        ScannerMode::C2 => 5.0,
        ScannerMode::C3 => 6.0,
    }
}

/// Host ids of the physical drawbar positions of the Drawbar Live mode, 16' first.
const LIVE_DRAWBARS: [&str; 9] = [
    "organz_live1",
    "organz_live2",
    "organz_live3",
    "organz_live4",
    "organz_live5",
    "organz_live6",
    "organz_live7",
    "organz_live8",
    "organz_live9",
];

/// Write `family` for both organ layers.
fn both_organ(out: &mut Vec<W>, family: &'static str, plain: f32) {
    for n in [ORGAN_FIRST, ORGAN_FIRST + 1] {
        out.push(W::layer(family, n, plain));
    }
}

pub(super) fn map_organ(e: &OrganEdit, out: &mut Vec<W>) {
    match *e {
        OrganEdit::Octave { layer, shift } => out.push(W::layer(
            "organx_octave",
            ORGAN_FIRST + layer.min(1),
            f32::from(shift),
        )),
        OrganEdit::Model { model, b3_bass } => {
            out.push(W::global("organ_model", organ_model_index(model)));
            for n in [ORGAN_FIRST, ORGAN_FIRST + 1] {
                out.push(W::layer("organx_b3_bass", n, flag(b3_bass)));
            }
        }
        OrganEdit::Scanner { layer, mode } => out.push(W::layer(
            "organx_scanner",
            ORGAN_FIRST + layer.min(1),
            scanner_index(mode),
        )),
        OrganEdit::Rotary { fast, stop_mode } => {
            out.push(W::global("rotary_speed_fast", flag(fast)));
            out.push(W::global("organx_stop_mode", flag(stop_mode)));
        }
        OrganEdit::CloseMic(on) => out.push(W::global("organx_close_mic", flag(on))),
        OrganEdit::Sustped { layer, on } => out.push(W::layer(
            "ped_sustped",
            ORGAN_FIRST + layer.min(1),
            flag(on),
        )),
        OrganEdit::PercPoly(on) => both_organ(out, "organz_poly_perc", flag(on)),
        OrganEdit::DrawbarLive(on) => both_organ(out, "organz_drawbar_live", flag(on)),
        OrganEdit::LiveDrawbars(d) => {
            for (family, v) in LIVE_DRAWBARS.iter().zip(d) {
                both_organ(out, family, f32::from(v));
            }
        }
        OrganEdit::RotaryOrgan(on) => both_organ(out, "organy_rotary_on", flag(on)),
        OrganEdit::RotaryStopAngle(a) => {
            both_organ(out, "organy_stop_angle", f32::from(a.min(STOP_ANGLE_COUNT)))
        }
        OrganEdit::RotaryAngle { wide } => both_organ(
            out,
            "organy_mic_angle",
            if wide {
                ANGLE_WIDE_DEG
            } else {
                ANGLE_NARROW_DEG
            },
        ),
        // SYNC is a momentary trigger (`organz_sync_{n}`) and the zone / aux assignments are
        // engine-only (`engine:` bindings); none has a writable host value, so they stay dimmed.
        OrganEdit::SyncPreset | OrganEdit::Zone { .. } | OrganEdit::AuxKb { .. } => {}
        OrganEdit::Pstick { layer, on } => {
            out.push(W::layer("ped_pstick", ORGAN_FIRST + layer.min(1), flag(on)))
        }
    }
}

pub(super) fn map_piano(e: &PianoEdit, out: &mut Vec<W>) {
    let n = |layer: u8| PIANO_FIRST + layer.min(1);
    match *e {
        PianoEdit::Level { layer, level } => out.push(W::global(
            if layer == 0 {
                "piano_a_level"
            } else {
                "piano_b_level"
            },
            level,
        )),
        PianoEdit::LayerActive { layer, on } => out.push(W::global(
            LAYER_ENABLED[2 + usize::from(layer.min(1))],
            flag(on),
        )),
        PianoEdit::Category(c) => out.push(W::global("piano_category", piano_category_index(c))),
        PianoEdit::Timbre(t) => {
            if let Some(i) = piano_timbre_index(t) {
                out.push(W::global("piano_timbre", i));
            }
        }
        PianoEdit::Touch(t) => out.push(W::global("piano_touch", piano_touch_index(t))),
        PianoEdit::StringRes(on) => out.push(W::global("piano_string_res", flag(on))),
        PianoEdit::PedalNoise(on) => out.push(W::global("piano_pedal_noise", flag(on))),
        PianoEdit::SoftRelease { layer, on } => {
            out.push(W::layer("pianox_soft_release", n(layer), flag(on)))
        }
        PianoEdit::Unison { layer, amount } => {
            out.push(W::layer("pianox_unison", n(layer), f32::from(amount)))
        }
        PianoEdit::DynComp { layer, amount } => {
            out.push(W::layer("pianox_dyn_comp", n(layer), f32::from(amount)))
        }
        PianoEdit::OctaveShift { layer, octaves } => {
            out.push(W::layer("pianox_octave", n(layer), f32::from(octaves)))
        }
        PianoEdit::Sustped(on) => {
            for layer in [PIANO_PEDAL_FIRST, PIANO_PEDAL_FIRST + 1] {
                out.push(W::layer("ped_sustped", layer, flag(on)));
            }
        }
        PianoEdit::Pstick(on) => {
            for layer in [PIANO_PEDAL_FIRST, PIANO_PEDAL_FIRST + 1] {
                out.push(W::layer("ped_pstick", layer, flag(on)));
            }
        }
        // The scene mask has no host parameter (the host has per-layer enable switches only).
        PianoEdit::SceneMask(_) => {}
        // KB zones and the aux-KB assignment are engine-only (`engine:` bindings).
        PianoEdit::Zone { .. } | PianoEdit::AuxKb { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::piano::PianoTimbre;

    fn organ(e: OrganEdit) -> Vec<W> {
        let mut v = Vec::new();
        map_organ(&e, &mut v);
        v
    }

    #[test]
    fn organ_edits_use_one_based_layers() {
        assert_eq!(
            organ(OrganEdit::Octave {
                layer: 1,
                shift: -2
            }),
            vec![W::layer("organx_octave", 2, -2.0)]
        );
        assert_eq!(
            organ(OrganEdit::Scanner {
                layer: 0,
                mode: ScannerMode::C3
            }),
            vec![W::layer("organx_scanner", 1, 6.0)]
        );
        assert_eq!(
            organ(OrganEdit::Model {
                model: nord_core::organ::OrganModel::Vox,
                b3_bass: true
            })
            .len(),
            3
        );
        assert_eq!(
            organ(OrganEdit::Rotary {
                fast: true,
                stop_mode: false
            }),
            vec![
                W::global("rotary_speed_fast", 1.0),
                W::global("organx_stop_mode", 0.0)
            ]
        );
    }

    #[test]
    fn stop_angle_maps_to_both_layers_and_clamps() {
        assert_eq!(
            organ(OrganEdit::RotaryStopAngle(9)),
            vec![
                W::layer("organy_stop_angle", ORGAN_FIRST, 6.0),
                W::layer("organy_stop_angle", ORGAN_FIRST + 1, 6.0)
            ]
        );
    }

    #[test]
    fn piano_edits_map_levels_flags_and_per_layer_banks() {
        let mut v = Vec::new();
        map_piano(
            &PianoEdit::Level {
                layer: 1,
                level: 0.3,
            },
            &mut v,
        );
        map_piano(
            &PianoEdit::Unison {
                layer: 0,
                amount: 2,
            },
            &mut v,
        );
        map_piano(&PianoEdit::Timbre(PianoTimbre::Off), &mut v);
        map_piano(&PianoEdit::Sustped(true), &mut v);
        map_piano(&PianoEdit::SceneMask(0xFF), &mut v);
        assert_eq!(v[0], W::global("piano_b_level", 0.3));
        assert_eq!(v[1], W::layer("pianox_unison", 1, 2.0));
        assert_eq!(v.len(), 4, "Timbre(Off) and SceneMask write nothing");
        assert_eq!(v[3], W::layer("ped_sustped", 4, 1.0));
    }
}
