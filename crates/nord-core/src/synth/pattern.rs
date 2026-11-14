//! Custom pattern of the Arpeggiator/Gate PATTERN EDIT / ACCENT / PAN pages (manual p.36-37).
//!
//! Fixed-size storage only, so rendering never allocates. The pattern is used instead of a
//! preset while `PerfState::pattern_custom` is set.

use super::pattern_presets::{gate_eighths, pan_of, GATE_CODES, PAN_CODES};
use super::*;

/// Maximum number of pattern steps (manual p.36: the Length dial sets 2 to 16).
pub const PATTERN_MAX_STEPS: usize = 16;

/// Velocity added to an accented step (Arp/Poly).
/// ASSUMED: +32 of 127; the manual only says an accent needs a velocity-sensitive parameter to
/// be audible (p.37). Confirm by comparing accented and plain step levels with Amp Velocity max.
pub const ACCENT_VELOCITY_BOOST: u8 = 32;

/// Stereo position of one pattern step (Pan page, manual p.37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternPan {
    Left,
    #[default]
    Center,
    Right,
}

impl PatternPan {
    /// Maps a host index `0..=2` (clamped): Left, Center, Right.
    pub fn from_index(i: i32) -> Self {
        match i {
            i32::MIN..=0 => Self::Left,
            1 => Self::Center,
            _ => Self::Right,
        }
    }

    /// Pan position `-1.0` (Left), `0.0` (Center), `1.0` (Right).
    pub fn position(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Center => 0.0,
            Self::Right => 1.0,
        }
    }
}

/// Constant-power left/right gains for a pan position `-1..=1`; `0` gives `(1, 1)` (unity per
/// channel, as an unpanned voice) and a hard side puts `sqrt(2)` on that channel only.
/// ASSUMED: constant-power law; the hardware pan law is unmeasured.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    )
}

/// User-editable pattern: per step gate length (eighths of a step, `0` = rest, `8` = full), tie
/// (hardware gate code `Q`, see [`GatePattern`]), accent and pan.
///
/// This is also the pattern that preset dial value 0 plays: the hardware keeps the user's own
/// pattern words aside while a preset is selected and restores them for 0 (VERIFIED, replayed by
/// `python tools/ns4_arp_gate.py dial <bank0_CODE.bin>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomPattern {
    pub len: u8,
    pub gate: [u8; PATTERN_MAX_STEPS],
    pub tie: [bool; PATTERN_MAX_STEPS],
    pub accent: [bool; PATTERN_MAX_STEPS],
    pub pan: [PatternPan; PATTERN_MAX_STEPS],
}

impl Default for CustomPattern {
    /// ASSUMED: eight half-length centred steps without accent, a neutral starting grid.
    fn default() -> Self {
        Self {
            len: 8,
            gate: [4; PATTERN_MAX_STEPS],
            tie: [false; PATTERN_MAX_STEPS],
            accent: [false; PATTERN_MAX_STEPS],
            pan: [PatternPan::Center; PATTERN_MAX_STEPS],
        }
    }
}

impl CustomPattern {
    /// Builds a pattern from the stored hardware words (`Edt_Syn_kArpPtnLen` + 2 as `steps`,
    /// Dur `gate`, Acc `accent`, Pan `pan`): 16 steps of 2 bits, step `n` at bits `2n..2n+1` from
    /// the least significant bit. Gate `0 E`, `1 q`, `2 Q` (tie), `3 .` (rest); accent digit `1` =
    /// accented; pan `0` centre, `1` left, `2` right. This is how a factory program's own pattern
    /// reaches the engine (`cbin::synth_pattern`).
    pub fn from_words(steps: u8, gate: u32, accent: u32, pan: u32) -> Self {
        let mut out = Self {
            len: steps.clamp(2, PATTERN_MAX_STEPS as u8),
            ..Self::default()
        };
        for i in 0..PATTERN_MAX_STEPS {
            let digit = |word: u32| ((word >> (2 * i)) & 3) as usize;
            let g = digit(gate);
            out.gate[i] = gate_eighths(GATE_CODES[g]);
            out.tie[i] = GATE_CODES[g] == b'Q';
            out.accent[i] = digit(accent) == 1;
            out.pan[i] = pan_of(PAN_CODES[digit(pan)]);
        }
        out
    }

    /// Starts from a preset (its gate and pan, no accent), as the menu does when a preset is
    /// selected and then customised.
    pub fn from_preset(p: &GatePattern) -> Self {
        Self {
            len: p.len,
            gate: p.steps,
            tie: p.tie,
            pan: p.pan,
            ..Self::default()
        }
    }
}

impl SynthLayer {
    /// Sets the custom pattern length (`2..=16`, clamped).
    pub fn set_pattern_length(&mut self, len: u8) {
        self.perf.custom.len = len.clamp(2, PATTERN_MAX_STEPS as u8);
    }

    /// Custom pattern length.
    pub fn pattern_length(&self) -> u8 {
        self.perf.custom.len
    }

    /// Gate length of a step in eighths of a step (`0` = rest, clamped to `8`); out-of-range steps
    /// are ignored. A rest drops the step's tie.
    ///
    /// Editing a gate while a preset (dial `1..=28`) plays first copies that preset into the custom
    /// pattern, as the hardware does: its gate (`0x118`) listener snapshots the words and sets the
    /// preset dial back to 0 (VERIFIED, `tools/ns4_arp_gate.py dial`). Accent, pan and length edits
    /// do not switch the dial on the hardware.
    pub fn set_step_gate(&mut self, step: usize, eighths: u8) {
        if !self.perf.pattern_custom && self.perf.pattern_preset > 0 {
            self.customize_preset(usize::from(self.perf.pattern_preset));
        }
        if let Some(g) = self.perf.custom.gate.get_mut(step) {
            *g = eighths.min(8);
            if *g == 0 {
                self.perf.custom.tie[step] = false;
            }
        }
    }

    /// Tie of a step (gate code `Q`): its note is held into the next step. Out-of-range steps are
    /// ignored.
    pub fn set_step_tie(&mut self, step: usize, tie: bool) {
        if let Some(t) = self.perf.custom.tie.get_mut(step) {
            *t = tie;
        }
    }

    /// Accent on a step (a velocity boost, audible with velocity-sensitive Amp/Filter).
    pub fn set_step_accent(&mut self, step: usize, accent: bool) {
        if let Some(a) = self.perf.custom.accent.get_mut(step) {
            *a = accent;
        }
    }

    /// Pan of a step.
    pub fn set_step_pan(&mut self, step: usize, pan: PatternPan) {
        if let Some(p) = self.perf.custom.pan.get_mut(step) {
            *p = pan;
        }
    }

    /// Replaces the custom pattern with preset `index` and switches the pattern to it.
    pub fn customize_preset(&mut self, index: usize) {
        let p = &PATTERN_PRESETS[index % PATTERN_PRESET_COUNT];
        self.perf.custom = CustomPattern::from_preset(p);
        self.perf.pattern_custom = true;
    }

    /// The preset dial value the hardware shows: `0` while the custom pattern plays (the "preset
    /// edited" state) and the selected preset otherwise.
    pub fn pattern_dial_value(&self) -> u8 {
        if self.perf.pattern_custom {
            0
        } else {
            self.perf.pattern_preset.min(PATTERN_PRESET_MAX)
        }
    }

    /// Turns the preset dial to `value` (`0..=28`, clamped). `1..=28` play that preset; `0` plays
    /// the saved custom pattern. The custom pattern is kept in both cases, so going back to 0
    /// restores it (hardware: `0xC0102F30` / `0xC010FDAC`).
    pub fn select_pattern_preset(&mut self, value: u8) {
        self.perf.pattern_preset = value.min(PATTERN_PRESET_MAX);
        self.perf.pattern_custom = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_gains_are_constant_power_and_centre_is_unity() {
        let (l, r) = pan_gains(0.0);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = pan_gains(-1.0);
        assert!((l - std::f32::consts::SQRT_2).abs() < 1e-6 && r.abs() < 1e-6);
        for p in [-0.7f32, 0.3, 1.0] {
            let (l, r) = pan_gains(p);
            assert!((l * l + r * r - 2.0).abs() < 1e-5);
        }
    }

    #[test]
    fn setters_clamp_and_ignore_bad_steps() {
        let mut s = SynthLayer::new(48_000.0);
        s.set_pattern_length(1);
        assert_eq!(s.pattern_length(), 2);
        s.set_pattern_length(99);
        assert_eq!(s.pattern_length(), 16);
        s.set_step_gate(3, 200);
        assert_eq!(s.perf.custom.gate[3], 8);
        s.set_step_gate(16, 1);
        s.set_step_accent(2, true);
        s.set_step_pan(2, PatternPan::Right);
        assert!(s.perf.custom.accent[2]);
        assert_eq!(s.perf.custom.pan[2], PatternPan::Right);
        assert_eq!(PatternPan::from_index(-4), PatternPan::Left);
        assert_eq!(PatternPan::from_index(9), PatternPan::Right);
    }

    #[test]
    fn gate_edit_on_a_preset_customizes_it_and_sets_the_dial_to_zero() {
        let mut s = SynthLayer::new(48_000.0);
        s.select_pattern_preset(10);
        assert_eq!(s.pattern_dial_value(), 10);
        s.set_step_gate(1, 8);
        assert!(s.perf.pattern_custom);
        assert_eq!(s.pattern_dial_value(), 0);
        // The preset's other steps and its ties were copied, then the edit was applied.
        assert_eq!(s.perf.custom.gate[0], 8);
        assert!(s.perf.custom.tie[0]);
        assert_eq!(s.perf.custom.gate[1], 8);
    }

    #[test]
    fn dial_zero_restores_the_saved_custom_pattern() {
        let mut s = SynthLayer::new(48_000.0);
        s.perf.custom.gate[0] = 2;
        s.select_pattern_preset(5);
        assert_eq!(s.pattern_dial_value(), 5);
        // Selecting a preset does not touch the saved custom pattern ...
        assert_eq!(s.perf.custom.gate[0], 2);
        // ... and the dial value is clamped.
        s.select_pattern_preset(200);
        assert_eq!(s.perf.pattern_preset, PATTERN_PRESET_MAX);
        s.select_pattern_preset(0);
        assert_eq!(s.pattern_dial_value(), 0);
        assert!(s.perf.plays_custom());
    }

    #[test]
    fn rest_drops_the_tie_and_tie_setter_ignores_bad_steps() {
        let mut s = SynthLayer::new(48_000.0);
        s.set_step_tie(2, true);
        assert!(s.perf.custom.tie[2]);
        s.set_step_gate(2, 0);
        assert!(!s.perf.custom.tie[2]);
        s.set_step_tie(16, true);
    }

    #[test]
    fn from_words_decodes_gate_accent_pan_and_ties() {
        // Gate step 0 Q, 1 q, 2 ., 3 E; accent on step 1; pan step 0 L, 1 R, 2 C.
        let c = CustomPattern::from_words(4, 0b00_11_01_10, 0b00_00_01_00, 0b00_10_01);
        assert_eq!(c.len, 4);
        assert_eq!(&c.gate[..4], &[8, 8, 0, 4]);
        assert_eq!(&c.tie[..4], &[true, false, false, false]);
        assert_eq!(&c.accent[..4], &[false, true, false, false]);
        assert_eq!(c.pan[0], PatternPan::Left);
        assert_eq!(c.pan[1], PatternPan::Right);
        assert_eq!(c.pan[2], PatternPan::Center);
        // Lengths are clamped to the 2..=16 the hardware allows.
        assert_eq!(CustomPattern::from_words(0, 0, 0, 0).len, 2);
        assert_eq!(CustomPattern::from_words(17, 0, 0, 0).len, 16);
    }

    #[test]
    fn from_words_reproduces_every_preset_from_its_words() {
        // Re-encode each preset as hardware words and decode them again.
        for (i, p) in PATTERN_PRESETS.iter().enumerate() {
            let (mut gate, mut pan) = (0u32, 0u32);
            for k in 0..usize::from(p.len) {
                let code = match (p.steps[k], p.tie[k]) {
                    (0, _) => 3,
                    (4, _) => 0,
                    (_, true) => 2,
                    _ => 1,
                };
                gate |= code << (2 * k);
                pan |= match p.pan[k] {
                    PatternPan::Center => 0,
                    PatternPan::Left => 1,
                    PatternPan::Right => 2,
                } << (2 * k);
            }
            let c = CustomPattern::from_words(p.len, gate, 0, pan);
            let want = CustomPattern::from_preset(p);
            let n = usize::from(p.len);
            // Steps beyond the length are not part of the pattern.
            assert_eq!(c.len, want.len, "preset {i}");
            assert_eq!(c.gate[..n], want.gate[..n], "preset {i}");
            assert_eq!(c.tie[..n], want.tie[..n], "preset {i}");
            assert_eq!(c.pan[..n], want.pan[..n], "preset {i}");
        }
    }

    #[test]
    fn customize_preset_copies_the_grid() {
        let mut s = SynthLayer::new(48_000.0);
        s.customize_preset(2);
        assert!(s.perf.pattern_custom);
        assert_eq!(s.perf.custom.gate[0], PATTERN_PRESETS[2].steps[0]);
        assert_eq!(s.pattern_length(), PATTERN_PRESETS[2].len);
    }
}
