//! Organized shadcn-style card layout for the `Firmware & DSP` tab in Studio Settings.
//!
//! Replaces the cramped horizontal-scrolling bar with scannable per-engine cards
//! (Presets, Organ, Piano, Synth, Effects, Global) that require zero horizontal scrolling.

use super::bar::FX_PORTS;
use super::choices::*;
use super::info::{self, SwitchInfo};
use super::presets::{self, Engine};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};
use nord_core::fx::FirmwareFx;
use nord_core::organ::ToneWheelSource;
use nord_core::render_opts::FirmwareOptions;

const CARD_BG: Color32 = Color32::from_rgba_premultiplied(20, 21, 26, 215);
const BORDER: Color32 = Color32::from_rgb(42, 42, 48);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(250, 250, 250);
const TEXT_MUTED: Color32 = Color32::from_rgb(161, 161, 170);
const ACCENT_RED: Color32 = Color32::from_rgb(184, 37, 42);

fn section_card(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    engine: Option<Engine>,
    o: &mut FirmwareOptions,
    body: impl FnOnce(&mut Ui, &mut FirmwareOptions),
) {
    Frame::NONE
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(13.0).strong().color(TEXT_PRIMARY));
                    ui.label(RichText::new(subtitle).size(11.0).color(TEXT_MUTED));
                });
                if let Some(eng) = engine {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if ui
                            .button(RichText::new("Firmware").size(11.0))
                            .on_hover_text("Apply verified firmware paths for this engine")
                            .clicked()
                        {
                            presets::apply_engine(o, eng, true);
                        }
                        if ui
                            .button(RichText::new("Classic").size(11.0))
                            .on_hover_text("Apply classic pre-firmware paths for this engine")
                            .clicked()
                        {
                            presets::apply_engine(o, eng, false);
                        }
                    });
                }
            });
            ui.add_space(8.0);
            body(ui, o);
        });
}

fn grid_dropdown<T: PartialEq + Copy>(
    ui: &mut Ui,
    sw: &SwitchInfo,
    value: &mut T,
    choices: &[Choice<T>],
) {
    ui.label(RichText::new(sw.label).size(12.0).color(TEXT_MUTED))
        .on_hover_text(sw.tooltip());
    let shown = choices
        .iter()
        .find(|(v, ..)| v == value)
        .map_or("?", |(_, n, _)| *n);
    egui::ComboBox::from_id_salt(("card_fw", sw.flag))
        .selected_text(shown)
        .width(195.0)
        .show_ui(ui, |ui| {
            for (v, name, hint) in choices {
                ui.selectable_value(value, *v, *name)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(*hint);
            }
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(sw.tooltip());
}

fn toggle(ui: &mut Ui, sw: &SwitchInfo, value: &mut bool) {
    ui.checkbox(value, sw.label).on_hover_text(sw.tooltip());
}

fn preset_button(ui: &mut Ui, label: &str, active: bool, tip: &str) -> bool {
    let fill = if active {
        ACCENT_RED
    } else {
        Color32::from_rgb(28, 28, 33)
    };
    let stroke = if active {
        Stroke::NONE
    } else {
        Stroke::new(1.0, BORDER)
    };
    ui.add(
        egui::Button::new(RichText::new(label).size(11.5).strong().color(TEXT_PRIMARY))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(6)),
    )
    .on_hover_text(tip)
    .clicked()
}

fn presets_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    let is_fw = *o == presets::firmware();
    let is_default = *o == FirmwareOptions::default();
    let is_classic = *o == presets::classic();

    section_card(
        ui,
        "Engine Presets",
        "Switch all synthesis, filter, and FX engines between ARM/SHARC firmware laws and classic defaults.",
        None,
        o,
        |ui, o| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                if preset_button(
                    ui,
                    "Firmware (Recommended)",
                    is_fw,
                    "--firmware-all: every verified ARM/SHARC firmware path enabled",
                ) {
                    *o = presets::firmware();
                }
                if preset_button(
                    ui,
                    "Defaults",
                    is_default,
                    "Engine startup defaults: firmware synth strip + P table",
                ) {
                    *o = FirmwareOptions::default();
                }
                if preset_button(
                    ui,
                    "Classic (Legacy)",
                    is_classic,
                    "--legacy-all plus the classic engine amp path",
                ) {
                    *o = presets::classic();
                }
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Active profile: {}", o.summary()))
                    .size(11.0)
                    .monospace()
                    .color(TEXT_MUTED),
            );
        },
    );
}

fn organ_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    section_card(
        ui,
        "Organ Engine",
        "Tonewheel models, B3/Farfisa processors, rotary drive, and Leslie speaker emulation.",
        Some(Engine::Organ),
        o,
        |ui, o| {
            egui::Grid::new("fw_card_organ_grid")
                .num_columns(4)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    grid_dropdown(ui, &info::TONE, &mut o.tone_source, &TONES);
                    let b3 = o.tone_source == Some(ToneWheelSource::B3Wheels);
                    ui.add_enabled_ui(b3, |ui| {
                        grid_dropdown(ui, &info::B3_TAIL, &mut o.b3_tail, &TAILS);
                    });
                    ui.end_row();

                    grid_dropdown(ui, &info::ROTARY, &mut o.rotary_drive, &ROTARIES);
                    grid_dropdown(ui, &info::ORGAN_LEVEL, &mut o.organ_level, &ORGAN_LEVELS);
                    ui.end_row();

                    grid_dropdown(ui, &info::LESLIE, &mut o.leslie_firmware, &LESLIES);
                    ui.add_enabled_ui(o.leslie_firmware, |ui| {
                        grid_dropdown(ui, &info::LESLIE_RING, &mut o.leslie_ring, &RINGS);
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 6.0);
                let mut bass = o.b3_bass_processor == Some(true);
                toggle(ui, &info::B3_BASS, &mut bass);
                o.b3_bass_processor = bass.then_some(true);

                let mut far = o.farfisa_processor == Some(true);
                toggle(ui, &info::FARFISA, &mut far);
                o.farfisa_processor = far.then_some(true);

                ui.add_enabled_ui(o.leslie_firmware, |ui| {
                    ui.label(
                        RichText::new(info::LESLIE_GAIN.label)
                            .size(12.0)
                            .color(TEXT_MUTED),
                    )
                    .on_hover_text(info::LESLIE_GAIN.tooltip());
                    let mut db = o.leslie_gain_mdb as f32 / 1000.0;
                    ui.add(egui::Slider::new(&mut db, -24.0..=24.0).fixed_decimals(1));
                    o.leslie_gain_mdb = (db * 1000.0).round() as i32;
                });
            });
        },
    );
}

fn piano_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    section_card(
        ui,
        "Piano Engine",
        "Acoustic/electric piano level scaling, velocity touch curves, and gain slew.",
        Some(Engine::Piano),
        o,
        |ui, o| {
            egui::Grid::new("fw_card_piano_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    grid_dropdown(ui, &info::PIANO_LEVEL, &mut o.piano_firmware_level, &LEVELS);
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 6.0);
                toggle(ui, &info::PIANO_BUS, &mut o.piano_bus_scale);
                toggle(ui, &info::PIANO_TOUCH, &mut o.piano_touch);
                toggle(ui, &info::PIANO_SLEW, &mut o.piano_slew);
                toggle(ui, &info::PIANO_CHAIN, &mut o.piano_chain_scale);
                toggle(ui, &info::PIANO_LAYER_LAW, &mut o.piano_layer_law);
            });
        },
    );
}

fn synth_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    section_card(
        ui,
        "Synth Engine",
        "Oscillator strip, filter models, sample playback kernels, envelopes, and arpeggiator.",
        Some(Engine::Synth),
        o,
        |ui, o| {
            egui::Grid::new("fw_card_synth_grid")
                .num_columns(4)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    grid_dropdown(ui, &info::CHAIN, &mut o.synth_chain, &CHAINS);
                    grid_dropdown(ui, &info::P_TABLE, &mut o.synth_p, &P_TABLES);
                    ui.end_row();

                    grid_dropdown(ui, &info::FILTER, &mut o.filter_engine, &FILTERS);
                    grid_dropdown(ui, &info::SAMPLE_KERNEL, &mut o.sample_kernel_key, &KERNELS);
                    ui.end_row();

                    grid_dropdown(ui, &info::SAMPLE_PITCH, &mut o.sample_pitch_fw, &PITCHES);
                    ui.label(
                        RichText::new(info::SHAPE.label)
                            .size(12.0)
                            .color(TEXT_MUTED),
                    )
                    .on_hover_text(info::SHAPE.tooltip());
                    ui.horizontal(|ui| {
                        let mut on = o.synth_shape_milli.is_some();
                        ui.checkbox(&mut on, "")
                            .on_hover_text(info::SHAPE.tooltip());
                        if on {
                            let mut x = f32::from(o.synth_shape_milli.unwrap_or(0)) / 1000.0;
                            ui.add(egui::Slider::new(&mut x, 0.0..=1.0).fixed_decimals(2));
                            o.synth_shape_milli = Some((x * 1000.0).round() as u16);
                        } else {
                            o.synth_shape_milli = None;
                            ui.label(RichText::new("Off").size(11.5).color(TEXT_MUTED));
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 6.0);
                toggle(ui, &info::STAGES, &mut o.synth_stages);
                toggle(ui, &info::KNOB_LAWS, &mut o.synth_knob_laws);
                toggle(ui, &info::SYNTH_VELOCITY, &mut o.synth_velocity_fw);
                toggle(ui, &info::ARP, &mut o.arp_firmware);
                toggle(ui, &info::ARP_GATE, &mut o.arp_firmware_gate);
                toggle(ui, &info::MORPH, &mut o.firmware_morph);
                toggle(ui, &info::MEASURED, &mut o.measured_laws);
            });
        },
    );
}

fn fx_ports_wrapped(ui: &mut Ui, fx: &mut FirmwareFx) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(14.0, 6.0);
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
                toggle(ui, sw, p);
            }
        }
    });
}

fn fx_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    section_card(
        ui,
        "Effects (FX)",
        "Amp simulation path and per-effect firmware ports (Mod, Delay, Reverb, Compressor, EQ).",
        Some(Engine::Fx),
        o,
        |ui, o| {
            egui::Grid::new("fw_card_fx_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    grid_dropdown(ui, &info::AMP, &mut o.amp_path, &AMPS);
                    ui.end_row();
                });
            ui.add_space(8.0);
            fx_ports_wrapped(ui, &mut o.fx);
        },
    );
}

fn global_card(ui: &mut Ui, o: &mut FirmwareOptions) {
    section_card(
        ui,
        "Global Routing & Performance",
        "Bus routing laws, tick timing, and idle FX CPU optimization.",
        None,
        o,
        |ui, o| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 6.0);
                toggle(ui, &info::ROUTING, &mut o.firmware_routing);
                toggle(ui, &info::TICK, &mut o.measured_tick);
                toggle(ui, &info::IDLE_SKIP, &mut o.fx_idle_skip);
            });
        },
    );
}

/// Render all firmware DSP options as cleanly grouped vertical cards (no horizontal scroll).
pub fn show(ui: &mut Ui, o: &mut FirmwareOptions) {
    presets_card(ui, o);
    ui.add_space(10.0);
    organ_card(ui, o);
    ui.add_space(10.0);
    piano_card(ui, o);
    ui.add_space(10.0);
    synth_card(ui, o);
    ui.add_space(10.0);
    fx_card(ui, o);
    ui.add_space(10.0);
    global_card(ui, o);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organized_cards_render_all_sections_without_panics() {
        let ctx = egui::Context::default();
        let mut opts = FirmwareOptions::firmware_all();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, &mut opts);
            });
        });
        assert_eq!(opts, FirmwareOptions::firmware_all());
    }
}
