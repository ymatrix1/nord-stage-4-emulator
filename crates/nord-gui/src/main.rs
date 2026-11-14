//! Nord Stage 4 Standalone Application Entry Point.
//!
//! Launches the interactive 1:1 hardware control surface and spawns the
//! real-time CPAL audio host thread and MIDIR input listener.

use eframe::egui;
use nord_gui::audio::start_audio_and_midi;
use nord_gui::firmware_panel::{self, FirmwarePanelState, HeadlessRequest};
use nord_gui::library::BackupLibraryState;
use nord_gui::panel::{render_stage4_panel, Stage4PanelState};
use nord_gui::screenshot::{self, ShotPlan};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Default, PartialEq)]
struct CliArgs {
    screenshot_target: Option<PathBuf>,
    screenshot_scale: Option<f32>,
    screenshot_section: Option<nord_panel::Section>,
    initial_program: Option<PathBuf>,
    initial_backup: Option<PathBuf>,
    initial_piano: Option<PathBuf>,
    initial_sample: Option<PathBuf>,
    firmware: nord_core::render_opts::FirmwareOptions,
}

fn parse_cli_args<I: IntoIterator<Item = String>>(args: I) -> CliArgs {
    let mut parsed = CliArgs::default();
    let args: Vec<String> = args.into_iter().collect();
    // Invalid firmware flag values are reported by `main` before the window opens.
    parsed.firmware = nord_core::render_opts::FirmwareOptions::from_args(&args).unwrap_or_default();
    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--screenshot" {
            if let Some(path) = iter.next() {
                parsed.screenshot_target = Some(PathBuf::from(path));
            }
        } else if arg == "--screenshot-scale" {
            parsed.screenshot_scale = iter.next().and_then(|v| v.parse().ok());
        } else if arg == "--screenshot-section" {
            parsed.screenshot_section = iter.next().and_then(|v| screenshot::parse_section(&v));
        } else if arg == "--program" || arg == "-p" {
            if let Some(path) = iter.next() {
                parsed.initial_program = Some(PathBuf::from(path));
            }
        } else if arg == "--backup" || arg == "-b" {
            if let Some(path) = iter.next() {
                parsed.initial_backup = Some(PathBuf::from(path));
            }
        } else if arg == "--piano" {
            if let Some(path) = iter.next() {
                parsed.initial_piano = Some(PathBuf::from(path));
            }
        } else if arg == "--sample" {
            if let Some(path) = iter.next() {
                parsed.initial_sample = Some(PathBuf::from(path));
            }
        } else if !arg.starts_with('-') {
            let lower = arg.to_ascii_lowercase();
            if lower.ends_with(".ns4p") {
                parsed.initial_program = Some(PathBuf::from(arg));
            } else if lower.ends_with(".ns4b") {
                parsed.initial_backup = Some(PathBuf::from(arg));
            } else if lower.ends_with(".npno") {
                parsed.initial_piano = Some(PathBuf::from(arg));
            } else if lower.ends_with(".nsmp4") {
                parsed.initial_sample = Some(PathBuf::from(arg));
            }
        }
    }
    parsed
}

/// Hard limit for `--screenshot` runs; the process exits non-zero when the PNG never arrives.
const SCREENSHOT_TIMEOUT_S: u64 = 20;

struct Stage4App {
    state: Stage4PanelState,
    library: BackupLibraryState,
    audio_handle: nord_gui::audio::AudioHandle,
    active_keys: Arc<[AtomicBool; 128]>,
    screenshot_target: Option<PathBuf>,
    shot: Option<ShotPlan>,
    shot_requested: bool,
    frame_count: u32,
    firmware: FirmwarePanelState,
}

impl Stage4App {
    fn new(cc: &eframe::CreationContext<'_>, cli: CliArgs) -> Self {
        nord_gui::style::setup_nord_visuals(&cc.egui_ctx);

        let active_keys = Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        let audio_handle = start_audio_and_midi(Arc::clone(&active_keys));

        let mut app = Self {
            state: Stage4PanelState::default(),
            library: BackupLibraryState::new(),
            audio_handle,
            active_keys,
            screenshot_target: None,
            shot: cli
                .screenshot_target
                .map(|t| ShotPlan::new(t, cli.screenshot_section, cli.screenshot_scale)),
            shot_requested: false,
            frame_count: 0,
            firmware: FirmwarePanelState::new(cli.firmware),
        };
        app.firmware.push(&app.audio_handle);
        if let Some(plan) = &app.shot {
            app.state.view = plan.view();
        }

        if let Some(path) = cli.initial_program {
            if let Ok(bytes) = std::fs::read(&path) {
                let stem = path.file_stem().and_then(|s| s.to_str());
                app.load_program_from_bytes(&bytes, stem);
            } else {
                eprintln!("[GUI] Could not read program file: {}", path.display());
            }
        }
        if let Some(path) = cli.initial_backup {
            match app.library.open_backup_path(&path) {
                Ok(()) => app.library.drawer_open = true,
                Err(e) => {
                    eprintln!("[GUI] Could not open backup '{}': {e}", path.display());
                    app.library.error_message = Some(e);
                }
            }
        }
        if let Some(path) = cli.initial_piano {
            app.library.request_piano_file(path, false);
        }
        if let Some(path) = cli.initial_sample {
            app.library.request_sample_file(path, 0);
        }

        app
    }

    fn load_program_from_bytes(&mut self, bytes: &[u8], title_override: Option<&str>) -> bool {
        let decoded = nord_core::cbin::CbinFile::parse(bytes)
            .and_then(|cbin| nord_core::cbin::ProgramDecoded::from_cbin_file(&cbin));
        match decoded {
            Ok(mut prog) => {
                if let Some(title) = title_override {
                    if !title.is_empty() {
                        prog.name = title.to_string();
                    }
                }
                println!(
                    "[GUI] Loaded program: {} (category=0x{:02X})",
                    prog.name, prog.category_id
                );
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
        self.library
            .poll_results(&mut self.state, &self.audio_handle);

        // Handle drag and drop of .ns4p, .ns4b, .npno, and .nsmp4 files
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = &file.path {
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase());
                match ext.as_deref() {
                    Some("ns4b") => {
                        if let Err(err) = self.library.open_backup_path(path) {
                            self.library.error_message = Some(err);
                        } else {
                            self.library.drawer_open = true;
                        }
                    }
                    Some("npno") => {
                        self.library
                            .request_piano_file(path.clone(), self.library.piano_slot_b);
                    }
                    Some("nsmp4") => {
                        self.library
                            .request_sample_file(path.clone(), self.library.synth_layer_idx);
                    }
                    Some("ns4p") => {
                        if let Ok(bytes) = std::fs::read(path) {
                            let stem = path.file_stem().and_then(|s| s.to_str());
                            self.load_program_from_bytes(&bytes, stem);
                        }
                    }
                    _ => {}
                }
            } else if let Some(bytes) = &file.bytes {
                self.library.load_dropped_bytes(&file.name, bytes);
            }
        }

        // Sync active keys from MIDI input into UI state
        for (i, pressed) in self.state.midi_active_keys.iter_mut().enumerate() {
            *pressed = self.active_keys[i].load(Ordering::Relaxed);
        }

        if self.shot.is_none() {
            egui::TopBottomPanel::top("ns4_library_bar")
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(36, 12, 14))
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ctx, |ui| {
                    self.library
                        .render_top_bar(ui, &mut self.state.sync_knobs_on_program_load);
                });

            let before = self.firmware.opts;
            egui::TopBottomPanel::top("ns4_firmware_bar").show(ctx, |ui| {
                self.firmware.show_bar(ui, &self.audio_handle);
            });
            if before != self.firmware.opts {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(firmware_panel::window_title(
                    &self.firmware.opts,
                )));
            }

            self.library
                .render_drawer(ctx, &mut self.state, &self.audio_handle);
        }

        self.library.sync_piano_ui(&mut self.state);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(nord_gui::style::CHASSIS_RED))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_stage4_panel(ui, &mut self.state, &self.audio_handle);
                    });
            });

        // Screenshot capture: F12 (whole window) or the CLI plan (section crop at a fixed zoom).
        if let Some(plan) = &self.shot {
            // InnerSize is in points at egui's current scale (native x zoom).
            let ppp = ctx.pixels_per_point();
            let phys = ctx.input(|i| i.screen_rect().size()) * ctx.pixels_per_point();
            if !self.shot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    screenshot::inner_size_command(plan, ppp),
                ));
                if plan.ready(self.frame_count, phys) {
                    self.shot_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                }
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F12)) && self.screenshot_target.is_none() {
            self.screenshot_target = Some(PathBuf::from("screenshot.png"));
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }

        let events = ctx.input(|i| i.raw.events.clone());
        for event in &events {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(plan) = self.shot.take() {
                    let ok = match plan.crop(image) {
                        Some(rgba) => rgba.save(&plan.target).map_err(|e| e.to_string()),
                        None => Err("empty crop".to_string()),
                    };
                    match ok {
                        Ok(()) => {
                            println!("Screenshot saved to {}", plan.target.display());
                            // Clean exit 0 (the normal close path ends with 0xcfffffff on Windows).
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("Failed to save screenshot: {e}");
                            std::process::exit(1);
                        }
                    }
                } else if let Some(target) = self.screenshot_target.take() {
                    let [w, h] = image.size;
                    let pixels: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
                        .collect();
                    if let Some(rgba) = image::RgbaImage::from_raw(w as u32, h as u32, pixels) {
                        match rgba.save(&target) {
                            Ok(()) => println!("Screenshot saved to {}", target.display()),
                            Err(e) => eprintln!("Failed to save screenshot: {e}"),
                        }
                    }
                }
            }
        }

        // Request repaint if keys are pressed or audio is playing
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn main() -> eframe::Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    let firmware = match nord_core::render_opts::FirmwareOptions::from_args(&argv) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("nord-stage4: {e}");
            std::process::exit(2);
        }
    };
    let headless = HeadlessRequest::from_args(&argv);
    if headless.is_headless() {
        if headless.dump_options {
            println!("firmware options: {}", firmware.summary());
        }
        if let Some(wav) = &headless.self_test_wav {
            match firmware_panel::run_self_test(&firmware, Some(wav)) {
                Ok(report) => println!(
                    "{report}
wrote {}",
                    wav.display()
                ),
                Err(e) => {
                    eprintln!("nord-stage4 self-test failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        return Ok(());
    }
    let cli = parse_cli_args(argv);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1900.0, 680.0])
        .with_min_inner_size([960.0, 360.0])
        .with_title(firmware_panel::window_title(&cli.firmware));
    if cli.screenshot_target.is_some() {
        // Non-intrusive: off-screen, no focus, no taskbar entry, and a hard timeout.
        viewport = viewport
            .with_position([-32000.0, -32000.0])
            .with_active(false)
            .with_taskbar(false)
            .with_min_inner_size([16.0, 16.0]);
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(SCREENSHOT_TIMEOUT_S));
            eprintln!("nord-stage4: screenshot timed out after {SCREENSHOT_TIMEOUT_S} s");
            std::process::exit(3);
        });
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Nord Stage 4",
        native_options,
        Box::new(move |cc| Ok(Box::new(Stage4App::new(cc, cli)))),
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
        let parsed = parse_cli_args([
            "nord-stage4".to_string(),
            "--program".to_string(),
            "bank_a/patch.ns4p".to_string(),
            "--backup".to_string(),
            "restore.ns4b".to_string(),
            "--piano".to_string(),
            "grand.npno".to_string(),
            "--sample".to_string(),
            "strings.nsmp4".to_string(),
        ]);
        assert_eq!(
            parsed.initial_program,
            Some(PathBuf::from("bank_a/patch.ns4p"))
        );
        assert_eq!(parsed.initial_backup, Some(PathBuf::from("restore.ns4b")));
        assert_eq!(parsed.initial_piano, Some(PathBuf::from("grand.npno")));
        assert_eq!(parsed.initial_sample, Some(PathBuf::from("strings.nsmp4")));

        let positional = parse_cli_args(["nord-stage4".to_string(), "factory.ns4b".to_string()]);
        assert_eq!(
            positional.initial_backup,
            Some(PathBuf::from("factory.ns4b"))
        );
    }

    #[test]
    fn test_cli_screenshot_flags() {
        let parsed = parse_cli_args(
            [
                "nord-stage4",
                "--screenshot",
                "o.png",
                "--screenshot-scale",
                "2",
                "--screenshot-section",
                "synth",
            ]
            .map(String::from),
        );
        assert_eq!(parsed.screenshot_target, Some(PathBuf::from("o.png")));
        assert_eq!(parsed.screenshot_scale, Some(2.0));
        assert_eq!(parsed.screenshot_section, Some(nord_panel::Section::Synth));
        let bad = parse_cli_args(["x", "--screenshot-section", "nope"].map(String::from));
        assert_eq!(bad.screenshot_section, None);
    }
}
