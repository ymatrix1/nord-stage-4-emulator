//! Library drawer and top-bar rendering.

use super::{BackupLibraryState, BackupTab};
use crate::audio::AudioHandle;
use crate::panel::Stage4PanelState;
use egui::{Color32, RichText};
use std::path::PathBuf;

impl BackupLibraryState {
    /// Renders the compact top library utility strip.
    pub fn render_top_bar(&mut self, ui: &mut egui::Ui, panel: &mut Stage4PanelState) {
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
            ui.checkbox(&mut panel.sync_knobs_on_program_load, "Sync Knobs on Preset")
                .on_hover_text(
                    "Real Nord Stage 4 knobs are non-motorized (OLED & LED ladders update on load, while pots keep their physical angle). Enable to move GUI knobs on preset load.",
                );

            ui.separator();
            ui.add_enabled_ui(self.backup_path.is_some(), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_path)
                        .desired_width(170.0)
                        .hint_text("Export path (.ns4b)"),
                );
                if ui
                    .button("Export backup")
                    .on_hover_text(
                        "Write the stored and organized programs and the Live slots into a copy of the opened .ns4b. The opened file is never changed.",
                    )
                    .clicked()
                {
                    let r = self.export_backup(&panel.program_ui.library);
                    self.report(r);
                }
                if ui
                    .button("Load Live slots")
                    .on_hover_text("Load the Live slots (.ns4l) of the opened backup into the eight Live programs.")
                    .clicked()
                {
                    let r = self.import_live_slots(&mut panel.program_ui.library);
                    self.report(r);
                }
            });

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

    /// Shows the outcome of an Export / Live import in the status strip.
    fn report(&mut self, result: Result<String, String>) {
        match result {
            Ok(msg) => {
                self.status_message = msg;
                self.error_message = None;
            }
            Err(err) => self.error_message = Some(err),
        }
    }

    /// L toggles the library drawer (like Z / P for the view). Ignored while a text field has
    /// the keyboard. Returns `true` when it toggled.
    pub fn handle_keys(&mut self, ctx: &egui::Context) -> bool {
        if ctx.wants_keyboard_input() {
            return false;
        }
        let l = ctx.input(|i| i.key_pressed(egui::Key::L) && !i.modifiers.any());
        if l {
            self.drawer_open = !self.drawer_open;
        }
        l
    }

    /// Renders the docked `.ns4b` archive browser side panel, sliding in and out with
    /// `drawer_open` (egui's animated side panel; it takes no space once fully closed).
    pub fn render_drawer(
        &mut self,
        ctx: &egui::Context,
        panel: &mut Stage4PanelState,
        audio: &AudioHandle,
    ) {
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
            .show_animated(ctx, self.drawer_open, |ui| {
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

    #[test]
    fn test_render_top_bar_and_closed_drawer_run_headless() {
        let mut lib = BackupLibraryState::new();
        let (audio, _rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| lib.render_top_bar(ui, &mut panel));
            lib.render_drawer(ctx, &mut panel, &audio);
        });
        assert!(!lib.drawer_open);
    }

    /// Headless harness: drawer + a central button, frames 50 ms apart.
    struct Rig {
        ctx: egui::Context,
        lib: BackupLibraryState,
        panel: Stage4PanelState,
        audio: AudioHandle,
        time: f64,
        left: f32,
        button: egui::Rect,
        clicks: u32,
    }

    impl Rig {
        fn new() -> Self {
            let mut rig = Self {
                ctx: egui::Context::default(),
                lib: BackupLibraryState::new(),
                panel: Stage4PanelState::default(),
                audio: AudioHandle::new().0,
                time: 1.0,
                left: 0.0,
                button: egui::Rect::NOTHING,
                clicks: 0,
            };
            rig.frame(vec![]);
            rig
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1200.0, 600.0),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let (lib, panel, audio) = (&mut self.lib, &mut self.panel, &self.audio);
            let (mut left, mut button, mut clicks) = (self.left, self.button, self.clicks);
            let _ = self.ctx.run(input, |ctx| {
                lib.render_drawer(ctx, panel, audio);
                left = ctx.available_rect().left();
                egui::CentralPanel::default().show(ctx, |ui| {
                    let r = ui.button("central control");
                    button = r.rect;
                    clicks += u32::from(r.clicked());
                });
            });
            (self.left, self.button, self.clicks) = (left, button, clicks);
        }

        fn settle(&mut self) {
            for _ in 0..12 {
                self.frame(vec![]);
            }
        }

        fn click(&mut self, at: egui::Pos2) {
            let ev = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(at)]);
            self.frame(vec![ev(true)]);
            self.frame(vec![ev(false)]);
        }
    }

    #[test]
    fn drawer_slides_in_and_out_and_takes_no_space_when_closed() {
        let mut rig = Rig::new();
        rig.settle();
        assert_eq!(rig.left, 0.0, "closed drawer leaves the panel full width");
        rig.lib.drawer_open = true;
        rig.frame(vec![]);
        let mid = rig.left;
        rig.settle();
        let open = rig.left;
        assert!((260.0..=580.0).contains(&open), "open width {open}");
        assert!(mid < open, "slides in over several frames ({mid} < {open})");
        rig.lib.drawer_open = false;
        rig.frame(vec![]);
        assert!(rig.left < open, "slides out");
        rig.settle();
        assert_eq!(rig.left, 0.0);
    }

    #[test]
    fn clicks_reach_the_panel_beside_the_drawer_and_not_through_it() {
        let mut rig = Rig::new();
        rig.lib.drawer_open = true;
        rig.settle();
        assert!(
            rig.button.left() >= rig.left,
            "button sits right of the drawer"
        );
        rig.click(rig.button.center());
        assert_eq!(rig.clicks, 1, "the panel stays clickable beside the drawer");
        let inside = egui::pos2(rig.left * 0.5, 300.0);
        rig.click(inside);
        assert_eq!(
            rig.clicks, 1,
            "a click inside the drawer never reaches the panel"
        );
        assert!(rig.lib.drawer_open);
    }

    #[test]
    fn l_toggles_the_drawer() {
        let mut rig = Rig::new();
        let l = |pressed| egui::Event::Key {
            key: egui::Key::L,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut toggled = false;
        rig.time += 0.05;
        let input = egui::RawInput {
            time: Some(rig.time),
            events: vec![l(true)],
            ..Default::default()
        };
        let lib = &mut rig.lib;
        let _ = rig.ctx.run(input, |ctx| toggled = lib.handle_keys(ctx));
        assert!(toggled && rig.lib.drawer_open);
    }
}
