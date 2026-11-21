//! "Nord OS update file" setting of the standalone GUI (D129).
//!
//! One-time setup for a layperson: download the official Nord OS update (link in the window), drop
//! that file on the window or paste its path, press Use this file. The app reads the WAVE tables
//! from it at start-up and remembers only the path (`nord_core::user_config`). The tables are
//! installed once per process, so a newly chosen file applies after the next start.

use nord_core::firmware_setting::{self, Source, Status};
use std::path::{Path, PathBuf};

/// State of the setting window and its status line.
pub struct OsUpdatePanel {
    config: Option<PathBuf>,
    /// What the running session was started with (`None` = placeholder).
    session: Option<PathBuf>,
    status: Status,
    /// The window is open.
    pub open: bool,
    input: String,
    message: Option<String>,
}

impl OsUpdatePanel {
    /// `env` is the value of `NS4_WAVE_TABLES` at start (after the saved default was applied).
    pub fn new(config: Option<PathBuf>, env: Option<&str>) -> Self {
        let status = match &config {
            Some(c) => firmware_setting::status(env, c),
            None => firmware_setting::status(env, Path::new("")),
        };
        let session = status.path.as_ref().map(|(p, _)| p.clone());
        Self {
            config,
            session,
            status,
            open: false,
            input: String::new(),
            message: None,
        }
    }

    /// Validates `text` as a path, saves it and refreshes the status; the message tells the user
    /// what happened in plain words.
    pub fn use_file(&mut self, text: &str) {
        let text = text.trim().trim_matches('"');
        let Some(config) = self.config.clone() else {
            self.message = Some("This system has no place to save settings.".into());
            return;
        };
        self.message = Some(match firmware_setting::set(Path::new(text), &config) {
            Ok(n) => {
                self.status = firmware_setting::status(None, &config);
                if self.status.path.as_ref().map(|(p, _)| p) == self.session.as_ref() {
                    format!("OK: {n} WAVE tables read.")
                } else {
                    format!("OK: {n} WAVE tables read. Restart the app to use them.")
                }
            }
            Err(e) => format!("Not used: {e}"),
        });
    }

    /// One line for the bar.
    pub fn summary(&self) -> String {
        match (&self.status.path, &self.status.tables) {
            (None, _) if self.status.config_error.is_some() => {
                "WAVE sound: placeholder (saved settings problem)".into()
            }
            (None, _) => "WAVE sound: placeholder (no Nord OS update file set)".into(),
            (Some(_), Some(Ok(n))) => format!("WAVE sound: {n} Nord tables"),
            (Some(_), Some(Err(_))) => "WAVE sound: placeholder (update file problem)".into(),
            (Some(_), None) => String::new(),
        }
    }

    /// Bar widget: status text and the button that opens the window.
    pub fn show_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(self.summary());
            if ui.small_button("Nord OS update file…").clicked() {
                self.open = !self.open;
            }
        });
    }

    /// Inline body (for embedding inside the shadcn Studio Settings tab without a popup window).
    pub fn show_inline(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.input = p.display().to_string();
            self.use_file(&self.input.clone());
        }
        self.window_body(ui);
    }

    /// The setting window, plus drag-and-drop of the downloaded file onto the app.
    pub fn show_window(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.open = true;
            self.input = p.display().to_string();
            self.use_file(&self.input.clone());
        }
        let mut open = self.open;
        egui::Window::new("Nord OS update file")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| self.window_body(ui));
        self.open = open;
    }

    fn window_body(&mut self, ui: &mut egui::Ui) {
        ui.label("The WAVE sounds need the wavetables from your own Nord OS update.");
        ui.label("1. Download the official update (Windows .zip/.exe or Mac .dmg):");
        ui.hyperlink_to(
            "Windows: Nord Stage 4 OS update",
            firmware_setting::DOWNLOAD_WINDOWS,
        );
        ui.hyperlink_to(
            "Mac: Nord Stage 4 OS update",
            firmware_setting::DOWNLOAD_MAC,
        );
        ui.label("2. Drop the downloaded file onto this window, or paste its path:");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let input_w = (ui.available_width() - 112.0).max(180.0);
            let field_h = ui.spacing().interact_size.y;
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .desired_width(input_w)
                    .min_size(egui::vec2(input_w, field_h))
                    .margin(egui::Margin::symmetric(8, 5))
                    .vertical_align(egui::Align::Center)
                    .hint_text("Path to Update Nord Stage 4 OS v1.66 (.zip / .exe / .dmg)"),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .button("Use this file")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                || enter
            {
                self.use_file(&self.input.clone());
            }
        });
        if let Some(m) = &self.message {
            ui.label(m);
        }
        ui.separator();
        match &self.status.path {
            None => {
                ui.label(firmware_setting::PLACEHOLDER_NOTE);
            }
            Some((p, src)) => {
                let from = match src {
                    Source::Environment => "from NS4_WAVE_TABLES",
                    Source::Saved => "saved",
                };
                ui.label(format!("Set ({from}): {}", p.display()));
                match &self.status.tables {
                    Some(Ok(n)) => ui.label(format!("{n} WAVE tables loaded.")),
                    Some(Err(e)) => ui.label(format!("Problem: {e}")),
                    None => ui.label(""),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ns4-gui-{name}-{}", std::process::id()))
            .join("config.json")
    }

    #[test]
    fn with_nothing_set_the_placeholder_is_named() {
        let p = OsUpdatePanel::new(Some(config("none")), None);
        assert!(p.summary().contains("placeholder"));
        assert_eq!(p.session, None);
    }

    #[test]
    fn a_bad_file_is_refused_with_a_plain_message_and_not_saved() {
        let c = config("bad");
        let mut p = OsUpdatePanel::new(Some(c.clone()), None);
        let bad = std::env::temp_dir().join(format!("ns4-gui-bad-{}.bin", std::process::id()));
        std::fs::write(&bad, b"junk").unwrap();
        p.use_file(&format!("\"{}\"", bad.display()));
        assert!(p
            .message
            .as_deref()
            .unwrap()
            .contains("not a Nord OS update"));
        assert!(!c.exists());
        let mut none = OsUpdatePanel::new(None, None);
        none.use_file("x");
        assert!(none.message.unwrap().contains("no place to save"));
    }

    #[test]
    fn the_summary_names_each_state() {
        let mut p = OsUpdatePanel::new(None, None);
        let at = Some((PathBuf::from("x"), Source::Saved));
        p.status = Status {
            path: at.clone(),
            tables: Some(Ok(46)),
            config_error: None,
        };
        assert!(p.summary().contains("46 Nord tables"));
        p.status = Status {
            path: at,
            tables: Some(Err("bad".into())),
            config_error: None,
        };
        assert!(p.summary().contains("update file problem"));
        p.status = Status {
            path: None,
            tables: None,
            config_error: Some("bad json".into()),
        };
        assert!(p.summary().contains("saved settings problem"));
    }

    #[test]
    fn the_window_and_bar_draw_without_a_display() {
        let mut p = OsUpdatePanel::new(Some(config("draw")), None);
        p.open = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| p.show_bar(ui));
            p.show_window(ctx);
        });
        assert!(p.open);
    }
}
