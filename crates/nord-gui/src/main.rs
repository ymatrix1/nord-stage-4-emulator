//! Nord Stage 4 Standalone Application Entry Point.
//!
//! Launches the interactive 1:1 hardware control surface and spawns the
//! real-time CPAL audio host thread and MIDIR input listener.

use eframe::egui;
use nord_gui::audio::start_audio_and_midi;
use nord_gui::panel::{render_stage4_panel, Stage4PanelState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Stage4App {
    state: Stage4PanelState,
    audio_handle: nord_gui::audio::AudioHandle,
    active_keys: Arc<[AtomicBool; 128]>,
    screenshot_target: Option<PathBuf>,
    frame_count: u32,
}

impl Stage4App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        screenshot_target: Option<PathBuf>,
        initial_program: Option<PathBuf>,
    ) -> Self {
        nord_gui::style::setup_nord_visuals(&cc.egui_ctx);

        let active_keys = Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        let audio_handle = start_audio_and_midi(Arc::clone(&active_keys));

        let mut app = Self {
            state: Stage4PanelState::default(),
            audio_handle,
            active_keys,
            screenshot_target,
            frame_count: 0,
        };

        if let Some(path) = initial_program {
            if let Ok(bytes) = std::fs::read(&path) {
                app.load_program_from_bytes(&bytes);
            } else {
                eprintln!("[GUI] Could not read program file: {}", path.display());
            }
        }

        app
    }

    fn load_program_from_bytes(&mut self, bytes: &[u8]) -> bool {
        let decoded = nord_core::cbin::CbinFile::parse(bytes)
            .and_then(|cbin| nord_core::cbin::ProgramDecoded::from_cbin_file(&cbin));
        match decoded {
            Ok(prog) => {
                println!("[GUI] Loaded program: {}", prog.name);
                self.state.apply_program(&prog);
                let _ = self
                    .audio_handle
                    .cmd_sender
                    .send(nord_gui::audio::AudioCommand::LoadProgram(Box::new(prog)));
                true
            }
            Err(e) => {
                eprintln!("[GUI] Failed to parse .ns4p program: {e:?}");
                false
            }
        }
    }
}

impl eframe::App for Stage4App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;

        // Handle drag and drop of .ns4p patch files
        ctx.input(|i| {
            for dropped in &i.raw.dropped_files {
                if let Some(path) = &dropped.path {
                    if path.extension().and_then(|s| s.to_str()) == Some("ns4p") {
                        if let Ok(bytes) = std::fs::read(path) {
                            self.load_program_from_bytes(&bytes);
                        }
                    }
                } else if let Some(bytes) = &dropped.bytes {
                    self.load_program_from_bytes(bytes);
                }
            }
        });

        // Sync active keys from MIDI input into UI state
        for (i, pressed) in self.state.midi_active_keys.iter_mut().enumerate() {
            *pressed = self.active_keys[i].load(Ordering::Relaxed);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(nord_gui::style::CHASSIS_RED))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_stage4_panel(ui, &mut self.state, &self.audio_handle);
                    });
            });

        // Screenshot capture support (CLI flag or F12 key)
        let f12_pressed = ctx.input(|i| i.key_pressed(egui::Key::F12));
        if f12_pressed && self.screenshot_target.is_none() {
            self.screenshot_target = Some(PathBuf::from("screenshot.png"));
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        } else if self.screenshot_target.is_some() && self.frame_count == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }

        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    if let Some(target) = self.screenshot_target.take() {
                        let [w, h] = image.size;
                        let pixels: Vec<u8> = image
                            .pixels
                            .iter()
                            .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
                            .collect();
                        if let Some(rgba) = image::RgbaImage::from_raw(w as u32, h as u32, pixels) {
                            if let Err(e) = rgba.save(&target) {
                                eprintln!("Failed to save screenshot: {e}");
                            } else {
                                println!("Screenshot saved to {}", target.display());
                            }
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        });

        // Request repaint if keys are pressed or audio is playing
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn main() -> eframe::Result<()> {
    let mut screenshot_target = None;
    let mut initial_program = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--screenshot" {
            if let Some(path) = args.next() {
                screenshot_target = Some(PathBuf::from(path));
            }
        } else if arg == "--program" || arg == "-p" {
            if let Some(path) = args.next() {
                initial_program = Some(PathBuf::from(path));
            }
        } else if !arg.starts_with('-') && arg.ends_with(".ns4p") {
            initial_program = Some(PathBuf::from(arg));
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1580.0, 440.0])
            .with_min_inner_size([960.0, 360.0])
            .with_title("Clavia Nord Stage 4"),
        ..Default::default()
    };

    eframe::run_native(
        "Nord Stage 4",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(Stage4App::new(
                cc,
                screenshot_target,
                initial_program,
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screenshot_command_exists() {
        let _ = egui::ViewportCommand::Screenshot(Default::default());
        let event = egui::Event::Screenshot {
            viewport_id: egui::ViewportId::ROOT,
            user_data: Default::default(),
            image: std::sync::Arc::new(egui::ColorImage::example()),
        };
        if let egui::Event::Screenshot { image, .. } = event {
            assert_eq!(image.width(), 128);
        }
    }

    #[test]
    fn test_active_keys_atomic_init() {
        let keys: Arc<[AtomicBool; 128]> =
            Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        assert!(!keys[60].load(Ordering::Relaxed));
        keys[60].store(true, Ordering::Relaxed);
        assert!(keys[60].load(Ordering::Relaxed));
    }

    #[test]
    fn test_cli_program_argument_detection() {
        let args = vec![
            "nord-stage4".to_string(),
            "--program".to_string(),
            "bank_a/patch.ns4p".to_string(),
        ];
        let mut initial_program = None;
        let mut iter = args.into_iter().skip(1);
        while let Some(arg) = iter.next() {
            if arg == "--program" || arg == "-p" {
                if let Some(path) = iter.next() {
                    initial_program = Some(PathBuf::from(path));
                }
            } else if !arg.starts_with('-') && arg.ends_with(".ns4p") {
                initial_program = Some(PathBuf::from(arg));
            }
        }
        assert_eq!(initial_program, Some(PathBuf::from("bank_a/patch.ns4p")));
    }
}
