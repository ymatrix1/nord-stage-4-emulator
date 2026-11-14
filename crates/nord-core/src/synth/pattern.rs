//! Custom pattern of the Arpeggiator/Gate PATTERN EDIT / ACCENT / PAN pages (manual p.36-37).
//!
//! Fixed-size storage only, so rendering never allocates. The pattern is used instead of a
//! preset while `PerfState::pattern_custom` is set.

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

/// User-editable pattern: per step gate length (eighths of a step, `0` = rest, `8` = full),
/// accent and pan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomPattern {
    pub len: u8,
    pub gate: [u8; PATTERN_MAX_STEPS],
    pub accent: [bool; PATTERN_MAX_STEPS],
    pub pan: [PatternPan; PATTERN_MAX_STEPS],
}

impl Default for CustomPattern {
    /// ASSUMED: eight half-length centred steps without accent, a neutral starting grid.
    fn default() -> Self {
        Self {
            len: 8,
            gate: [4; PATTERN_MAX_STEPS],
            accent: [false; PATTERN_MAX_STEPS],
            pan: [PatternPan::Center; PATTERN_MAX_STEPS],
        }
    }
}

impl CustomPattern {
    /// Starts from a preset (its gate and pan, no accent), as the menu does when a preset is
    /// selected and then customised.
    pub fn from_preset(p: &GatePattern) -> Self {
        Self {
            len: p.len,
            gate: p.steps,
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
    /// are ignored.
    pub fn set_step_gate(&mut self, step: usize, eighths: u8) {
        if let Some(g) = self.perf.custom.gate.get_mut(step) {
            *g = eighths.min(8);
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
    fn customize_preset_copies_the_grid() {
        let mut s = SynthLayer::new(48_000.0);
        s.customize_preset(2);
        assert!(s.perf.pattern_custom);
        assert_eq!(s.perf.custom.gate[0], PATTERN_PRESETS[2].steps[0]);
        assert_eq!(s.pattern_length(), PATTERN_PRESETS[2].len);
    }
}
