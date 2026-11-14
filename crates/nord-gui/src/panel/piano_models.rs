//! PIANO SELECT as pure state: the library models grouped by piano type (manual p.24-25).
//!
//! TYPE picks one of the six types and selects a model of it; the dial steps through the models
//! of the current type; the TYPE LEDs follow the type of the current model. With no library (or a
//! flat list without types) TYPE only changes the engine type, which still drives the timbre sets.

use super::piano_logic::CATEGORIES;
use super::prelude::*;
use crate::audio::PianoEdit;
use nord_panel::behaviour::cycle;

/// Models carry a type (the list is parallel to `models`).
pub fn is_typed(s: &Stage4PanelState) -> bool {
    let u = &s.piano_ui;
    !u.models.is_empty() && u.model_types.len() == u.models.len()
}

/// Indices into `models` of the models of type `cat`; every model when the list is flat.
pub fn in_type(s: &Stage4PanelState, cat: PianoCategory) -> Vec<usize> {
    let u = &s.piano_ui;
    if !is_typed(s) {
        return (0..u.models.len()).collect();
    }
    (0..u.models.len())
        .filter(|&i| u.model_types[i] == cat)
        .collect()
}

/// `(position, count)` of the selected model among the models of the current type; position is
/// 1-based and `0` when the type has no model.
pub fn position(s: &Stage4PanelState) -> (usize, usize) {
    let idxs = in_type(s, s.piano_cat);
    let pos = idxs
        .iter()
        .position(|&i| i == s.piano_ui.model_idx)
        .map_or(0, |p| p + 1);
    (pos, idxs.len())
}

/// Select model `i` (dial position, remembered for its type) and ask the library to load it.
pub fn select(s: &mut Stage4PanelState, i: usize) {
    let u = &mut s.piano_ui;
    let Some(entry) = u.models.get(i).cloned() else {
        return;
    };
    u.model_idx = i;
    if let Some(c) = u.model_types.get(i) {
        u.last_model[c.index()] = Some(i);
    }
    u.model_request = Some(entry);
}

/// TYPE button: next of the six types. With a typed library the next type that has models is
/// chosen and its last-used (else first) model is loaded.
pub fn type_cycle(s: &mut Stage4PanelState, out: &mut Vec<PianoEdit>) {
    let typed = is_typed(s);
    let start = super::piano_logic::category_index(s);
    let mut next = cycle(start, CATEGORIES.len(), true);
    if typed {
        for _ in 0..CATEGORIES.len() {
            if !in_type(s, CATEGORIES[next]).is_empty() {
                break;
            }
            next = cycle(next, CATEGORIES.len(), true);
        }
    }
    s.piano_cat = CATEGORIES[next];
    out.push(PianoEdit::Category(s.piano_cat));
    // DYNO 1/2 exist for Electric only (manual p.26); leaving it clears them (ASSUMED: Off).
    if super::piano_logic::timbre_index(s) >= super::piano_logic::timbre_steps(s.piano_cat) {
        s.piano_timbre = PianoTimbre::Off;
        out.push(PianoEdit::Timbre(PianoTimbre::Off));
    }
    if typed {
        let idxs = in_type(s, s.piano_cat);
        let remembered = s.piano_ui.last_model[next].filter(|i| idxs.contains(i));
        if let Some(i) = remembered.or_else(|| idxs.first().copied()) {
            select(s, i);
        }
    }
}

/// Step the PIANO SELECT dial through the models of the current type (wraps). Returns `false`
/// when there is no model to step to.
pub fn model_step(s: &mut Stage4PanelState, forward: bool) -> bool {
    let idxs = in_type(s, s.piano_cat);
    if idxs.is_empty() {
        return false;
    }
    let at = idxs
        .iter()
        .position(|&i| i == s.piano_ui.model_idx)
        .unwrap_or(if forward { idxs.len() - 1 } else { 0 });
    select(s, idxs[cycle(at, idxs.len(), forward)]);
    true
}

/// The loaded Piano A model changed to model `i`: move the dial there and light the LED of its
/// type (the engine type is sent by the library when the sound arrives).
pub fn follow_loaded(s: &mut Stage4PanelState, i: usize) {
    let u = &mut s.piano_ui;
    u.model_idx = i;
    if let Some(&c) = u.model_types.get(i) {
        u.last_model[c.index()] = Some(i);
        s.piano_cat = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two Grand, one Upright, no Electric, one Clav.
    fn lib() -> Stage4PanelState {
        let mut s = Stage4PanelState::default();
        s.piano_ui.models = ["g0", "g1", "u0", "c0"].map(String::from).to_vec();
        s.piano_ui.model_types = vec![
            PianoCategory::Grand,
            PianoCategory::Grand,
            PianoCategory::Upright,
            PianoCategory::Clav,
        ];
        s
    }

    #[test]
    fn type_selects_the_first_model_of_the_type_and_skips_empty_types() {
        let mut s = lib();
        let mut out = vec![];
        type_cycle(&mut s, &mut out);
        assert_eq!(s.piano_cat, PianoCategory::Upright);
        assert_eq!(s.piano_ui.model_request.as_deref(), Some("u0"));
        type_cycle(&mut s, &mut out);
        assert_eq!(s.piano_cat, PianoCategory::Clav, "Electric has no models");
        assert_eq!(s.piano_ui.model_idx, 3);
        type_cycle(&mut s, &mut out);
        assert_eq!(
            s.piano_cat,
            PianoCategory::Grand,
            "Digital, Misc are empty too"
        );
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], PianoEdit::Category(PianoCategory::Upright));
    }

    #[test]
    fn leaving_electric_clears_a_dyno_timbre() {
        let mut s = Stage4PanelState {
            piano_cat: PianoCategory::Electric,
            piano_timbre: PianoTimbre::Dyno2,
            ..Stage4PanelState::default()
        };
        let mut out = vec![];
        type_cycle(&mut s, &mut out);
        assert_eq!(s.piano_cat, PianoCategory::Clav);
        assert_eq!(s.piano_timbre, PianoTimbre::Off);
        assert_eq!(out[1], PianoEdit::Timbre(PianoTimbre::Off));
    }

    #[test]
    fn type_returns_to_the_last_model_used_in_that_type() {
        let mut s = lib();
        let mut out = vec![];
        assert!(model_step(&mut s, true));
        assert_eq!(s.piano_ui.model_idx, 1);
        type_cycle(&mut s, &mut out);
        for _ in 0..2 {
            type_cycle(&mut s, &mut out);
        }
        assert_eq!(s.piano_cat, PianoCategory::Grand);
        assert_eq!(s.piano_ui.model_request.as_deref(), Some("g1"));
    }

    #[test]
    fn dial_wraps_inside_the_current_type_only() {
        let mut s = lib();
        assert!(model_step(&mut s, true));
        assert!(model_step(&mut s, true));
        assert_eq!(s.piano_ui.model_idx, 0, "g1 wraps to g0, never to u0");
        assert!(model_step(&mut s, false));
        assert_eq!(s.piano_ui.model_idx, 1);
        assert_eq!(position(&s), (2, 2));
    }

    #[test]
    fn following_a_loaded_model_moves_dial_and_type_led() {
        let mut s = lib();
        follow_loaded(&mut s, 2);
        assert_eq!(s.piano_cat, PianoCategory::Upright);
        assert_eq!(position(&s), (1, 1));
        assert_eq!(s.piano_ui.last_model[1], Some(2));
    }

    #[test]
    fn without_a_library_type_only_cycles_the_category() {
        let mut s = Stage4PanelState::default();
        let mut out = vec![];
        type_cycle(&mut s, &mut out);
        assert_eq!(s.piano_cat, PianoCategory::Upright);
        assert!(s.piano_ui.model_request.is_none());
        assert!(!model_step(&mut s, true));
        assert_eq!(position(&s), (0, 0));
    }

    #[test]
    fn a_flat_untyped_list_is_not_filtered() {
        let mut s = Stage4PanelState::default();
        s.piano_ui.models = ["a", "b", "c"].map(String::from).to_vec();
        let mut out = vec![];
        type_cycle(&mut s, &mut out);
        assert_eq!(s.piano_cat, PianoCategory::Upright);
        assert!(
            s.piano_ui.model_request.is_none(),
            "TYPE does not pick a model"
        );
        assert!(model_step(&mut s, true));
        assert_eq!(s.piano_ui.model_request.as_deref(), Some("b"));
        assert_eq!(in_type(&s, PianoCategory::Clav), [0, 1, 2]);
    }
}
