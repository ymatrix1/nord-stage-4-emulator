//! Selector tables of the Layer Effects bay: LED order, labels and engine support.
//!
//! Order and names of the six LED positions per selector follow the manual (p.49-53) and the
//! firmware mode tables in `docs/specs/effects-and-routing.md` (Mod 1: A-PAN, TREM, RM, A-WAH,
//! WAH, PUMP; Mod 2: PHAS, FLANG, CHOR, VIBE, ENS, SPIN). The LED order is `ASSUMED` (the
//! photo does not resolve the LED labels); the names and the count of six are manual text.

use nord_core::fx::{DelayFeedbackFx, Mod1Type, Mod2Type, ReverbModel, DELAY_SYNC_BEATS};

/// One LED position of a type selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice<T> {
    /// Panel label (manual spelling).
    pub label: &'static str,
    /// Engine value, or `None` when the engine has no such type.
    pub engine: Option<T>,
    /// What is missing when `engine` is `None` (shown in the tooltip).
    pub missing: &'static str,
}

const fn ok<T>(label: &'static str, engine: T) -> Choice<T> {
    Choice {
        label,
        engine: Some(engine),
        missing: "",
    }
}

const fn missing<T>(label: &'static str, why: &'static str) -> Choice<T> {
    Choice {
        label,
        engine: None,
        missing: why,
    }
}

/// Mod 1 selector (manual p.49). A-WAH and PUMP have no engine type yet (`effects-and-routing.md`).
pub const MOD1: [Choice<Mod1Type>; 6] = [
    ok("A-PAN", Mod1Type::AutoPan),
    ok("TREM", Mod1Type::Tremolo),
    ok("RM", Mod1Type::RingMod),
    missing(
        "A-WAH",
        "A-WAH (envelope-follower wah) is not implemented in the engine",
    ),
    ok("WAH", Mod1Type::Wah),
    missing(
        "PUMP",
        "PUMP (duck-and-recover amplitude modulation) is not implemented in the engine",
    ),
];

/// Mod 2 selector (manual p.50). ENS and SPIN have no engine type yet.
pub const MOD2: [Choice<Mod2Type>; 6] = [
    ok("PHAS", Mod2Type::Phaser),
    ok("FLANG", Mod2Type::Flanger),
    ok("CHOR", Mod2Type::Chorus),
    ok("VIBE", Mod2Type::Vibe),
    missing(
        "ENS",
        "Mod 2 ENS (ensemble) is not implemented in the engine (only the delay feedback ENS is)",
    ),
    missing(
        "SPIN",
        "SPIN (rotating-speaker motion) is not implemented in the engine",
    ),
];

/// Delay feedback effect selector (manual p.51); the first LED is Bypass.
pub const DELAY_FX: [Choice<DelayFeedbackFx>; 6] = [
    ok("BYP", DelayFeedbackFx::Off),
    ok("ENS", DelayFeedbackFx::Ensemble),
    ok("VIBE", DelayFeedbackFx::Vibe),
    ok("CHOR", DelayFeedbackFx::Chorus),
    ok("FLAM", DelayFeedbackFx::Flam),
    ok("SPACE", DelayFeedbackFx::Space),
];

/// Reverb type selector (manual p.52): the three left types have a VARIATION, the three right
/// ones CHORALE (Shift + selector).
pub const REVERB: [Choice<ReverbModel>; 6] = [
    ok("SPRING", ReverbModel::Spring),
    ok("BOOTH", ReverbModel::Booth),
    ok("ROOM", ReverbModel::Room),
    ok("STAGE", ReverbModel::Stage),
    ok("HALL", ReverbModel::Hall),
    ok("CATH", ReverbModel::Cathedral),
];

/// Index of the first reverb type with CHORALE instead of VARIATION.
pub const REVERB_CHORALE_FROM: usize = 3;

/// Index of `Stage`, the engine default reverb.
pub const REVERB_DEFAULT: usize = 3;

/// MST CLK division labels; entry `n >= 1` is `DELAY_SYNC_BEATS[n - 1]` (plugin `SyncDivisionSel`).
pub const MST_LABELS: [&str; 18] = [
    "FREE", "1/2", "1/4D", "1/2T", "1/4S", "1/4", "1/8D", "1/4T", "1/8S", "1/8", "1/16D", "1/8T",
    "1/16S", "1/16", "1/16T", "1/32", "1/32T", "1/64",
];

/// Beats of MST CLK step `idx` (`0` = free running delay time).
pub fn mst_beats(idx: usize) -> Option<f32> {
    idx.checked_sub(1)
        .and_then(|i| DELAY_SYNC_BEATS.get(i).copied())
}

/// First selectable LED after `from` (wrapping); stays put when nothing else is available.
pub fn next_supported<T: Copy>(table: &[Choice<T>], from: usize) -> usize {
    let n = table.len();
    (1..=n)
        .map(|k| (from + k) % n)
        .find(|&i| table[i].engine.is_some())
        .unwrap_or(from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_have_six_led_positions() {
        assert_eq!(MOD1.len(), 6);
        assert_eq!(MOD2.len(), 6);
        assert_eq!(DELAY_FX.len(), 6);
        assert_eq!(REVERB.len(), 6);
    }

    #[test]
    fn unsupported_types_name_what_is_missing() {
        let dim: Vec<_> = MOD1
            .iter()
            .filter(|c| c.engine.is_none())
            .inspect(|c| assert!(!c.missing.is_empty()))
            .map(|c| c.label)
            .collect();
        assert_eq!(dim, ["A-WAH", "PUMP"]);
        let dim: Vec<_> = MOD2
            .iter()
            .filter(|c| c.engine.is_none())
            .inspect(|c| assert!(!c.missing.is_empty()))
            .map(|c| c.label)
            .collect();
        assert_eq!(dim, ["ENS", "SPIN"]);
    }

    #[test]
    fn mst_labels_cover_the_beat_table() {
        assert_eq!(MST_LABELS.len(), DELAY_SYNC_BEATS.len() + 1);
        assert_eq!(mst_beats(0), None);
        assert_eq!(mst_beats(1), Some(2.0));
        assert_eq!(mst_beats(17), Some(0.0625));
        assert_eq!(mst_beats(18), None);
        assert_eq!(MST_LABELS[1], "1/2");
        assert_eq!(MST_LABELS[17], "1/64");
    }

    #[test]
    fn cycling_skips_unsupported_types_and_wraps() {
        // WAH (4) -> A-PAN (0): PUMP (5) is skipped.
        assert_eq!(next_supported(&MOD1, 4), 0);
        // RM (2) -> WAH (4): A-WAH (3) is skipped.
        assert_eq!(next_supported(&MOD1, 2), 4);
        assert_eq!(next_supported(&MOD2, 3), 0);
        assert_eq!(next_supported(&REVERB, 5), 0);
    }

    #[test]
    fn chorale_split_matches_the_manual() {
        assert_eq!(REVERB[REVERB_CHORALE_FROM].label, "STAGE");
        assert_eq!(REVERB[REVERB_DEFAULT].engine, Some(ReverbModel::Stage));
    }
}
