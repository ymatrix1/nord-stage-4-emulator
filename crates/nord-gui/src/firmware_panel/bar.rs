//! egui widgets of the firmware engine-path bar: one row of dropdowns / checkboxes per engine.

use super::choices::*;
use super::info::{self, SwitchInfo};
use super::presets::{self, Engine};
use nord_core::fx::FirmwareFx;
use nord_core::organ::ToneWheelSource;
use nord_core::render_opts::FirmwareOptions;

/// The optional FX ports in bar order: `FirmwareFx` port name and its switch description.
pub(super) const FX_PORTS: [(&str, SwitchInfo); 8] = [
    ("delay-laws", info::FX_DELAY_LAWS),
    ("delay-time", info::FX_DELAY_TIME),
    ("reverb-mix", info::FX_REVERB_MIX),
    ("reverb", info::REVERB),
    ("compressor", info::FX_COMPRESSOR),
    ("mod1", info::FX_MOD1),
    ("mod2", info::FX_MOD2),
    ("eq", info::FX_EQ),
];

/// Labelled combo box over `choices`, with the switch tooltip on the label and the box.
fn dropdown<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    sw: &SwitchInfo,
    value: &mut T,
    choices: &[Choice<T>],
) {
    ui.label(sw.label).on_hover_text(sw.tooltip());
    let shown = choices
        .iter()
        .find(|(v, ..)| v == value)
        .map_or("?", |(_, n, _)| *n);
    egui::ComboBox::from_id_salt(sw.flag)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, name, hint) in choices {
                ui.selectable_value(value, *v, *name).on_hover_text(*hint);
            }
        })
        .response
        .on_hover_text(sw.tooltip());
}

fn checkbox(ui: &mut egui::Ui, sw: &SwitchInfo, value: &mut bool) {
    ui.checkbox(value, sw.label).on_hover_text(sw.tooltip());
}

/// Group heading with its Classic / Firmware buttons (when `engine` is given), then the widgets;
/// `first` suppresses the leading separator. Returns the preset button clicked, if any.
fn group(
    ui: &mut egui::Ui,
    name: &str,
    first: bool,
    engine: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> Option<bool> {
    if !first {
        ui.separator();
    }
    ui.label(egui::RichText::new(name).strong());
    let mut clicked = None;
    if engine {
        for (label, fw, tip) in [
            ("C", false, "Classic: this group's pre-firmware paths"),
            ("F", true, "Firmware: this group's verified firmware paths"),
        ] {
            if ui.small_button(label).on_hover_text(tip).clicked() {
                clicked = Some(fw);
            }
        }
    }
    add(ui);
    clicked
}

/// Runs one group over a copy of the options and applies its engine preset click afterwards.
fn engine_group(
    ui: &mut egui::Ui,
    o: &mut FirmwareOptions,
    (name, engine): (&str, Engine),
    first: bool,
    add: impl FnOnce(&mut egui::Ui, &mut FirmwareOptions),
) {
    let mut next = *o;
    let clicked = group(ui, name, first, true, |ui| add(ui, &mut next));
    if let Some(fw) = clicked {
        presets::apply_engine(&mut next, engine, fw);
    }
    *o = next;
}

/// Shape: checkbox (off = saw placeholder) plus a 0..1 slider stored in thousandths.
fn shape(ui: &mut egui::Ui, value: &mut Option<u16>) {
    let mut on = value.is_some();
    ui.checkbox(&mut on, info::SHAPE.label)
        .on_hover_text(info::SHAPE.tooltip());
    if on {
        let mut x = f32::from(value.unwrap_or(0)) / 1000.0;
        ui.add(egui::Slider::new(&mut x, 0.0..=1.0).fixed_decimals(2));
        *value = Some((x * 1000.0).round() as u16);
    } else {
        *value = None;
    }
}

/// Leslie gain slider in dB, stored in millidecibel.
fn leslie_gain(ui: &mut egui::Ui, mdb: &mut i32) {
    ui.label(info::LESLIE_GAIN.label)
        .on_hover_text(info::LESLIE_GAIN.tooltip());
    let mut db = *mdb as f32 / 1000.0;
    ui.add(egui::Slider::new(&mut db, -24.0..=24.0).fixed_decimals(1));
    *mdb = (db * 1000.0).round() as i32;
}

/// One checkbox per optional FX port plus the all / none shortcut for them.
fn fx_ports(ui: &mut egui::Ui, fx: &mut FirmwareFx) {
    let all = fx.ports_on().len() == FX_PORTS.len();
    let mut on = all;
    if ui
        .checkbox(&mut on, info::FX_ALL.label)
        .on_hover_text(info::FX_ALL.tooltip())
        .changed()
    {
        for (name, _) in FX_PORTS {
            if let Some(p) = fx.port_mut(name) {
                *p = on;
            }
        }
    }
    for (name, sw) in &FX_PORTS {
        if let Some(p) = fx.port_mut(name) {
            checkbox(ui, sw, p);
        }
    }
}

/// The presets, the active set and the grouped switch rows. Mutates `o` in place.
///
/// Rows inside a horizontal scroll area: wrapped layouts clip combo boxes at the right edge, and
/// a narrow window then scrolls the bar instead of hiding switches.
pub fn show(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    egui::ScrollArea::horizontal()
        .id_salt("fw_bar_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let style = ui.style_mut();
            for id in [egui::TextStyle::Body, egui::TextStyle::Button] {
                if let Some(f) = style.text_styles.get_mut(&id) {
                    f.size = 12.5;
                }
            }
            style.spacing.item_spacing.y = 3.0;
            style.spacing.interact_size.y = 18.0;
            ui.horizontal(|ui| presets_and_status(ui, o));
            organ_row(ui, o);
            piano_row(ui, o);
            synth_row(ui, o);
            fx_row(ui, o);
        });
}

fn organ_row(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    ui.horizontal(|ui| {
        engine_group(ui, o, ("Organ", Engine::Organ), true, |ui, next| {
            dropdown(ui, &info::TONE, &mut next.tone_source, &TONES);
            let b3 = next.tone_source == Some(ToneWheelSource::B3Wheels);
            ui.add_enabled_ui(b3, |ui| {
                dropdown(ui, &info::B3_TAIL, &mut next.b3_tail, &TAILS);
            });
            let mut bass = next.b3_bass_processor == Some(true);
            checkbox(ui, &info::B3_BASS, &mut bass);
            next.b3_bass_processor = bass.then_some(true);
            let mut far = next.farfisa_processor == Some(true);
            checkbox(ui, &info::FARFISA, &mut far);
            next.farfisa_processor = far.then_some(true);
            dropdown(ui, &info::ROTARY, &mut next.rotary_drive, &ROTARIES);
            dropdown(ui, &info::ORGAN_LEVEL, &mut next.organ_level, &ORGAN_LEVELS);
            dropdown(ui, &info::LESLIE, &mut next.leslie_firmware, &LESLIES);
            ui.add_enabled_ui(next.leslie_firmware, |ui| {
                dropdown(ui, &info::LESLIE_RING, &mut next.leslie_ring, &RINGS);
                leslie_gain(ui, &mut next.leslie_gain_mdb);
            });
        });
    });
}

fn piano_row(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    ui.horizontal(|ui| {
        engine_group(ui, o, ("Piano", Engine::Piano), true, |ui, next| {
            dropdown(
                ui,
                &info::PIANO_LEVEL,
                &mut next.piano_firmware_level,
                &LEVELS,
            );
            checkbox(ui, &info::PIANO_BUS, &mut next.piano_bus_scale);
            checkbox(ui, &info::PIANO_CHAIN, &mut next.piano_chain_scale);
            checkbox(ui, &info::PIANO_LAYER_LAW, &mut next.piano_layer_law);
        });
        group(ui, "Global", false, false, |ui| {
            checkbox(ui, &info::ROUTING, &mut o.firmware_routing);
        });
    });
}

fn synth_row(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    ui.horizontal(|ui| {
        engine_group(ui, o, ("Synth", Engine::Synth), true, |ui, next| {
            dropdown(ui, &info::CHAIN, &mut next.synth_chain, &CHAINS);
            checkbox(ui, &info::STAGES, &mut next.synth_stages);
            checkbox(ui, &info::KNOB_LAWS, &mut next.synth_knob_laws);
            dropdown(ui, &info::P_TABLE, &mut next.synth_p, &P_TABLES);
            dropdown(ui, &info::FILTER, &mut next.filter_engine, &FILTERS);
            checkbox(ui, &info::MEASURED, &mut next.measured_laws);
            shape(ui, &mut next.synth_shape_milli);
            dropdown(
                ui,
                &info::SAMPLE_KERNEL,
                &mut next.sample_kernel_key,
                &KERNELS,
            );
            dropdown(ui, &info::SAMPLE_PITCH, &mut next.sample_pitch_fw, &PITCHES);
        });
    });
}

fn fx_row(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    ui.horizontal(|ui| {
        engine_group(ui, o, ("FX", Engine::Fx), true, |ui, next| {
            dropdown(ui, &info::AMP, &mut next.amp_path, &AMPS);
            fx_ports(ui, &mut next.fx);
        });
    });
}

fn presets_and_status(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    if ui
        .button("Firmware preset")
        .on_hover_text("--firmware-all: every verified path on (not measured laws, routing)")
        .clicked()
    {
        *o = presets::firmware();
    }
    if ui
        .button("Classic preset")
        .on_hover_text("--legacy-all plus the engine amp path: the pre-firmware engine")
        .clicked()
    {
        *o = presets::classic();
    }
    if ui
        .button("Defaults")
        .on_hover_text("Engine defaults (what a plain start gives): firmware synth strip + P table")
        .clicked()
    {
        *o = FirmwareOptions::default();
    }
    ui.add(egui::Label::new(format!("Engine: {}", o.summary())).truncate());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique<T>(c: &[Choice<T>]) -> bool {
        let mut n: Vec<_> = c.iter().map(|x| x.1).collect();
        n.sort_unstable();
        n.dedup();
        n.len() == c.len()
    }

    #[test]
    fn dropdown_names_are_unique_and_cover_every_variant() {
        assert!(unique(&TONES) && unique(&TAILS) && unique(&ROTARIES) && unique(&ORGAN_LEVELS));
        assert!(unique(&CHAINS) && unique(&FILTERS) && unique(&LEVELS) && unique(&AMPS));
        assert!(unique(&LESLIES) && unique(&RINGS) && unique(&KERNELS) && unique(&PITCHES));
        assert_eq!(ROTARIES.len(), 3);
        assert_eq!(TONES.len(), 6);
        assert_eq!(RINGS.len(), 3);
    }

    #[test]
    fn fx_port_rows_match_the_firmware_fx_port_names() {
        let names: Vec<&str> = FX_PORTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, nord_core::fx::FX_PORT_NAMES);
        let mut fx = FirmwareFx::OFF;
        for (n, _) in FX_PORTS {
            assert!(fx.port_mut(n).is_some(), "{n}");
        }
        fx = FirmwareFx::DEFAULT;
        assert_eq!(fx.ports_on().len(), 0);
    }

    #[test]
    fn classic_preset_is_the_engine_amp_with_legacy_piano() {
        let p = presets::classic();
        assert_eq!(p.amp_path, Some(false));
        assert!(!p.piano_firmware_level && p.tone_source.is_none());
    }

    #[test]
    fn shape_slider_round_trips_thousandths() {
        let ctx = egui::Context::default();
        let mut v = Some(250u16);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| shape(ui, &mut v));
        });
        assert_eq!(v, Some(250));
        let mut off = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| shape(ui, &mut off));
        });
        assert_eq!(off, None);
    }

    #[test]
    fn leslie_gain_slider_round_trips_millidecibels() {
        let ctx = egui::Context::default();
        let mut v = -3500i32;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| leslie_gain(ui, &mut v));
        });
        assert_eq!(v, -3500);
    }

    #[test]
    fn fx_all_checkbox_state_follows_the_ports() {
        let ctx = egui::Context::default();
        for start in [FirmwareFx::ALL, FirmwareFx::DEFAULT] {
            let mut fx = start;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| fx_ports(ui, &mut fx));
            });
            assert_eq!(fx, start, "no input leaves the ports unchanged");
        }
    }
}
