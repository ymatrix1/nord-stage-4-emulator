//! Test-only record of what the widgets painted (frames and knob extents), read by the
//! panel legibility audit (`panel/legibility_tests.rs`). Not compiled into release builds.

use egui::{Context, Id, Rect};

/// Kind of a recorded rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Group frame whose title straddles the top border (`group_frame`).
    GroupBreak,
    /// Group frame with its title inside (rotary speaker box).
    GroupInside,
    /// Knob body plus tick ring.
    Knob,
    /// Everything a fader cap sweeps over (slot ends plus cap half-height and shadow).
    CapTravel,
    /// Bounding square of an LED (bezel included).
    Led,
}

/// One recorded rectangle.
#[derive(Clone, Debug)]
pub struct Rec {
    pub kind: Kind,
    pub rect: Rect,
    pub title: String,
}

fn key() -> Id {
    Id::new("widget-probe")
}

/// Record a rectangle painted this frame.
pub fn record(ctx: &Context, kind: Kind, rect: Rect, title: &str) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<Rec>>(key()).push(Rec {
            kind,
            rect,
            title: title.to_string(),
        });
    });
}

/// Record an LED whose dark bezel has radius `r` centred at `c` (test builds only).
pub fn led(p: &egui::Painter, c: egui::Pos2, r: f32) {
    let rect = Rect::from_center_size(c, egui::vec2(2.0 * r, 2.0 * r));
    record(p.ctx(), Kind::Led, rect, "");
}

/// Record the travel of a fader cap (test builds only).
pub fn cap_travel(ctx: &Context, rect: Rect, id: &str) {
    record(ctx, Kind::CapTravel, rect, id);
}

/// Everything recorded since the last [`clear`].
pub fn recorded(ctx: &Context) -> Vec<Rec> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<Rec>>(key()).clone())
}

/// Forget the records (start of a frame).
pub fn clear(ctx: &Context) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<Rec>>(key()).clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_clears() {
        let ctx = Context::default();
        record(&ctx, Kind::Knob, Rect::NOTHING, "k");
        assert_eq!(recorded(&ctx).len(), 1);
        clear(&ctx);
        assert!(recorded(&ctx).is_empty());
    }
}
