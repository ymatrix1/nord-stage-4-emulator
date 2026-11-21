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
    /// `--program A:14`: panel location to load from `--backup` (no file path).
    initial_slot: Option<String>,
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
                if nord_gui::headless_load::parse_slot(&path).is_some() {
                    parsed.initial_slot = Some(path);
                } else {
                    parsed.initial_program = Some(PathBuf::from(path));
                }
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
    headless_load: nord_gui::headless_load::HeadlessLoad,
    firmware: FirmwarePanelState,
    os_update: nord_gui::os_update_panel::OsUpdatePanel,
    debug_dialog: nord_gui::debug_dialog::DebugDialogState,
}

impl Stage4App {
    fn new(cc: &eframe::CreationContext<'_>, cli: CliArgs) -> Self {
        nord_gui::style::setup_nord_visuals(&cc.egui_ctx);

        let active_keys = Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        let audio_handle = start_audio_and_midi(Arc::clone(&active_keys));

        let state = Stage4PanelState {
            use_baked_plate: true,
            ..Default::default()
        };

        let mut app = Self {
            state,
            library: BackupLibraryState::new(),
            audio_handle,
            active_keys,
            screenshot_target: None,
            shot: cli
                .screenshot_target
                .map(|t| ShotPlan::new(t, cli.screenshot_section, cli.screenshot_scale)),
            shot_requested: false,
            frame_count: 0,
            headless_load: nord_gui::headless_load::HeadlessLoad::new(cli.initial_slot.as_deref()),
            firmware: FirmwarePanelState::new(cli.firmware),
            os_update: nord_gui::os_update_panel::OsUpdatePanel::new(
                nord_core::user_config::default_config_path(),
                std::env::var("NS4_WAVE_TABLES").ok().as_deref(),
            ),
            debug_dialog: nord_gui::debug_dialog::DebugDialogState::default(),
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
                // Through `send_command` so the control thread's shadow engine sees it too.
                let _ = self
                    .audio_handle
                    .send_command(nord_gui::audio::AudioCommand::LoadProgram(Box::new(prog)));
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
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.state.use_baked_plate && self.shot.is_none() {
            egui::Rgba::TRANSPARENT.to_array()
        } else {
            egui::Rgba::from(nord_gui::style::CHASSIS_RED).to_array()
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        eprintln!("[Audio] {}", self.audio_handle.meter().snapshot().summary());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        self.library
            .poll_results(&mut self.state, &self.audio_handle);
        if self
            .headless_load
            .drive(&mut self.state, &self.library, &self.audio_handle)
        {
            self.frame_count = 1; // a screenshot waits for the program to be loaded
        }

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
            let before = self.firmware.opts;
            nord_gui::debug_dialog::show_debug_dialog(
                ctx,
                &mut self.debug_dialog,
                &mut self.state,
                &mut self.library,
                &mut self.firmware,
                &mut self.os_update,
                &self.audio_handle,
            );
            if before != self.firmware.opts {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(firmware_panel::window_title(
                    &self.firmware.opts,
                )));
            }
            self.library.handle_keys(ctx);
            self.library
                .render_drawer(ctx, &mut self.state, &self.audio_handle);
        }

        // Transparent floating top bar ABOVE the keyboard, aligned to the keyboard's left/right edges
        if self.shot.is_none() {
            let screen = ctx.screen_rect();
            let plate_rect = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("ns4_baked_plate_rect")))
                .filter(|r| r.is_positive() && r.width() > 100.0)
                .unwrap_or(screen);
            let left_pad = (plate_rect.left() - screen.left()).max(0.0);
            let bar_w = plate_rect.width().min(screen.width() - left_pad).max(200.0);

            egui::TopBottomPanel::top("ns4_floating_bar")
                .exact_height(22.0)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if left_pad > 0.0 {
                            ui.add_space(left_pad);
                        }
                        ui.allocate_ui(egui::vec2(bar_w, 18.0), |ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.spacing_mut().button_padding = egui::vec2(7.0, 1.5);
                            ui.horizontal(|ui| {
                                let hw_pill = |ui: &mut egui::Ui, label: &str| {
                                    let rt = egui::RichText::new(label)
                                        .size(9.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(236, 240, 244));
                                    ui.add(
                                        egui::Button::new(rt)
                                            .fill(egui::Color32::from_rgba_unmultiplied(
                                                20, 22, 26, 200,
                                            ))
                                            .stroke(egui::Stroke::new(
                                                0.8,
                                                egui::Color32::from_rgba_unmultiplied(
                                                    255, 255, 255, 45,
                                                ),
                                            ))
                                            .corner_radius(4.0),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                };
                                if hw_pill(ui, "OPEN .NS4B").clicked() {
                                    self.debug_dialog
                                        .open_tab(nord_gui::debug_dialog::DebugTab::Library);
                                }
                                let dbg_label = if self.debug_dialog.open {
                                    "HIDE DEBUG"
                                } else {
                                    "DEBUG"
                                };
                                if hw_pill(ui, dbg_label).clicked() {
                                    if self.debug_dialog.open {
                                        self.debug_dialog.open = false;
                                    } else {
                                        let tab = self.debug_dialog.tab;
                                        self.debug_dialog.open_tab(tab);
                                    }
                                }
                                if hw_pill(ui, "? HELP").clicked() {
                                    self.debug_dialog
                                        .open_tab(nord_gui::debug_dialog::DebugTab::Guide);
                                }

                                // Draggable empty space between left pills and right window icons
                                let drag_w = (ui.available_width() - 52.0).max(12.0);
                                let (_, drag_resp) = ui.allocate_exact_size(
                                    egui::vec2(drag_w, 16.0),
                                    egui::Sense::click_and_drag(),
                                );
                                if drag_resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                                }
                                if drag_resp.is_pointer_button_down_on() && drag_resp.dragged() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                }

                                // Vector Minimize button
                                let (min_rect, min_resp) = ui.allocate_exact_size(
                                    egui::vec2(22.0, 16.0),
                                    egui::Sense::click(),
                                );
                                let min_resp =
                                    min_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                let min_bg = if min_resp.hovered() {
                                    egui::Color32::from_rgba_unmultiplied(46, 50, 58, 230)
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(20, 22, 26, 200)
                                };
                                ui.painter().rect_filled(min_rect, 4.0, min_bg);
                                ui.painter().rect_stroke(
                                    min_rect,
                                    4.0,
                                    egui::Stroke::new(
                                        0.8,
                                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 45),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(
                                            min_rect.center().x - 4.5,
                                            min_rect.center().y + 1.5,
                                        ),
                                        egui::pos2(
                                            min_rect.center().x + 4.5,
                                            min_rect.center().y + 1.5,
                                        ),
                                    ],
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(236, 240, 244)),
                                );
                                if min_resp.clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                                }

                                // Vector Close button
                                let (cls_rect, cls_resp) = ui.allocate_exact_size(
                                    egui::vec2(22.0, 16.0),
                                    egui::Sense::click(),
                                );
                                let cls_resp =
                                    cls_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                let cls_bg = if cls_resp.hovered() {
                                    egui::Color32::from_rgb(184, 37, 42)
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(20, 22, 26, 200)
                                };
                                ui.painter().rect_filled(cls_rect, 4.0, cls_bg);
                                ui.painter().rect_stroke(
                                    cls_rect,
                                    4.0,
                                    egui::Stroke::new(
                                        0.8,
                                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 45),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                let c = cls_rect.center();
                                let d = 3.8;
                                let stroke =
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(236, 240, 244));
                                ui.painter().line_segment(
                                    [c + egui::vec2(-d, -d), c + egui::vec2(d, d)],
                                    stroke,
                                );
                                ui.painter().line_segment(
                                    [c + egui::vec2(d, -d), c + egui::vec2(-d, d)],
                                    stroke,
                                );
                                if cls_resp.clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            });
                        });
                    });
                });
        }

        self.library.sync_piano_ui(&mut self.state);
        let bg_fill = if self.state.use_baked_plate {
            egui::Color32::TRANSPARENT
        } else {
            nord_gui::style::CHASSIS_RED
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg_fill))
            .show(ctx, |ui| {
                render_stage4_panel(ui, &mut self.state, &self.audio_handle);
            });

        // Borderless window resize handles (right edge, bottom edge, bottom-right corner)
        if self.shot.is_none() {
            let screen = ctx.screen_rect();

            // Borderless window resize handles (right edge, bottom edge, bottom-right corner)
            let edge = 8.0;
            let corner_rect = egui::Rect::from_min_max(
                egui::pos2(screen.right() - 14.0, screen.bottom() - 14.0),
                screen.right_bottom(),
            );
            let right_rect = egui::Rect::from_min_max(
                egui::pos2(screen.right() - edge, screen.top() + 18.0),
                egui::pos2(screen.right(), screen.bottom() - 14.0),
            );
            let bottom_rect = egui::Rect::from_min_max(
                egui::pos2(screen.left() + 14.0, screen.bottom() - edge),
                egui::pos2(screen.right() - 14.0, screen.bottom()),
            );
            let handle_resize =
                |r: egui::Rect, icon: egui::CursorIcon, dir: egui::ResizeDirection| {
                    let (hovered, pressed) = ctx.input(|i| {
                        let h = i.pointer.hover_pos().is_some_and(|p| r.contains(p));
                        let p = h && i.pointer.primary_pressed();
                        (h, p)
                    });
                    if hovered {
                        ctx.set_cursor_icon(icon);
                    }
                    if pressed {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                    }
                };
            handle_resize(
                corner_rect,
                egui::CursorIcon::ResizeNwSe,
                egui::ResizeDirection::SouthEast,
            );
            handle_resize(
                right_rect,
                egui::CursorIcon::ResizeHorizontal,
                egui::ResizeDirection::East,
            );
            handle_resize(
                bottom_rect,
                egui::CursorIcon::ResizeVertical,
                egui::ResizeDirection::South,
            );
        }

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

fn build_app_icon() -> egui::IconData {
    let size = 64usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            // Rounded square mask (radius = 12 px)
            let dx = if x < 12 {
                12.0 - x as f32
            } else if x >= size - 12 {
                x as f32 - (size - 13) as f32
            } else {
                0.0
            };
            let dy = if y < 12 {
                12.0 - y as f32
            } else if y >= size - 12 {
                y as f32 - (size - 13) as f32
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 12.2 {
                continue;
            }
            let alpha = ((12.2 - dist).clamp(0.0, 1.0) * 255.0) as u8;
            // Crimson chassis gradient
            let t = y as f32 / size as f32;
            let mut r = (182.0 - t * 42.0) as u8;
            let mut g = (30.0 - t * 10.0) as u8;
            let mut b = (36.0 - t * 10.0) as u8;

            // Top slate control strip (y 14..32, x 10..54)
            if (14..32).contains(&y) && (10..54).contains(&x) {
                r = 48;
                g = 58;
                b = 68;
                // Glowing red LED dots at y 19..22
                if (19..23).contains(&y) && ((15..19).contains(&x) || (30..34).contains(&x)) {
                    r = 255;
                    g = 52;
                    b = 52;
                }
                // Glowing green LED dot at x 45..49
                if (19..23).contains(&y) && (45..49).contains(&x) {
                    r = 92;
                    g = 230;
                    b = 46;
                }
            }
            // Piano keybed along bottom (y 36..56, x 8..56)
            if (36..56).contains(&y) && (8..56).contains(&x) {
                let kx = x - 8;
                let key_slot = kx % 8;
                let is_black =
                    y < 48 && (5..8).contains(&key_slot) && (kx / 8) != 2 && (kx / 8) != 5;
                if is_black {
                    r = 20;
                    g = 22;
                    b = 26;
                } else if key_slot == 0 {
                    r = 165;
                    g = 162;
                    b = 155;
                } else {
                    r = 246;
                    g = 244;
                    b = 238;
                }
            }
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = alpha;
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}

fn main() -> eframe::Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    // the saved Nord OS update file fills NS4_WAVE_TABLES (env and flags still win), before any thread
    nord_core::user_config::apply_saved_default();
    let firmware = match nord_core::render_opts::FirmwareOptions::from_args(&argv) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("nord-stage4: {e}");
            std::process::exit(2);
        }
    };
    if nord_gui::audio::probe::requested(&argv).is_some() {
        match nord_gui::audio::probe::run(&argv) {
            Ok(report) => println!("{report}"),
            Err(e) => {
                eprintln!("nord-stage4 --audio-probe: {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
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
        .with_decorations(false)
        .with_transparent(true)
        .with_icon(build_app_icon())
        .with_inner_size([1760.0, 436.0])
        .with_min_inner_size([960.0, 250.0])
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
    fn test_app_icon_is_valid_64x64_rgba() {
        let icon = build_app_icon();
        assert_eq!(icon.width, 64);
        assert_eq!(icon.height, 64);
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
        // Center pixel is opaque crimson/slate, corner pixel is transparent.
        assert_eq!(icon.rgba[3], 0);
        assert_eq!(icon.rgba[(32 * 64 + 32) * 4 + 3], 255);
    }

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

        let slot = parse_cli_args(["nord-stage4", "--program", "A:14"].map(String::from));
        assert_eq!(slot.initial_slot.as_deref(), Some("A:14"));
        assert_eq!(slot.initial_program, None, "a location is not a file");

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
