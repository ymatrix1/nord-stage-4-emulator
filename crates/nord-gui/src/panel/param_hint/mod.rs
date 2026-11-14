//! Parameter hint line: while a knob moves, the Synth OLED header and the lower half of the Program
//! OLED show `<label> <value>` (`LFO Rate 0.08 Hz`), as on the hardware (manual p.42; LFO course
//! video 0:27-0:45).
//!
//! Knob widgets call [`touch`] with their inventory id and normalised position; owners whose
//! value is not a plain sweep (Master Clock divisions, stored layer values) call [`touch_value`]
//! after it. The OLED painters read [`active`]. State lives in the egui context, so nothing is
//! threaded through the bays.

pub mod fmt;
pub mod table;

use table::lookup;

/// Seconds the hint stays up after the last movement.
/// ASSUMED: about 1.5 s; the course videos show the hint disappearing shortly after the knob stops
/// but no timing was measured. A frame-by-frame reading of the LFO clip would confirm it.
pub const HOLD_S: f64 = 1.5;

#[derive(Debug, Clone, PartialEq)]
struct Touched {
    id: String,
    norm: f32,
    value: Option<String>,
    at: f64,
}

fn key() -> egui::Id {
    egui::Id::new("param_hint.touched")
}

/// Knob `id` moved to normalised position `norm` (`0..=1`).
pub fn touch(ctx: &egui::Context, id: &str, norm: f32) {
    let at = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        d.insert_temp(
            key(),
            Touched {
                id: id.to_string(),
                norm,
                value: None,
                at,
            },
        )
    });
}

/// Replace the value text of the hint of knob `id` (steps, clock divisions, stored layer values).
pub fn touch_value(ctx: &egui::Context, id: &str, value: &str) {
    let at = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        d.insert_temp(
            key(),
            Touched {
                id: id.to_string(),
                norm: 0.0,
                value: Some(value.to_string()),
                at,
            },
        )
    });
}

/// The hint line to show now, if a knob moved within [`HOLD_S`]. Schedules the repaint that
/// removes it.
pub fn active(ctx: &egui::Context) -> Option<String> {
    let t: Touched = ctx.data(|d| d.get_temp(key()))?;
    let left = t.at + HOLD_S - ctx.input(|i| i.time);
    if left <= 0.0 {
        return None;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
    text(&t.id, t.norm, t.value.as_deref())
}

/// `<label> <value>` of control `id`; `None` for ids without a table row.
pub fn text(id: &str, norm: f32, value: Option<&str>) -> Option<String> {
    let row = lookup(id)?;
    let v = value.map(str::to_string).or_else(|| row.fmt.render(norm));
    Some(match v {
        Some(v) => format!("{} {v}", row.label),
        None => row.label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_time(t: f64) -> egui::RawInput {
        egui::RawInput {
            time: Some(t),
            ..Default::default()
        }
    }

    #[test]
    fn text_joins_label_and_value() {
        assert_eq!(
            text("synth.lfo.mod_amt", 0.71, None).as_deref(),
            Some("LFO Amount 7.1")
        );
        assert_eq!(
            text("synth.lfo.dest", 0.0, Some("OscCtrl")).as_deref(),
            Some("LFO Destination OscCtrl")
        );
        assert_eq!(
            text("synth.lfo.dest", 0.0, None).as_deref(),
            Some("LFO Destination")
        );
        assert_eq!(text("nope", 0.5, None), None);
    }

    #[test]
    fn a_touch_shows_then_expires_and_a_value_overrides() {
        let ctx = egui::Context::default();
        let _ = ctx.run(at_time(10.0), |ctx| {
            touch(ctx, "synth.lfo.mod_amt", 0.5);
            assert_eq!(active(ctx).as_deref(), Some("LFO Amount 5.0"));
            touch_value(ctx, "mod1.rate", "1/8");
            assert_eq!(active(ctx).as_deref(), Some("Mod 1 Rate 1/8"));
        });
        let _ = ctx.run(at_time(10.0 + HOLD_S + 0.1), |ctx| {
            assert_eq!(active(ctx), None);
        });
    }

    #[test]
    fn nothing_is_shown_before_any_touch() {
        let ctx = egui::Context::default();
        let _ = ctx.run(at_time(1.0), |ctx| assert_eq!(active(ctx), None));
    }
}
