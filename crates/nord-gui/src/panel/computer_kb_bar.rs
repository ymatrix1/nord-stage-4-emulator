//! The "Computer keyboard" control group (bar under the panel): enable, base octave, velocity,
//! sustain indicator and a key legend. State and the key driver are in `computer_kb.rs`.

use super::computer_kb::ComputerKeyboard;
use super::computer_keys as map;
use super::prelude::*;

/// Name of the octave's C ("C4").
fn octave_label(octave: i8) -> String {
    format!("C{octave}")
}

/// The "Computer keyboard" control group (enable, base octave, velocity, pedal, key legend).
pub fn show_bar(ui: &mut Ui, kb: &mut ComputerKeyboard) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut kb.enabled, "Computer keyboard")
            .on_hover_text(
            "Play with the PC keys: A S D F G H J K L ; ' white, W E T Y U O P black (physical \
                 positions, any layout). Z / X octave, C / V velocity, Space sustain. While on, \
                 the P / Z / L shortcuts need Shift.",
        );
        ui.add_enabled_ui(kb.enabled, |ui| {
            let step_sz = egui::vec2(26.0, ui.spacing().interact_size.y);
            if ui
                .add(egui::Button::new("-").min_size(step_sz))
                .on_hover_text("Octave down (Z)")
                .clicked()
            {
                kb.octave = map::step_octave(kb.octave, -1);
            }
            ui.label(format!("Octave {}", octave_label(kb.octave)));
            if ui
                .add(egui::Button::new("+").min_size(step_sz))
                .on_hover_text("Octave up (X)")
                .clicked()
            {
                kb.octave = map::step_octave(kb.octave, 1);
            }
            ui.label("Velocity");
            ui.add(egui::Slider::new(&mut kb.velocity, 1..=127))
                .on_hover_text("Note On velocity of the computer keys (C down / V up)");
            let (txt, on) = if kb.sustain_down() {
                ("Sustain on", true)
            } else {
                ("Sustain: Space", false)
            };
            let _ = ui.selectable_label(on, txt);
        });
        ui.weak("A-' white   W-P black   Z/X octave   C/V velocity   Shift+P/Z/L view");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_draws_without_panicking() {
        let ctx = egui::Context::default();
        let mut kb = ComputerKeyboard::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show_bar(ui, &mut kb));
        });
        assert_eq!(octave_label(4), "C4");
    }
}
