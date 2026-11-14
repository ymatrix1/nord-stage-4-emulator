//! Zig Zag, fractional Range and the custom gate pattern as engine writes (manual p.35-37).
//!
//! One [`PatternParam`] is one plain-value write into `SynthLayer::perf` (host ids `synthp_*`).
//! Nothing here allocates, so the DSP thread applies it directly.

use nord_core::synth::{PatternPan, SynthLayer, MAX_RANGE_EXTRA_THIRDS};

/// A Zig Zag / Range fraction / custom pattern control value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternParam {
    /// ARP page 1 ZIG ZAG: two steps forward, one back.
    Zigzag(bool),
    /// RANGE knob detents between whole octaves: `0..=2` thirds on top of the octave range.
    RangeThirds(u8),
    /// Play the custom pattern (`true`) or the preset (`false`).
    Custom(bool),
    /// Copy preset `n` into the custom pattern and play the custom pattern.
    Customize(u8),
    /// Custom pattern length `2..=16` (ARP page 2 LENGTH).
    Len(u8),
    /// Gate length of `step` in eighths of a step, `0` = rest (ARP page 3).
    Gate { step: u8, eighths: u8 },
    /// Accent of `step` (ARP page 4).
    Accent { step: u8, on: bool },
    /// Pan of `step` (ARP page 5).
    Pan { step: u8, pan: PatternPan },
}

impl PatternParam {
    /// Write the value into `l` (DSP thread; no allocation, out-of-range values are clamped).
    pub fn apply(self, l: &mut SynthLayer) {
        match self {
            Self::Zigzag(b) => l.perf.zigzag = b,
            Self::RangeThirds(t) => l.perf.range_extra_thirds = t.min(MAX_RANGE_EXTRA_THIRDS),
            Self::Custom(b) => l.perf.pattern_custom = b,
            Self::Customize(n) => l.customize_preset(usize::from(n)),
            Self::Len(n) => l.set_pattern_length(n),
            Self::Gate { step, eighths } => l.set_step_gate(usize::from(step), eighths),
            Self::Accent { step, on } => l.set_step_accent(usize::from(step), on),
            Self::Pan { step, pan } => l.set_step_pan(usize::from(step), pan),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth::PATTERN_PRESETS;

    fn fresh() -> SynthLayer {
        SynthLayer::new(48_000.0)
    }

    #[test]
    fn zigzag_range_and_custom_flag_reach_the_perf_state() {
        let mut l = fresh();
        PatternParam::Zigzag(true).apply(&mut l);
        PatternParam::RangeThirds(9).apply(&mut l);
        PatternParam::Custom(true).apply(&mut l);
        assert!(l.perf.zigzag && l.perf.pattern_custom);
        assert_eq!(l.perf.range_extra_thirds, MAX_RANGE_EXTRA_THIRDS);
    }

    #[test]
    fn custom_pattern_edits_land_on_their_step_and_clamp() {
        let mut l = fresh();
        PatternParam::Len(99).apply(&mut l);
        assert_eq!(l.pattern_length(), 16);
        PatternParam::Gate {
            step: 3,
            eighths: 200,
        }
        .apply(&mut l);
        PatternParam::Accent { step: 4, on: true }.apply(&mut l);
        PatternParam::Pan {
            step: 5,
            pan: PatternPan::Left,
        }
        .apply(&mut l);
        assert_eq!(l.perf.custom.gate[3], 8);
        assert!(l.perf.custom.accent[4] && !l.perf.custom.accent[3]);
        assert_eq!(l.perf.custom.pan[5], PatternPan::Left);
        // An out-of-range step is ignored, not a panic.
        PatternParam::Gate {
            step: 200,
            eighths: 1,
        }
        .apply(&mut l);
    }

    #[test]
    fn customize_copies_the_preset_grid_and_switches_to_it() {
        let mut l = fresh();
        PatternParam::Customize(3).apply(&mut l);
        assert!(l.perf.pattern_custom);
        assert_eq!(l.perf.custom.gate, PATTERN_PRESETS[3].steps);
        assert_eq!(l.pattern_length(), PATTERN_PRESETS[3].len);
    }
}
