//! MORPH ASSIGN (manual p.38-39): hold-to-assign, double-tap latch, depth editing, clear.
//!
//! The manual assigns a morph by holding a Morph Assign button while moving a panel control.
//! Controls of the other bays are not reachable from here, so the Program area edits the depth
//! of the targets it owns (7 layer levels, 3 synth cutoffs): dial = depth, PAGE = target.

use crate::audio::{MorphTarget, ProgramEdit};
use nord_core::morph::MorphSource;
use nord_panel::behaviour::Latch;

/// Depth step per dial click.
///
/// ASSUMED: 0.05 (20 clicks end to end); the manual does not give a resolution.
pub const DEPTH_STEP: f32 = 0.05;

/// The three sources in button order WHEEL, A.T., CTRLPED.
pub const SOURCES: [MorphSource; 3] = [
    MorphSource::ModWheel,
    MorphSource::Aftertouch,
    MorphSource::ControlPedal,
];

/// Index of `s` in [`SOURCES`].
pub fn source_index(s: MorphSource) -> usize {
    SOURCES.iter().position(|&x| x == s).unwrap_or(0)
}

/// Label as printed on the panel.
pub fn source_label(s: MorphSource) -> &'static str {
    match s {
        MorphSource::ModWheel => "WHEEL",
        MorphSource::Aftertouch => "A.T.",
        MorphSource::ControlPedal => "CTRLPED",
    }
}

/// Morph assign state.
#[derive(Debug, Clone, Default)]
pub struct MorphUi {
    held: Option<MorphSource>,
    latch: Latch,
    latch_source: Option<MorphSource>,
    /// Source shown by a plain tap.
    pub viewed: Option<MorphSource>,
    /// Target being edited, `0..MorphTarget::COUNT`.
    pub target: usize,
    depth: [[f32; MorphTarget::COUNT]; 3],
}

impl MorphUi {
    /// Source being assigned: the held button, else the latched one.
    pub fn assigning(&self) -> Option<MorphSource> {
        self.held.or(if self.latch.is_active() {
            self.latch_source
        } else {
            None
        })
    }

    /// A Morph Assign button went down.
    pub fn down(&mut self, s: MorphSource) {
        self.held = Some(s);
    }

    /// A Morph Assign button went up.
    pub fn up(&mut self, s: MorphSource) {
        if self.held == Some(s) {
            self.held = None;
        }
    }

    /// Double tap: enter the latch for `s`; a second double tap leaves it.
    pub fn double_tap(&mut self, s: MorphSource) {
        if self.latch.is_active() && self.latch_source == Some(s) {
            self.exit();
        } else {
            self.latch = Latch::default();
            self.latch.double_tap();
            self.latch_source = Some(s);
        }
    }

    /// Shift/Exit: leave the latch.
    pub fn exit(&mut self) {
        self.latch.exit();
        self.latch_source = None;
        self.held = None;
    }

    /// Stored depth of `source` on the current target.
    pub fn depth(&self, source: MorphSource) -> f32 {
        self.depth[source_index(source)][self.target]
    }

    /// Depth of `source` on target `t`.
    pub fn depth_at(&self, source: MorphSource, t: usize) -> f32 {
        self.depth[source_index(source)][t.min(MorphTarget::COUNT - 1)]
    }

    /// Dial turned while assigning: depth edit of the current target.
    pub fn dial(&mut self, delta: i32) -> Option<ProgramEdit> {
        let source = self.assigning()?;
        let d = &mut self.depth[source_index(source)][self.target];
        *d = ((*d + delta as f32 * DEPTH_STEP) * 100.0).round() / 100.0;
        *d = d.clamp(-1.0, 1.0);
        Some(ProgramEdit::MorphDepth {
            source,
            target: MorphTarget::from_index(self.target),
            depth: *d,
        })
    }

    /// PAGE buttons while assigning: previous/next target (wraps).
    pub fn page(&mut self, delta: i32) {
        self.target = (self.target as i32 + delta).rem_euclid(MorphTarget::COUNT as i32) as usize;
    }

    /// Shift+Morph Assign: clear every depth of `s`.
    pub fn clear(&mut self, s: MorphSource) -> ProgramEdit {
        self.depth[source_index(s)] = [0.0; MorphTarget::COUNT];
        ProgramEdit::ClearMorph(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_assigns_only_while_down() {
        let mut m = MorphUi::default();
        assert_eq!(m.assigning(), None);
        m.down(MorphSource::ModWheel);
        assert_eq!(m.assigning(), Some(MorphSource::ModWheel));
        m.up(MorphSource::ModWheel);
        assert_eq!(m.assigning(), None);
    }

    #[test]
    fn double_tap_latches_and_exit_clears() {
        let mut m = MorphUi::default();
        m.double_tap(MorphSource::Aftertouch);
        assert_eq!(m.assigning(), Some(MorphSource::Aftertouch));
        m.double_tap(MorphSource::ControlPedal);
        assert_eq!(
            m.assigning(),
            Some(MorphSource::ControlPedal),
            "other source re-latches"
        );
        m.double_tap(MorphSource::ControlPedal);
        assert_eq!(m.assigning(), None, "same source toggles off");
        m.double_tap(MorphSource::ModWheel);
        m.exit();
        assert_eq!(m.assigning(), None);
    }

    #[test]
    fn dial_sets_depth_of_the_current_target_and_clamps() {
        let mut m = MorphUi::default();
        assert!(m.dial(1).is_none(), "not assigning: no edit");
        m.double_tap(MorphSource::ModWheel);
        m.page(2);
        let Some(ProgramEdit::MorphDepth {
            source,
            target,
            depth,
        }) = m.dial(4)
        else {
            panic!("expected a depth edit");
        };
        assert_eq!(source, MorphSource::ModWheel);
        assert_eq!(target, MorphTarget::LayerLevel(2));
        assert!((depth - 0.2).abs() < 1e-6);
        m.dial(100);
        assert_eq!(m.depth(MorphSource::ModWheel), 1.0);
        assert_eq!(m.depth_at(MorphSource::Aftertouch, 2), 0.0);
    }

    #[test]
    fn page_wraps_over_ten_targets_and_clear_zeroes_the_source() {
        let mut m = MorphUi::default();
        m.page(-1);
        assert_eq!(m.target, MorphTarget::COUNT - 1);
        m.double_tap(MorphSource::ControlPedal);
        m.dial(3);
        let e = m.clear(MorphSource::ControlPedal);
        assert!(matches!(
            e,
            ProgramEdit::ClearMorph(MorphSource::ControlPedal)
        ));
        assert_eq!(m.depth(MorphSource::ControlPedal), 0.0);
        assert_eq!(source_label(MorphSource::Aftertouch), "A.T.");
        assert_eq!(source_index(MorphSource::ControlPedal), 2);
    }
}
