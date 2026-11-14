//! Custom arpeggiator pattern edits (`PatternParam`) to host parameters.
//!
//! Host ids: `synthp_<name>_<synth layer 1..3>`; per-step ids carry the 1-based step before the
//! layer (`synthp_step_gate_10_2` is step 10 of Synth B). Steps are 0-based in the panel edit.

use super::map::flag;
use super::write::ParamWrite as W;
use nord_core::synth::PatternPan;
use nord_gui::audio::PatternParam;

macro_rules! step_ids {
    ($prefix:literal) => {
        [
            concat!($prefix, "_1"),
            concat!($prefix, "_2"),
            concat!($prefix, "_3"),
            concat!($prefix, "_4"),
            concat!($prefix, "_5"),
            concat!($prefix, "_6"),
            concat!($prefix, "_7"),
            concat!($prefix, "_8"),
            concat!($prefix, "_9"),
            concat!($prefix, "_10"),
            concat!($prefix, "_11"),
            concat!($prefix, "_12"),
            concat!($prefix, "_13"),
            concat!($prefix, "_14"),
            concat!($prefix, "_15"),
            concat!($prefix, "_16"),
        ]
    };
}

const STEP_GATE: [&str; 16] = step_ids!("synthp_step_gate");
const STEP_ACCENT: [&str; 16] = step_ids!("synthp_step_accent");
const STEP_PAN: [&str; 16] = step_ids!("synthp_step_pan");

fn pan_index(p: PatternPan) -> f32 {
    match p {
        PatternPan::Left => 0.0,
        PatternPan::Center => 1.0,
        PatternPan::Right => 2.0,
    }
}

/// Map one pattern edit for synth layer `n` (1..=3). `Customize` copies a preset into the custom
/// pattern, which would need 48 writes of derived values; it has no mapping yet.
pub(super) fn map_pattern(p: PatternParam, n: u8, out: &mut Vec<W>) {
    match p {
        PatternParam::Zigzag(b) => out.push(W::layer("synthp_zigzag", n, flag(b))),
        PatternParam::RangeThirds(t) => {
            out.push(W::layer("synthp_range_frac", n, f32::from(t.min(2))))
        }
        PatternParam::Custom(b) => out.push(W::layer("synthp_pat_custom", n, flag(b))),
        PatternParam::Len(l) => out.push(W::layer("synthp_pat_len", n, f32::from(l))),
        PatternParam::Gate { step, eighths } => {
            if let Some(id) = STEP_GATE.get(usize::from(step)) {
                out.push(W::layer(id, n, f32::from(eighths.min(8))));
            }
        }
        PatternParam::Accent { step, on } => {
            if let Some(id) = STEP_ACCENT.get(usize::from(step)) {
                out.push(W::layer(id, n, flag(on)));
            }
        }
        PatternParam::Pan { step, pan } => {
            if let Some(id) = STEP_PAN.get(usize::from(step)) {
                out.push(W::layer(id, n, pan_index(pan)));
            }
        }
        PatternParam::Customize(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: PatternParam) -> Vec<W> {
        let mut v = Vec::new();
        map_pattern(p, 2, &mut v);
        v
    }

    #[test]
    fn step_edits_use_one_based_step_ids_then_the_layer() {
        assert_eq!(
            run(PatternParam::Gate {
                step: 9,
                eighths: 5
            }),
            vec![W::layer("synthp_step_gate_10", 2, 5.0)]
        );
        assert_eq!(
            run(PatternParam::Pan {
                step: 0,
                pan: PatternPan::Right
            }),
            vec![W::layer("synthp_step_pan_1", 2, 2.0)]
        );
        assert_eq!(
            run(PatternParam::Accent { step: 15, on: true }),
            vec![W::layer("synthp_step_accent_16", 2, 1.0)]
        );
        assert!(run(PatternParam::Gate {
            step: 16,
            eighths: 1
        })
        .is_empty());
    }

    #[test]
    fn scalar_edits_and_customize() {
        assert_eq!(
            run(PatternParam::Len(12)),
            vec![W::layer("synthp_pat_len", 2, 12.0)]
        );
        assert_eq!(
            run(PatternParam::RangeThirds(9)),
            vec![W::layer("synthp_range_frac", 2, 2.0)]
        );
        assert_eq!(
            run(PatternParam::Zigzag(true)),
            vec![W::layer("synthp_zigzag", 2, 1.0)]
        );
        assert_eq!(
            run(PatternParam::Custom(true)),
            vec![W::layer("synthp_pat_custom", 2, 1.0)]
        );
        assert!(run(PatternParam::Customize(3)).is_empty());
    }
}
