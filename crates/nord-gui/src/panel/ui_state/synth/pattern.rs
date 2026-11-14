//! Arpeggiator Zig Zag, Range fraction and the custom gate pattern (manual p.35-37).
//!
//! The pattern pages edit a custom pattern. The first edit copies the selected preset into it
//! (`PatternParam::Customize`), so the panel's grid and the engine's agree; choosing a preset on
//! page 2 switches back to the fixed preset.

use super::layer::SynthLayerUi;
use super::{Scope, SynthUi};
use crate::audio::{PatternParam, SynthParam};
use nord_core::synth::{PatternPan, PATTERN_PRESETS};

/// Longest custom pattern (the engine's `PATTERN_MAX_STEPS`).
const MAX_LEN: u8 = 16;
/// Last detent of the GATE dial: 0..=8 eighths, then 9 = a full step tied into the next (`Q`).
const TIE_DETENT: u8 = 9;

impl SynthLayerUi {
    /// Whole octaves of the Range knob (`1..=4`).
    pub fn arp_octaves(&self) -> u8 {
        (self.range_detent() / 3 + 1).min(4)
    }

    /// Fraction of the Range knob in thirds of an octave on top of the octaves (`0..=2`).
    pub fn arp_thirds(&self) -> u8 {
        self.range_detent() % 3
    }

    /// Range knob detent `0..=9` (1 octave .. 4 octaves in steps of 1/3).
    fn range_detent(&self) -> u8 {
        ((self.arp_range - 1.0).max(0.0) * 3.0).round().min(9.0) as u8
    }

    /// Mirror of choosing preset `p`: the fixed preset plays and its grid is shown.
    pub fn select_preset(&mut self, p: u8) {
        let pat = &PATTERN_PRESETS[usize::from(p) % PATTERN_PRESETS.len()];
        self.pattern_preset = p;
        self.pattern_custom = false;
        self.pattern_len = pat.len;
        self.step_gate = pat.steps;
        self.step_tie = pat.tie;
        self.step_accent = [false; 16];
        self.step_pan = pat.pan.map(|p| match p {
            PatternPan::Left => 0,
            PatternPan::Center => 1,
            PatternPan::Right => 2,
        });
        self.pattern_cursor = self.pattern_cursor.min(pat.len - 1);
    }

    /// Emit Zig Zag, the Range fraction and the pattern source (every step when custom).
    pub fn emit_pattern(&self, f: &mut impl FnMut(SynthParam)) {
        let p = |x| SynthParam::Pattern(x);
        f(p(PatternParam::Zigzag(self.zigzag)));
        f(p(PatternParam::RangeThirds(self.arp_thirds())));
        if !self.pattern_custom {
            f(p(PatternParam::Custom(false)));
            return;
        }
        f(p(PatternParam::Len(self.pattern_len)));
        for i in 0..16u8 {
            let k = usize::from(i);
            f(p(PatternParam::Gate {
                step: i,
                eighths: self.step_gate[k],
            }));
            // After the gate: a rest (gate 0) drops the tie in the engine.
            f(p(PatternParam::Tie {
                step: i,
                on: self.step_tie[k],
            }));
            f(p(PatternParam::Accent {
                step: i,
                on: self.step_accent[k],
            }));
            f(p(PatternParam::Pan {
                step: i,
                pan: PatternPan::from_index(i32::from(self.step_pan[k])),
            }));
        }
        f(p(PatternParam::Custom(true)));
    }
}

impl SynthUi {
    /// ARP page 1 ZIG ZAG dial.
    pub(super) fn set_zigzag(&mut self, on: bool) {
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Zigzag(on)),
            |l| l.zigzag = on,
        );
    }

    /// ARP page 2 PRESET dial: back to a fixed preset.
    pub(super) fn choose_preset(&mut self, p: u8) {
        self.edit(Scope::Arp, SynthParam::PatternPreset(p), |l| {
            l.select_preset(p)
        });
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Custom(false)),
            |_| {},
        );
    }

    /// The first pattern edit turns the selected preset into the custom pattern.
    fn ensure_custom(&mut self) {
        if self.layer().pattern_custom {
            return;
        }
        let preset = self.layer().pattern_preset;
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Customize(preset)),
            |l| l.pattern_custom = true,
        );
    }

    /// ARP page 2 LENGTH dial.
    pub(super) fn step_pattern_len(&mut self, steps: i32) {
        self.ensure_custom();
        let n = (i32::from(self.layer().pattern_len) + steps).clamp(2, i32::from(MAX_LEN)) as u8;
        self.edit(Scope::Arp, SynthParam::Pattern(PatternParam::Len(n)), |l| {
            l.pattern_len = n;
            l.pattern_cursor = l.pattern_cursor.min(n - 1);
        });
    }

    /// POSITION dial of pages 3-5: moves the step cursor (UI only).
    pub(super) fn move_cursor(&mut self, steps: i32) {
        let l = self.layer_mut();
        let max = i32::from(l.pattern_len) - 1;
        l.pattern_cursor = (i32::from(l.pattern_cursor) + steps).clamp(0, max) as u8;
    }

    /// Page 3 GATE dial: gate length of the cursor step in eighths, `0` = rest, `8` = a full
    /// step, and one more detent past 8 = a full step tied into the next one (gate code `Q`,
    /// the hardware's `E q Q .` cycle). Turning back down from the tie detent returns to 8.
    pub(super) fn step_gate(&mut self, steps: i32) {
        self.ensure_custom();
        let i = self.layer().pattern_cursor;
        let k = usize::from(i);
        let detent = i32::from(self.layer().step_gate[k]) + i32::from(self.layer().step_tie[k]);
        let next = (detent + steps).clamp(0, i32::from(TIE_DETENT));
        let (v, tie) = (next.min(8) as u8, next == i32::from(TIE_DETENT));
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Gate {
                step: i,
                eighths: v,
            }),
            |l| l.step_gate[k] = v,
        );
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Tie { step: i, on: tie }),
            |l| l.step_tie[k] = tie,
        );
    }

    /// Page 4 ACCENT dial: clockwise accents the cursor step.
    pub(super) fn step_accent(&mut self, steps: i32) {
        self.ensure_custom();
        let (i, on) = (self.layer().pattern_cursor, steps > 0);
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Accent { step: i, on }),
            |l| l.step_accent[usize::from(i)] = on,
        );
    }

    /// Page 5 PAN dial: Left / Center / Right of the cursor step.
    pub(super) fn step_pan(&mut self, steps: i32) {
        self.ensure_custom();
        let i = self.layer().pattern_cursor;
        let v = (i32::from(self.layer().step_pan[usize::from(i)]) + steps).clamp(0, 2) as u8;
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Pan {
                step: i,
                pan: PatternPan::from_index(i32::from(v)),
            }),
            |l| l.step_pan[usize::from(i)] = v,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{SynthEdit, SynthTarget};
    use nord_core::synth::SynthLayer;

    fn params(s: &mut SynthUi) -> Vec<SynthParam> {
        s.take_edits()
            .into_iter()
            .filter_map(|e| match e {
                SynthEdit::Param(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }

    fn engine_of(s: &mut SynthUi) -> SynthLayer {
        let mut l = SynthLayer::new(48_000.0);
        for p in params(s) {
            p.apply(&mut l);
        }
        l
    }

    #[test]
    fn range_detents_split_into_octaves_and_thirds() {
        let mut l = SynthLayerUi::default();
        assert_eq!((l.arp_octaves(), l.arp_thirds()), (1, 0));
        l.arp_range = 2.0 + 2.0 / 3.0;
        assert_eq!((l.arp_octaves(), l.arp_thirds()), (2, 2));
        l.arp_range = 4.0;
        assert_eq!((l.arp_octaves(), l.arp_thirds()), (4, 0));
        l.arp_range = 9.0;
        assert_eq!(l.arp_octaves(), 4, "clamped to 4 octaves");
    }

    #[test]
    fn first_edit_customizes_the_selected_preset_and_edits_the_step() {
        let mut s = SynthUi::new();
        s.choose_preset(2);
        s.take_edits();
        s.layer_mut().pattern_cursor = 1;
        s.step_gate(3);
        let p = params(&mut s);
        assert_eq!(
            p[0],
            SynthParam::Pattern(PatternParam::Customize(2)),
            "grid is copied first"
        );
        let want = (PATTERN_PRESETS[2].steps[1] + 3).min(8);
        assert!(p.contains(&SynthParam::Pattern(PatternParam::Gate {
            step: 1,
            eighths: want
        })));
        assert!(s.layer().pattern_custom);
        assert_eq!(s.layer().step_gate[1], want, "preset 2 step 1 plus 3");
        s.step_gate(1);
        let again = params(&mut s);
        assert!(
            !again
                .iter()
                .any(|p| matches!(p, SynthParam::Pattern(PatternParam::Customize(_)))),
            "already custom: the preset is not copied again"
        );
        assert_eq!(again.len(), 2, "only the step's gate and tie are sent");
    }

    #[test]
    fn selecting_a_preset_mirrors_its_gate_and_pan() {
        let mut s = SynthUi::new();
        s.choose_preset(13);
        let l = s.layer();
        assert_eq!(l.pattern_len, 16);
        assert_eq!(l.step_gate, PATTERN_PRESETS[13].steps);
        // Preset 13 pan LLRLLRLLRRLRRLRR: Left = 0, Right = 2.
        assert_eq!(&l.step_pan[..4], &[0, 0, 2, 0]);
        s.choose_preset(3);
        assert_eq!(s.layer().step_pan, [1; 16], "preset 3 is centred");
    }

    #[test]
    fn panel_grid_equals_the_engine_grid_after_edits() {
        let mut s = SynthUi::new();
        s.choose_preset(3);
        s.step_pattern_len(2);
        s.move_cursor(2);
        s.step_accent(1);
        s.step_pan(5);
        s.move_cursor(1);
        s.step_gate(-9);
        let l = engine_of(&mut s);
        let ui = s.layer();
        assert!(l.perf.pattern_custom);
        assert_eq!(l.pattern_length(), ui.pattern_len);
        assert_eq!(l.perf.custom.gate, ui.step_gate);
        assert_eq!(l.perf.custom.accent, ui.step_accent);
        for k in 0..16 {
            assert_eq!(
                l.perf.custom.pan[k],
                PatternPan::from_index(i32::from(ui.step_pan[k]))
            );
        }
        assert_eq!(ui.step_pan[2], 2);
        assert!(ui.step_accent[2]);
        assert_eq!(ui.step_gate[3], 0);
    }

    #[test]
    fn gate_dial_has_a_tie_detent_past_a_full_step() {
        let mut s = SynthUi::new();
        s.layer_mut().pattern_cursor = 3;
        s.step_gate(100);
        let l = s.layer();
        assert_eq!(
            (l.step_gate[3], l.step_tie[3]),
            (8, true),
            "top detent is Q"
        );
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::Pattern(PatternParam::Gate {
            step: 3,
            eighths: 8
        })));
        assert!(p.contains(&SynthParam::Pattern(PatternParam::Tie {
            step: 3,
            on: true
        })));
        // One click down: the full step stays, the tie goes.
        s.step_gate(-1);
        assert_eq!((s.layer().step_gate[3], s.layer().step_tie[3]), (8, false));
        // One click up again toggles it back on; further clicks stay on the tie detent.
        s.step_gate(1);
        s.step_gate(5);
        assert_eq!((s.layer().step_gate[3], s.layer().step_tie[3]), (8, true));
        // All the way down: a rest, no tie.
        s.step_gate(-100);
        assert_eq!((s.layer().step_gate[3], s.layer().step_tie[3]), (0, false));
    }

    #[test]
    fn tie_reaches_the_engine_and_the_panel_grid_matches_it() {
        let mut s = SynthUi::new();
        s.choose_preset(3);
        s.layer_mut().pattern_cursor = 1;
        s.take_edits();
        s.step_gate(100);
        let l = engine_of(&mut s);
        assert!(l.perf.pattern_custom);
        assert!(l.perf.custom.tie[1] && l.perf.custom.gate[1] == 8);
        assert_eq!(l.perf.custom.tie, s.layer().step_tie);
        assert_eq!(l.perf.custom.gate, s.layer().step_gate);
    }

    #[test]
    fn a_preset_with_ties_shows_them_and_customizing_keeps_them() {
        let mut s = SynthUi::new();
        // Preset 13 has Q steps (`QEEEEEQE...`); the panel mirrors them.
        s.choose_preset(13);
        assert_eq!(s.layer().step_tie, PATTERN_PRESETS[13].tie);
        assert!(s.layer().step_tie[0]);
        s.take_edits();
        s.layer_mut().pattern_cursor = 1;
        s.step_accent(1);
        let l = engine_of(&mut s);
        assert_eq!(
            l.perf.custom.tie, PATTERN_PRESETS[13].tie,
            "ties survive customizing"
        );
        // Taking the tie off step 0 reaches the engine.
        s.layer_mut().pattern_cursor = 0;
        s.step_gate(-1);
        assert!(!s.layer().step_tie[0]);
        assert_eq!(s.layer().step_gate[0], 8);
    }

    #[test]
    fn choosing_a_preset_returns_to_the_fixed_pattern() {
        let mut s = SynthUi::new();
        s.step_gate(1);
        assert!(s.layer().pattern_custom);
        s.take_edits();
        s.choose_preset(5);
        let p = params(&mut s);
        assert!(p.contains(&SynthParam::PatternPreset(5)));
        assert!(p.contains(&SynthParam::Pattern(PatternParam::Custom(false))));
        let l = s.layer();
        assert!(!l.pattern_custom);
        assert_eq!(l.pattern_len, PATTERN_PRESETS[5].len);
        assert_eq!(l.step_gate, PATTERN_PRESETS[5].steps);
    }

    #[test]
    fn length_and_cursor_are_clamped() {
        let mut s = SynthUi::new();
        s.step_pattern_len(100);
        assert_eq!(s.layer().pattern_len, 16);
        s.move_cursor(100);
        assert_eq!(s.layer().pattern_cursor, 15);
        s.step_pattern_len(-100);
        assert_eq!(s.layer().pattern_len, 2);
        assert_eq!(s.layer().pattern_cursor, 1, "cursor follows the new length");
        s.move_cursor(-100);
        assert_eq!(s.layer().pattern_cursor, 0);
    }

    #[test]
    fn grouped_pattern_edit_reaches_all_layers() {
        let mut s = SynthUi::new();
        s.group_arp.double_tap();
        s.take_edits();
        s.step_accent(1);
        assert!(s
            .layers
            .iter()
            .all(|l| l.step_accent[0] && l.pattern_custom));
        assert!(s
            .take_edits()
            .iter()
            .all(|e| matches!(e, SynthEdit::Param(SynthTarget::All, _))));
    }

    #[test]
    fn emit_pattern_covers_the_custom_grid_only_when_custom() {
        let mut ui = SynthLayerUi::default();
        let mut n = 0;
        ui.emit_pattern(&mut |_| n += 1);
        assert_eq!(n, 3, "Zig Zag, Range fraction and Custom(false)");
        ui.pattern_custom = true;
        ui.step_gate[7] = 6;
        ui.step_gate[8] = 8;
        ui.step_tie[8] = true;
        let mut l = SynthLayer::new(48_000.0);
        ui.emit_pattern(&mut |p| p.apply(&mut l));
        assert!(l.perf.pattern_custom);
        assert_eq!(l.perf.custom.gate[7], 6);
        assert!(l.perf.custom.tie[8] && !l.perf.custom.tie[7]);
    }
}
