//! Behaviour of the Piano bay controls as pure state transitions.
//!
//! Each function changes `Stage4PanelState` / `PianoUi` and pushes the [`PianoEdit`]s the DSP thread
//! needs; the bay widgets only call these (so they are testable without egui).

use super::prelude::*;
use crate::audio::PianoEdit;
use nord_panel::behaviour::cycle;

pub use super::piano_models::{model_step, position as models_position, type_cycle};

/// Piano types in LED order (manual p.24).
pub const CATEGORIES: [PianoCategory; 6] = [
    PianoCategory::Grand,
    PianoCategory::Upright,
    PianoCategory::Electric,
    PianoCategory::Clav,
    PianoCategory::Digital,
    PianoCategory::Misc,
];

/// Acoustic / electric timbre steps; `Off` lights no LED (manual p.26).
pub const TIMBRES: [PianoTimbre; 6] = [
    PianoTimbre::Off,
    PianoTimbre::Soft,
    PianoTimbre::Mid,
    PianoTimbre::Bright,
    PianoTimbre::Dyno1,
    PianoTimbre::Dyno2,
];

/// KB TOUCH steps in LED order, HEAVY first (manual p.25).
pub const TOUCHES: [VelocityTouch; 3] = [
    VelocityTouch::Heavy,
    VelocityTouch::Medium,
    VelocityTouch::Light,
];

/// Index of `x` in `list`, 0 when absent.
fn index_of<T: PartialEq>(list: &[T], x: &T) -> usize {
    list.iter().position(|y| y == x).unwrap_or(0)
}

/// Index of the selected type in [`CATEGORIES`].
pub fn category_index(s: &Stage4PanelState) -> usize {
    index_of(&CATEGORIES, &s.piano_cat)
}

/// Index of the selected touch in [`TOUCHES`].
pub fn touch_index(s: &Stage4PanelState) -> usize {
    index_of(&TOUCHES, &s.piano_touch)
}

/// Index of the selected timbre in [`TIMBRES`].
pub fn timbre_index(s: &Stage4PanelState) -> usize {
    index_of(&TIMBRES, &s.piano_timbre)
}

/// Scene mask (bit 0 Organ A .. bit 6 Synth C) from the panel's active flags.
pub fn scene_mask(s: &Stage4PanelState) -> u8 {
    [
        s.organ_a_active,
        s.organ_b_active,
        s.piano_a_active,
        s.piano_b_active,
        s.synth_a_active,
        s.synth_b_active,
        s.synth_c_active,
    ]
    .iter()
    .enumerate()
    .fold(0u8, |m, (i, on)| m | (u8::from(*on) << i))
}

fn piano_bits(s: &Stage4PanelState) -> u8 {
    u8::from(s.piano_a_active) | (u8::from(s.piano_b_active) << 1)
}

fn set_layer(s: &mut Stage4PanelState, layer: usize, on: bool, out: &mut Vec<PianoEdit>) {
    if layer == 0 {
        s.piano_a_active = on;
    } else {
        s.piano_b_active = on;
    }
    out.push(PianoEdit::LayerActive {
        layer: layer as u8,
        on,
    });
}

fn is_active(s: &Stage4PanelState, layer: usize) -> bool {
    if layer == 0 {
        s.piano_a_active
    } else {
        s.piano_b_active
    }
}

/// ON tap: switch the whole Piano section off, or bring back the layers it had.
pub fn on_tap(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    let bits = piano_bits(s);
    if bits != 0 {
        s.piano_ui.restore_layers = bits;
        for l in 0..2 {
            if is_active(s, l) {
                set_layer(s, l, false, out);
            }
        }
    } else {
        let restore = s.piano_ui.restore_layers & 0b11;
        let restore = if restore == 0 { 0b01 } else { restore };
        for l in 0..2 {
            if restore & (1 << l) != 0 {
                set_layer(s, l, true, out);
            }
        }
        s.piano_ui.focus = restore.trailing_zeros() as usize;
    }
    s.fx_focus = 1;
}

/// SOLO (ON held): only the Piano plays; the previous scene is remembered (manual p.23, p.43).
pub fn solo_start(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    if s.piano_ui.solo_saved.is_some() {
        return;
    }
    s.piano_ui.solo_saved = Some(scene_mask(s));
    let mut bits = piano_bits(s) << 2;
    if bits == 0 {
        bits = 1 << 2;
    }
    out.push(PianoEdit::SceneMask(bits));
}

/// ON released after SOLO: bring the previous scene back.
pub fn solo_end(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    if let Some(mask) = s.piano_ui.solo_saved.take() {
        out.push(PianoEdit::SceneMask(mask));
    }
}

/// Layer button tap. A non-active layer replaces the other one (`keep_other` adds it instead,
/// the mouse stand-in for "press both buttons"); an active layer just takes the focus.
pub fn layer_tap(
    s: &mut Stage4PanelState,
    layer: usize,
    keep_other: bool,
    out: &mut Vec<PianoEdit>,
) {
    let layer = layer.min(1);
    let other = 1 - layer;
    if !is_active(s, layer) {
        set_layer(s, layer, true, out);
        if !keep_other && is_active(s, other) {
            set_layer(s, other, false, out);
        }
    }
    s.piano_ui.focus = layer;
    s.fx_focus = 1;
}

/// Layer button held: turn the layer off when the other one stays on (manual p.23).
pub fn layer_hold(s: &mut Stage4PanelState, layer: usize, out: &mut Vec<PianoEdit>) {
    let layer = layer.min(1);
    let other = 1 - layer;
    if is_active(s, layer) && is_active(s, other) {
        set_layer(s, layer, false, out);
        s.piano_ui.focus = other;
    }
}

/// Fader move of one layer.
pub fn set_level(s: &mut Stage4PanelState, layer: usize, level: f32, out: &mut Vec<PianoEdit>) {
    let level = level.clamp(0.0, 1.0);
    for l in s.edit_layers(layer, 2) {
        if l == 0 {
            s.piano_a_level = level;
        } else {
            s.piano_b_level = level;
        }
        out.push(PianoEdit::Level {
            layer: l as u8,
            level,
        });
    }
}

/// OCTAVE SHIFT +-1 of the focused layer, clamped to -2..=2.
pub fn octave_step(s: &mut Stage4PanelState, delta: i8, out: &mut Vec<PianoEdit>) {
    let l = s.piano_ui.layer();
    let v = (s.piano_ui.octave[l] + delta).clamp(-2, 2);
    for t in s.edit_layers(l, 2) {
        s.piano_ui.octave[t] = v;
        if t == 0 {
            s.piano_octave = v;
        }
        out.push(PianoEdit::OctaveShift {
            layer: t as u8,
            octaves: v,
        });
    }
}

/// KB ZONE +-1 (Shift + octave buttons): moves the focused layer's zone assignment and sends it
/// (`PianoEdit::Zone`, see [`super::kb_zone`] for the positions).
pub fn zone_step(s: &mut Stage4PanelState, forward: bool, out: &mut Vec<PianoEdit>) {
    let l = s.piano_ui.layer();
    let pos = super::kb_zone::step(s.piano_ui.kb_zone[l], forward);
    for t in s.edit_layers(l, 2) {
        s.piano_ui.kb_zone[t] = pos;
        out.push(PianoEdit::Zone {
            layer: t as u8,
            leds: super::kb_zone::leds(pos),
        });
    }
}

/// SOFT RELEASE of the focused layer.
pub fn soft_release_toggle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    let l = s.piano_ui.layer();
    let on = !s.piano_ui.soft_release[l];
    for t in s.edit_layers(l, 2) {
        s.piano_ui.soft_release[t] = on;
        out.push(PianoEdit::SoftRelease { layer: t as u8, on });
    }
}

/// STRING RES toggle.
pub fn string_res_toggle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    s.string_res = !s.string_res;
    out.push(PianoEdit::StringRes(s.string_res));
}

/// PED NOISE toggle (Shift + STRING RES).
pub fn ped_noise_toggle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    s.pedal_noise = !s.pedal_noise;
    out.push(PianoEdit::PedalNoise(s.pedal_noise));
}

/// KB TOUCH: HEAVY -> MEDIUM -> LIGHT -> HEAVY.
pub fn touch_cycle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    s.piano_touch = TOUCHES[cycle(touch_index(s), TOUCHES.len(), true)];
    out.push(PianoEdit::Touch(s.piano_touch));
}

/// DYN COMP: off, 1, 2, 3 for the focused layer.
pub fn dyn_comp_cycle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    let l = s.piano_ui.layer();
    let v = cycle(usize::from(s.piano_ui.dyn_comp[l]), 4, true) as u8;
    for t in s.edit_layers(l, 2) {
        s.piano_ui.dyn_comp[t] = v;
        out.push(PianoEdit::DynComp {
            layer: t as u8,
            amount: v,
        });
    }
}

/// UNISON: off, 1, 2, 3 for the focused layer.
pub fn unison_cycle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    let l = s.piano_ui.layer();
    let v = cycle(usize::from(s.piano_ui.unison[l]), 4, true) as u8;
    for t in s.edit_layers(l, 2) {
        s.piano_ui.unison[t] = v;
        out.push(PianoEdit::Unison {
            layer: t as u8,
            amount: v,
        });
    }
}

/// Number of TIMBRE steps (incl. Off) of a type: the acoustic types have Soft, Mid, Bright; DYNO 1
/// and 2 exist for the Electric type only (manual p.26).
pub fn timbre_steps(cat: PianoCategory) -> usize {
    if cat == PianoCategory::Electric {
        TIMBRES.len()
    } else {
        4
    }
}

/// TIMBRE: Off, Soft, Mid, Bright, Dyno 1, Dyno 2 (the last two on Electric only). On the Clav type the seven pick-up EQ rocker
/// combinations (manual p.26) are stepped instead; the engine has no Clav timbre, so those are UI only.
pub fn timbre_cycle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    if s.piano_cat == PianoCategory::Clav {
        s.piano_ui.clav_timbre = (s.piano_ui.clav_timbre + 1) % 8;
        return;
    }
    let i = timbre_index(s);
    let next = if i + 1 >= timbre_steps(s.piano_cat) {
        0
    } else {
        i + 1
    };
    s.piano_timbre = TIMBRES[next];
    out.push(PianoEdit::Timbre(s.piano_timbre));
}

#[cfg(test)]
#[path = "piano_logic_tests.rs"]
mod tests;

#[cfg(test)]
mod smoke_tests {
    use super::*;

    #[test]
    fn index_helpers_default_to_the_expected_led() {
        let s = Stage4PanelState::default();
        assert_eq!(category_index(&s), 0);
        assert_eq!(timbre_index(&s), 0);
        assert_eq!(touch_index(&s), 1, "Medium is the middle LED");
    }
}
