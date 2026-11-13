//! egui rendering of the library top bar and `.ns4b` browser drawer.

use super::{BackupLibraryState, BackupTab};
use crate::audio::AudioHandle;
use crate::panel::Stage4PanelState;
use egui::{Color32, RichText};
use std::path::PathBuf;

impl BackupLibraryState {
    /// Renders the compact top library utility strip.
    pub fn render_top_bar(&mut self, ui: &mut egui::Ui, sync_knobs_on_load: &mut bool) {
        ui.horizontal(|ui| {
            let toggle_label = if self.drawer_open {
                "Close Library"
            } else {
                "Open .ns4b Library"
            };
            if ui.button(toggle_label).clicked() {
                self.drawer_open = !self.drawer_open;
                if self.drawer_open && self.archive.is_none() && !self.path_input.is_empty() {
                    let p = PathBuf::from(self.path_input.trim());
                    if p.is_file() {
                        if let Err(err) = self.open_backup_path(&p) {
                            self.error_message = Some(err);
                        }
                    }
                }
            }

            ui.separator();
            ui.checkbox(sync_knobs_on_load, "Sync Knobs on Preset")
                .on_hover_text(
                    "Real Nord Stage 4 knobs are non-motorized (OLED & LED ladders update on load, while pots keep their physical angle). Enable to move GUI knobs on preset load.",
                );

            ui.separator();
            if self.pending_jobs > 0 {
                ui.spinner();
            }
            if let Some(err) = &self.error_message {
                ui.colored_label(Color32::from_rgb(255, 135, 135), err);
            } else {
                ui.label(RichText::new(&self.status_message).color(Color32::LIGHT_GRAY));
            }
        });
    }

    /// Renders the docked `.ns4b` archive browser side panel when `drawer_open` is true.
    pub fn render_drawer(
        &mut self,
        ctx: &egui::Context,
        panel: &mut Stage4PanelState,
        audio: &AudioHandle,
    ) {
        if !self.drawer_open {
            return;
        }
        egui::SidePanel::left("ns4_library_panel")
            .resizable(true)
            .default_width(360.0)
            .min_width(260.0)
            .max_width(580.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(Color32::from_rgb(36, 42, 48))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(
                        RichText::new("Nord Stage 4 Backup & Sample Library")
                            .color(Color32::WHITE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("X").on_hover_text("Close Library").clicked() {
                            self.drawer_open = false;
                        }
                        ui.checkbox(&mut self.auto_close_on_load, "Auto-close")
                            .on_hover_text("Close library panel after selecting a preset or sample");
                    });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Backup (.ns4b):");
                    let input_w = (ui.available_width() - 56.0).max(110.0);
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .desired_width(input_w)
                            .hint_text("Path to .ns4b backup (or drag-and-drop file)"),
                    );
                    let enter =
                        edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Open").clicked() || enter {
                        let p = PathBuf::from(self.path_input.trim());
                        if let Err(err) = self.open_backup_path(&p) {
                            self.error_message = Some(err);
                        }
                    }
                });

                if let Some(ver) = &self.manifest_summary {
                    ui.small(format!(
                        "Archive OS: {ver} | Programs: {} | Pianos: {} | Samples: {}",
                        self.programs.len(),
                        self.pianos.len(),
                        self.samples.len()
                    ));
                }

                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut self.active_tab,
                        BackupTab::Programs,
                        format!("Programs ({})", self.programs.len()),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        BackupTab::Pianos,
                        format!("Pianos ({})", self.pianos.len()),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        BackupTab::Samples,
                        format!("Samples ({})", self.samples.len()),
                    );
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Filter:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .desired_width(150.0)
                            .hint_text("Search name..."),
                    );
                    if self.active_tab == BackupTab::Pianos {
                        ui.label("Target:");
                        ui.selectable_value(&mut self.piano_slot_b, false, "Piano A");
                        ui.selectable_value(&mut self.piano_slot_b, true, "Piano B");
                    } else if self.active_tab == BackupTab::Samples {
                        ui.label("Target:");
                        ui.selectable_value(&mut self.synth_layer_idx, 0, "Synth A");
                        ui.selectable_value(&mut self.synth_layer_idx, 1, "Synth B");
                        ui.selectable_value(&mut self.synth_layer_idx, 2, "Synth C");
                    }
                });

                ui.separator();
                let query = self.filter.trim().to_ascii_lowercase();
                let mut selected_program: Option<String> = None;
                let mut selected_piano: Option<String> = None;
                let mut selected_sample: Option<String> = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let items: &[String] = match self.active_tab {
                            BackupTab::Programs => &self.programs,
                            BackupTab::Pianos => &self.pianos,
                            BackupTab::Samples => &self.samples,
                        };
                        if items.is_empty() {
                            ui.weak("No entries loaded. Open a .ns4b backup or drag-and-drop .ns4p / .npno / .nsmp4 onto the window.");
                            return;
                        }
                        for entry in items {
                            if !query.is_empty() && !entry.to_ascii_lowercase().contains(&query) {
                                continue;
                            }
                            if ui.selectable_label(false, entry).clicked() {
                                match self.active_tab {
                                    BackupTab::Programs => selected_program = Some(entry.clone()),
                                    BackupTab::Pianos => selected_piano = Some(entry.clone()),
                                    BackupTab::Samples => selected_sample = Some(entry.clone()),
                                }
                            }
                        }
                    });

                if let Some(p) = selected_program {
                    self.load_program_entry(&p, panel, audio);
                }
                if let Some(p) = selected_piano {
                    self.request_piano_entry(&p);
                }
                if let Some(s) = selected_sample {
                    self.request_sample_entry(&s);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_frame(f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), f);
    }

    #[test]
    fn test_render_drawer_is_noop_when_closed_and_runs_when_open() {
        let mut lib = BackupLibraryState::new();
        let mut panel = Stage4PanelState::default();
        let (audio, _rx) = AudioHandle::new();
        run_frame(|ctx| lib.render_drawer(ctx, &mut panel, &audio));
        lib.drawer_open = true;
        run_frame(|ctx| lib.render_drawer(ctx, &mut panel, &audio));
        assert!(lib.drawer_open);
    }

    #[test]
    fn test_render_top_bar_runs_headless() {
        let mut lib = BackupLibraryState::new();
        let mut sync = true;
        run_frame(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| lib.render_top_bar(ui, &mut sync));
        });
        assert!(sync);
    }
}
