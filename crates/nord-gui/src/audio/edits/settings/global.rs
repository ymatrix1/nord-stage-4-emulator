//! System, Sound, Output and Aux KB menu items (manual OS v1.6x p.57-59).

use super::SettingsEdit;
use nord_core::engine::{AuxVelocityCurve, Level3, Stage4Engine, TonewheelMode};

/// Aux KB synth choice index (`---`, `A--`, `-B-`, `--C`, `AB-`, `A-C`, `-BC`, `ABC`) to the
/// layer bitmask (bit 0 = A). VERIFIED against the labels in `menus/aux_kb.rs` (the choice order
/// is not a plain bitmask), checked by `aux_synth_choices_map_to_their_layers`.
const SYNTH_MASK: [u8; 8] = [0, 0b001, 0b010, 0b100, 0b011, 0b101, 0b110, 0b111];

fn level3(v: usize) -> Level3 {
    match v {
        0 => Level3::Low,
        2.. => Level3::High,
        _ => Level3::Normal,
    }
}

/// Handles the global-settings, program-level and Aux KB ids. `false` when `e` is not one.
pub(super) fn apply(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    if e.id.starts_with("aux_kb.") {
        return aux_kb(e, engine);
    }
    if e.id == "sound.1.program_levels" {
        engine.set_program_level_db(e.float());
        return true;
    }
    let mut s = engine.global_settings();
    match e.id {
        "system.2.transpose" => s.transpose = e.int().clamp(-6, 6) as i8,
        "system.3.fine_tune" => s.fine_tune_cents = e.float(),
        "sound.2.piano_pedal_noise" => s.piano_pedal_noise_db = e.float(),
        "sound.3.piano_string_res" => s.piano_string_res_db = e.float(),
        "sound.5.b3_tonewheel_mode" => {
            s.tonewheel_mode = match e.index() {
                0 => TonewheelMode::Clean,
                2.. => TonewheelMode::Vintage2,
                _ => TonewheelMode::Vintage1,
            }
        }
        "sound.6.b3_click_level" => s.organ_click_high = e.index() == 1,
        // 70/30, 60/40, 50/50, 40/60, 30/70
        "sound.7.rotary_bass_horn" => s.rotary_bass_percent = 70 - 10 * e.index().min(4) as u8,
        "sound.8.rotary_rotor_speed" => s.rotor_speed = level3(e.index()),
        "sound.9.rotary_rotor_acc" => s.rotor_acc = level3(e.index()),
        "sound.10.rotary_horn_speed" => s.horn_speed = level3(e.index()),
        "sound.11.rotary_horn_acc" => s.horn_acc = level3(e.index()),
        "output.2.mono" => s.mono_output = e.on(),
        _ => return false,
    }
    engine.set_global_settings(s);
    true
}

fn aux_kb(e: &SettingsEdit, engine: &mut Stage4Engine) -> bool {
    let aux = &mut engine.aux_kb;
    let l = aux.layers;
    let mut organ = u8::from(l[0]) | u8::from(l[1]) << 1;
    let mut piano = u8::from(l[2]) | u8::from(l[3]) << 1;
    let mut synth = u8::from(l[4]) | u8::from(l[5]) << 1 | u8::from(l[6]) << 2;
    match e.id {
        "aux_kb.1.enable" => {
            aux.enabled = e.on();
            return true;
        }
        "aux_kb.2.organ" => organ = (e.index() & 3) as u8,
        "aux_kb.2.piano" => piano = (e.index() & 3) as u8,
        "aux_kb.2.synth" => synth = SYNTH_MASK[e.index().min(7)],
        "aux_kb.3.curve" => {
            aux.curve = match e.index() {
                0 => AuxVelocityCurve::Soft,
                2.. => AuxVelocityCurve::Hard,
                _ => AuxVelocityCurve::Medium,
            };
            return true;
        }
        _ => return false,
    }
    aux.set_section_masks(organ, piano, synth);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::menus::Value;

    fn run(id: &'static str, value: Value, e: &mut Stage4Engine) {
        assert!(SettingsEdit { id, value }.apply(e), "{id} not handled");
    }

    #[test]
    fn system_sound_and_output_items_write_the_global_settings() {
        let mut e = Stage4Engine::new(48_000.0);
        run("system.2.transpose", Value::Int(9), &mut e);
        run("system.3.fine_tune", Value::Int(-20), &mut e);
        run("sound.2.piano_pedal_noise", Value::Float(3.5), &mut e);
        run("sound.5.b3_tonewheel_mode", Value::Index(0), &mut e);
        run("sound.6.b3_click_level", Value::Index(1), &mut e);
        run("sound.7.rotary_bass_horn", Value::Index(0), &mut e);
        run("sound.8.rotary_rotor_speed", Value::Index(2), &mut e);
        run("sound.11.rotary_horn_acc", Value::Index(0), &mut e);
        run("output.2.mono", Value::Bool(true), &mut e);
        let s = e.global_settings();
        assert_eq!((s.transpose, s.fine_tune_cents), (6, -20.0));
        assert_eq!(s.piano_pedal_noise_db, 3.5);
        assert_eq!(s.tonewheel_mode, TonewheelMode::Clean);
        assert!(s.organ_click_high && s.mono_output);
        assert_eq!(s.rotary_bass_percent, 70);
        assert_eq!((s.rotor_speed, s.horn_acc), (Level3::High, Level3::Low));
    }

    #[test]
    fn program_levels_reach_the_engine() {
        let mut e = Stage4Engine::new(48_000.0);
        run("sound.1.program_levels", Value::Float(-3.5), &mut e);
        assert_eq!(e.program_level_db(), -3.5);
    }

    #[test]
    fn aux_synth_choices_map_to_their_layers() {
        let mut e = Stage4Engine::new(48_000.0);
        run("aux_kb.1.enable", Value::Bool(true), &mut e);
        run("aux_kb.2.organ", Value::Index(2), &mut e); // -B
        run("aux_kb.2.piano", Value::Index(3), &mut e); // AB
        run("aux_kb.2.synth", Value::Index(3), &mut e); // --C
        assert!(e.aux_kb.enabled);
        assert_eq!(
            e.aux_kb.layers,
            [false, true, true, true, false, false, true]
        );
        run("aux_kb.2.synth", Value::Index(5), &mut e); // A-C
        assert_eq!(e.aux_kb.layers[4..], [true, false, true]);
        assert_eq!(
            e.aux_kb.layers[..4],
            [false, true, true, true],
            "others kept"
        );
        run("aux_kb.3.curve", Value::Index(2), &mut e);
        assert_eq!(e.aux_kb.curve, AuxVelocityCurve::Hard);
    }
}
