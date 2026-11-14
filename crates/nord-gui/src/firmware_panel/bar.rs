//! egui widgets of the firmware engine-path bar: one row of dropdowns / checkboxes per engine.

use super::info::{self, SwitchInfo};
use nord_core::organ::{OutputLevel, ToneWheelSource};
use nord_core::render_opts::{FirmwareOptions, RotaryDrive};
use nord_core::synth::{FilterEngine, PSpec, PTableSpec, SynthChain};

/// Dropdown entry: value, name, per-choice status hint shown when hovering the entry.
type Choice<T> = (T, &'static str, &'static str);

const TONES: [Choice<Option<ToneWheelSource>>; 6] = [
    (None, "Model default", "Per-model default source"),
    (Some(ToneWheelSource::Legacy), "Legacy", "ASSUMED"),
    (
        Some(ToneWheelSource::Firmware),
        "Pipe (firmware)",
        "VERIFIED",
    ),
    (
        Some(ToneWheelSource::B3Wheels),
        "B3 wheels (firmware)",
        "VERIFIED ladder, per-key weights not modelled",
    ),
    (
        Some(ToneWheelSource::Vox),
        "Vox bank (firmware)",
        "partial, processors not modelled",
    ),
    (
        Some(ToneWheelSource::FarfisaWheels),
        "Farfisa (firmware)",
        "partial, processors not modelled",
    ),
];
const TAILS: [Choice<Option<bool>>; 2] = [
    (None, "On (default)", "EMPIRICAL"),
    (Some(false), "Off", "EMPIRICAL"),
];
const ROTARIES: [Choice<RotaryDrive>; 3] = [
    (
        RotaryDrive::Engine,
        "Engine",
        "tanh(g x), program drive as stored",
    ),
    (RotaryDrive::Unity, "Unity", "tanh(g x) / g, EMPIRICAL"),
    (RotaryDrive::Scaled, "Scaled /10", "drive / 10, EMPIRICAL"),
];
const ORGAN_LEVELS: [Choice<OutputLevel>; 4] = [
    (
        OutputLevel::ModelDefault,
        "Model default",
        "firmware for Pipe, legacy otherwise",
    ),
    (OutputLevel::Legacy, "Legacy", "bus scale 0.08"),
    (
        OutputLevel::Firmware,
        "Firmware",
        "swell word, EMPIRICAL gain K",
    ),
    (
        OutputLevel::Chain,
        "Verified chain",
        "word 1.6 x FX path 0.3732, VERIFIED (13 dB under the clips)",
    ),
];
const CHAINS: [Choice<Option<SynthChain>>; 2] = [
    (Some(SynthChain::Legacy), "Classic", "engine chain"),
    (
        Some(SynthChain::FirmwareStrip),
        "Firmware strip (default)",
        "partly VERIFIED, P table EMPIRICAL",
    ),
];
const P_TABLES: [Choice<Option<PSpec>>; 3] = [
    (
        Some(PSpec::Table(PTableSpec::BUILTIN)),
        "Per-wave table (default)",
        "EMPIRICAL, docs/research/synth-validation.md",
    ),
    (
        Some(PSpec::Fitted),
        "Smooth Lead fit",
        "EMPIRICAL, one program",
    ),
    (None, "Off (unit P)", "no level or cutoff correction"),
];
const FILTERS: [Choice<Option<FilterEngine>>; 2] = [
    (None, "ZDF (default)", "engine filter"),
    (Some(FilterEngine::Firmware), "Firmware", "VERIFIED"),
];
const LEVELS: [Choice<bool>; 2] = [
    (false, "Legacy", "touch curve"),
    (true, "Firmware", "VERIFIED level model for .npno"),
];
const AMPS: [Choice<Option<bool>>; 3] = [
    (None, "Default (firmware)", "VERIFIED drive stage"),
    (Some(false), "Engine", "tanh drive and RBJ cabinet"),
    (Some(true), "Firmware", "VERIFIED drive stage"),
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

/// Group heading followed by its widgets; `first` suppresses the leading separator.
fn group(ui: &mut egui::Ui, name: &str, first: bool, add: impl FnOnce(&mut egui::Ui)) {
    if !first {
        ui.separator();
    }
    ui.label(egui::RichText::new(name).strong());
    add(ui);
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

/// The presets, the active set and the grouped switch rows. Mutates `o` in place.
///
/// Three compact rows inside a horizontal scroll area: wrapped layouts clip combo boxes at the
/// right edge, and a narrow window then scrolls the bar instead of hiding switches.
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
            ui.horizontal(|ui| {
                group(ui, "Organ", true, |ui| {
                    dropdown(ui, &info::TONE, &mut o.tone_source, &TONES);
                    let b3 = o.tone_source == Some(ToneWheelSource::B3Wheels);
                    ui.add_enabled_ui(b3, |ui| {
                        dropdown(ui, &info::B3_TAIL, &mut o.b3_tail, &TAILS);
                    });
                    dropdown(ui, &info::ROTARY, &mut o.rotary_drive, &ROTARIES);
                    dropdown(ui, &info::ORGAN_LEVEL, &mut o.organ_level, &ORGAN_LEVELS);
                });
                group(ui, "Piano", false, |ui| {
                    dropdown(ui, &info::PIANO_LEVEL, &mut o.piano_firmware_level, &LEVELS);
                    checkbox(ui, &info::PIANO_BUS, &mut o.piano_bus_scale);
                    checkbox(ui, &info::PIANO_CHAIN, &mut o.piano_chain_scale);
                });
                group(ui, "FX", false, |ui| {
                    dropdown(ui, &info::AMP, &mut o.amp_path, &AMPS);
                    checkbox(ui, &info::FX_PORTS, &mut o.firmware_fx);
                    checkbox(ui, &info::REVERB, &mut o.firmware_reverb);
                });
            });
            ui.horizontal(|ui| {
                group(ui, "Synth", true, |ui| {
                    dropdown(ui, &info::CHAIN, &mut o.synth_chain, &CHAINS);
                    checkbox(ui, &info::STAGES, &mut o.synth_stages);
                    dropdown(ui, &info::P_TABLE, &mut o.synth_p, &P_TABLES);
                    dropdown(ui, &info::FILTER, &mut o.filter_engine, &FILTERS);
                    checkbox(ui, &info::MEASURED, &mut o.measured_laws);
                    shape(ui, &mut o.synth_shape_milli);
                });
                group(ui, "Global", false, |ui| {
                    checkbox(ui, &info::ROUTING, &mut o.firmware_routing);
                });
            });
        });
}

fn presets_and_status(ui: &mut egui::Ui, o: &mut FirmwareOptions) {
    if ui
        .button("Firmware preset")
        .on_hover_text("--firmware-all: every verified path on (not measured laws, routing)")
        .clicked()
    {
        *o = FirmwareOptions::firmware_all();
    }
    if ui
        .button("Legacy preset")
        .on_hover_text("--legacy-all plus the engine amp path: the pre-firmware engine")
        .clicked()
    {
        *o = legacy_preset();
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

/// Pre-firmware engine: `--legacy-all` plus the engine amp path.
pub fn legacy_preset() -> FirmwareOptions {
    FirmwareOptions {
        amp_path: Some(false),
        ..FirmwareOptions::legacy_all()
    }
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
        assert_eq!(ROTARIES.len(), 3);
        assert_eq!(TONES.len(), 6);
    }

    #[test]
    fn legacy_preset_is_the_engine_amp_with_legacy_piano() {
        let p = legacy_preset();
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
}
