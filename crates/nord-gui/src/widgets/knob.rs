//! Rotary knob widget.

use super::basic::label;
use egui::{pos2, vec2, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui};

/// Rotary knob with 3D knurled rim, drop shadow, and crisp pointer notch.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    id: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    name: &str,
) -> bool {
    let hit = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));
    // Knobs that print numerals draw a smaller body; the hit area keeps the layout size.
    let radius = super::scale::body_radius(id, radius);
    let resp = ui.interact(hit, ui.id().with(id), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() {
        let d = -resp.drag_delta().y * 0.006 * (max - min);
        let nv = (*value + d).clamp(min, max);
        if nv != *value {
            *value = nv;
            changed = true;
        }
        // Parameter hint line on the OLEDs; `id` is the inventory id where the knob has one.
        let n = ((*value - min) / (max - min).max(1e-6)).clamp(0.0, 1.0);
        crate::panel::param_hint::touch(ui.ctx(), id, n);
    }
    #[cfg(test)]
    super::probe::record(
        ui.ctx(),
        super::probe::Kind::Knob,
        Rect::from_center_size(center, vec2(radius + 5.5, radius + 5.5) * 2.0),
        id,
    );
    let p = ui.painter();
    let norm = ((*value - min) / (max - min).max(1e-6)).clamp(0.0, 1.0);
    let a0 = 0.75 * std::f32::consts::PI;
    let sweep = 1.5 * std::f32::consts::PI;

    // Ticks around knob
    for i in 0..=10 {
        let a = a0 + sweep * (i as f32 / 10.0);
        let r1 = radius + 3.0;
        let r2 = radius + 5.5;
        p.line_segment(
            [
                pos2(center.x + a.cos() * r1, center.y + a.sin() * r1),
                pos2(center.x + a.cos() * r2, center.y + a.sin() * r2),
            ],
            Stroke::new(1.0_f32, Color32::from_gray(190)),
        );
    }

    super::scale::paint(p, center, radius, id);

    // 1. Drop shadow
    p.circle_filled(
        center + vec2(0.0, 2.0),
        radius + 1.5,
        Color32::from_black_alpha(100),
    );

    // 2. Outer knurled bezel ring
    p.circle_filled(center, radius + 0.8, Color32::from_rgb(18, 20, 24));
    p.circle_stroke(
        center,
        radius + 0.8,
        Stroke::new(1.0_f32, Color32::from_rgb(52, 56, 64)),
    );

    // 3. Dial face body
    p.circle_filled(center, radius, Color32::from_rgb(32, 35, 40));
    // Ambient specular sheen (top-left)
    p.circle_filled(
        center - vec2(0.6, 0.8),
        radius * 0.80,
        Color32::from_rgb(46, 50, 56),
    );
    // Center indented cap
    p.circle_filled(center, radius * 0.68, Color32::from_rgb(24, 26, 30));

    // 4. Pointer notch (beveled white indicator line)
    let a = a0 + sweep * norm;
    let p_start = pos2(
        center.x + a.cos() * (radius * 0.22),
        center.y + a.sin() * (radius * 0.22),
    );
    let p_end = pos2(
        center.x + a.cos() * (radius * 0.88),
        center.y + a.sin() * (radius * 0.88),
    );
    // Subtle shadow behind pointer
    p.line_segment(
        [p_start + vec2(0.5, 0.5), p_end + vec2(0.5, 0.5)],
        Stroke::new(2.8_f32, Color32::from_black_alpha(120)),
    );
    // Crisp white pointer
    p.line_segment(
        [p_start, p_end],
        Stroke::new(2.4_f32, Color32::from_rgb(245, 248, 252)),
    );

    if !name.is_empty() {
        label(
            p,
            pos2(center.x, center.y + radius + 13.0),
            name,
            9.5,
            Color32::from_gray(225),
            Align2::CENTER_CENTER,
        );
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::support::with_ui;
    use egui::pos2;

    #[test]
    fn test_knob_unchanged_without_input() {
        with_ui(|ui| {
            let mut v = 0.5_f32;
            assert!(!knob(
                ui,
                pos2(50.0, 50.0),
                14.0,
                "k",
                &mut v,
                0.0,
                1.0,
                "K"
            ));
            assert_eq!(v, 0.5);
        });
    }

    /// Dragging a knob that has an inventory id raises the parameter hint of that id.
    #[test]
    fn dragging_a_knob_raises_its_parameter_hint() {
        use egui::{Event, Modifiers, PointerButton, RawInput};
        let ctx = egui::Context::default();
        let c = pos2(50.0, 50.0);
        let mut v = 0.5_f32;
        let mut run = |events: Vec<Event>| {
            let _ = ctx.run(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        knob(ui, c, 14.0, "synth.lfo.mod_amt", &mut v, 0.0, 1.0, "");
                    });
                },
            );
        };
        assert_eq!(crate::panel::param_hint::active(&ctx), None);
        run(vec![Event::PointerMoved(c)]);
        run(vec![Event::PointerButton {
            pos: c,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        run(vec![Event::PointerMoved(pos2(c.x, c.y - 12.0))]);
        run(vec![Event::PointerMoved(pos2(c.x, c.y - 24.0))]);
        let h = crate::panel::param_hint::active(&ctx).expect("hint");
        assert!(h.starts_with("LFO Amount "), "{h}");
    }
}
