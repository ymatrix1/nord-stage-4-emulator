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
        self.step_accent = [false; 16];
        self.step_pan = [1; 16];
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
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Len(n)),
            |l| {
                l.pattern_len = n;
                l.pattern_cursor = l.pattern_cursor.min(n - 1);
            },
        );
    }

    /// POSITION dial of pages 3-5: moves the step cursor (UI only).
    pub(super) fn move_cursor(&mut self, steps: i32) {
        let l = self.layer_mut();
        let max = i32::from(l.pattern_len) - 1;
        l.pattern_cursor = (i32::from(l.pattern_cursor) + steps).clamp(0, max) as u8;
    }

    /// Page 3 GATE dial: gate length of the cursor step in eighths.
    pub(super) fn step_gate(&mut self, steps: i32) {
        self.ensure_custom();
        let (i, g) = (self.layer().pattern_cursor, self.layer().step_gate);
        let v = (i32::from(g[usize::from(i)]) + steps).clamp(0, 8) as u8;
        self.edit(
            Scope::Arp,
            SynthParam::Pattern(PatternParam::Gate {
                step: i,
                eighths: v,
            }),
            |l| l.step_gate[usize::from(i)] = v,
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
        assert!(p.contains(&SynthParam::Pattern(PatternParam::Gate {
            step: 1,
            eighths: 5
        })));
        assert!(s.layer().pattern_custom);
        assert_eq!(s.layer().step_gate[1], 5, "preset 3 has gate 2 on step 2");
        s.step_gate(1);
        let again = params(&mut s);
        assert_eq!(again.len(), 1, "already custom: only the step is sent");
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
        assert!(s.layers.iter().all(|l| l.step_accent[0] && l.pattern_custom));
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
        let mut l = SynthLayer::new(48_000.0);
        ui.emit_pattern(&mut |p| p.apply(&mut l));
        assert!(l.perf.pattern_custom);
        assert_eq!(l.perf.custom.gate[7], 6);
    }
}
