//! Backup Archive (`.ns4b`) and CNSP Sample (`.npno` / `.nsmp4`) Browser & Async Loader.
//!
//! Decodes multi-megabyte `.npno` and `.nsmp4` files on a background worker thread and keeps
//! retained `Arc` references on the UI thread so the real-time CPAL audio callback never drops
//! heap allocations when hot-swapping instruments.

use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{Color32, RichText};
use nord_core::cbin::{CbinFile, Ns4BackupArchive, ProgramDecoded};
use nord_core::piano::NpnoSound;
use nord_core::synth::{SynthOscMode, SynthSampleSound};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Active category tab inside the `.ns4b` library drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupTab {
    Programs,
    Pianos,
    Samples,
}

#[derive(Debug)]
enum LibraryLoadOutcome {
    Program {
        title: String,
        program: Box<ProgramDecoded>,
    },
    Piano {
        key: String,
        slot_b: bool,
        sound: Arc<NpnoSound>,
        from_program: bool,
    },
    SynthSample {
        key: String,
        layer_idx: usize,
        sound: Arc<SynthSampleSound>,
        from_program: bool,
    },
    Error(String),
}

/// Manages `.ns4b` archive browsing, cached `.npno`/`.nsmp4` decoding, and RT-safe `Arc` lifetimes.
pub struct BackupLibraryState {
    pub drawer_open: bool,
    pub auto_close_on_load: bool,
    pub path_input: String,
    pub backup_path: Option<PathBuf>,
    pub archive: Option<Arc<Ns4BackupArchive>>,
    pub manifest_summary: Option<String>,
    pub programs: Vec<String>,
    pub pianos: Vec<String>,
    pub samples: Vec<String>,
    pub filter: String,
    pub active_tab: BackupTab,
    pub piano_slot_b: bool,
    pub synth_layer_idx: usize,
    pub pending_jobs: usize,
    pub status_message: String,
    pub error_message: Option<String>,
    piano_cache: HashMap<String, Arc<NpnoSound>>,
    sample_cache: HashMap<String, Arc<SynthSampleSound>>,
    retired_pianos: Vec<Arc<NpnoSound>>,
    retired_samples: Vec<Arc<SynthSampleSound>>,
    result_tx: Sender<LibraryLoadOutcome>,
    result_rx: Receiver<LibraryLoadOutcome>,
}

impl Default for BackupLibraryState {
    fn default() -> Self {
        Self::new()
    }
}

impl BackupLibraryState {
    pub fn new() -> Self {
        let (result_tx, result_rx) = unbounded();
        let default_path = Self::discover_default_backup()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Self {
            drawer_open: false,
            auto_close_on_load: true,
            path_input: default_path,
            backup_path: None,
            archive: None,
            manifest_summary: None,
            programs: Vec::new(),
            pianos: Vec::new(),
            samples: Vec::new(),
            filter: String::new(),
            active_tab: BackupTab::Programs,
            piano_slot_b: false,
            synth_layer_idx: 0,
            pending_jobs: 0,
            status_message: "No .ns4b backup loaded".to_string(),
            error_message: None,
            piano_cache: HashMap::new(),
            sample_cache: HashMap::new(),
            retired_pianos: Vec::new(),
            retired_samples: Vec::new(),
            result_tx,
            result_rx,
        }
    }

    /// Looks for a local `.ns4b` archive path to pre-populate the path input without reading it yet.
    pub fn discover_default_backup() -> Option<PathBuf> {
        let factory_ref = Path::new(".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b");
        if factory_ref.is_file() {
            return Some(factory_ref.to_path_buf());
        }
        if let Ok(entries) = std::fs::read_dir(".") {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("ns4b"))
                {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Indexes an `.ns4b` PKZIP archive and refreshes the sorted `.ns4p` / `.npno` / `.nsmp4` catalogs.
    pub fn open_backup_path(&mut self, path: &Path) -> Result<(), String> {
        let archive = Ns4BackupArchive::open(path).map_err(|e| format!("{e}"))?;
        let mut reader = BufReader::new(
            File::open(path).map_err(|e| format!("failed to reopen '{}': {e}", path.display()))?,
        );
        let manifest_summary = archive
            .read_manifest(&mut reader)
            .ok()
            .map(|m| format!("v{} build {}", m.product_version, m.product_build));

        let mut programs: Vec<String> = archive
            .entries_with_ext(".ns4p")
            .into_iter()
            .map(|e| e.name.clone())
            .collect();
        programs.sort();

        let mut pianos: Vec<String> = archive
            .entries_with_ext(".npno")
            .into_iter()
            .map(|e| e.name.clone())
            .collect();
        pianos.sort();

        let mut samples: Vec<String> = archive
            .entries_with_ext(".nsmp4")
            .into_iter()
            .map(|e| e.name.clone())
            .collect();
        samples.sort();

        // Retire any cached sounds from a previous archive cleanly on the UI thread later
        for (_, arc) in self.piano_cache.drain() {
            self.retired_pianos.push(arc);
        }
        for (_, arc) in self.sample_cache.drain() {
            self.retired_samples.push(arc);
        }

        let file_label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("backup.ns4b");
        self.status_message = format!(
            "Loaded {file_label}: {} .ns4p, {} .npno, {} .nsmp4",
            programs.len(),
            pianos.len(),
            samples.len()
        );
        self.error_message = None;
        self.path_input = path.display().to_string();
        self.backup_path = Some(path.to_path_buf());
        self.archive = Some(Arc::new(archive));
        self.manifest_summary = manifest_summary;
        self.programs = programs;
        self.pianos = pianos;
        self.samples = samples;
        self.reap_retired_sounds();
        Ok(())
    }

    /// Reads and applies a `.ns4p` program entry from the currently opened `.ns4b` archive,
    /// automatically resolving and scheduling matching `.npno` and `.nsmp4` banks.
    pub fn load_program_entry(
        &mut self,
        entry_name: &str,
        panel: &mut Stage4PanelState,
        audio: &AudioHandle,
    ) {
        let (Some(path), Some(archive)) = (&self.backup_path, &self.archive) else {
            self.error_message = Some("Open an .ns4b archive first".to_string());
            return;
        };
        let res = File::open(path)
            .map_err(|e| format!("{e}"))
            .and_then(|f| {
                let mut r = BufReader::new(f);
                archive
                    .read_entry(&mut r, entry_name)
                    .map_err(|e| format!("{e}"))
            })
            .and_then(|bytes| {
                CbinFile::parse(&bytes)
                    .and_then(|c| c.decode_program())
                    .map_err(|e| format!("{e}"))
            });

        match res {
            Ok(mut prog) => {
                let refs = archive.resolve_program_sounds(&prog);
                let active_mask = prog.global.scene_1_mask | prog.global.scene_2_mask;
                let piano_jobs = [
                    (false, prog.piano.layer_a.enabled, 2u8, refs.piano_a),
                    (true, prog.piano.layer_b.enabled, 3u8, refs.piano_b),
                ];
                let synth_jobs = [
                    (0usize, prog.synth.layer_a.enabled, 4u8, refs.synth_a),
                    (1usize, prog.synth.layer_b.enabled, 5u8, refs.synth_b),
                    (2usize, prog.synth.layer_c.enabled, 6u8, refs.synth_c),
                ];

                let stem = Path::new(entry_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Program");
                let is_generic = prog.name.is_empty()
                    || prog.name == "Nord Stage 4 Program"
                    || prog.name.ends_with(" Program");
                if is_generic {
                    prog.name = stem.to_string();
                }
                panel.apply_program(&prog);
                audio.send_command(AudioCommand::LoadProgram(Box::new(prog)));

                let mut resolved_count = 0usize;
                for (slot_b, enabled, bit, entry_opt) in piano_jobs {
                    let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
                    if needed {
                        if let Some(entry) = entry_opt {
                            resolved_count += 1;
                            self.schedule_piano_entry(&entry, slot_b, true);
                        }
                    }
                }
                for (layer_idx, enabled, bit, entry_opt) in synth_jobs {
                    let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
                    if needed {
                        if let Some(entry) = entry_opt {
                            resolved_count += 1;
                            self.schedule_sample_entry(&entry, layer_idx, true);
                        }
                    }
                }

                self.error_message = None;
                self.status_message = if resolved_count > 0 {
                    format!("Program: {stem} (+{resolved_count} resolved sample banks)")
                } else {
                    format!("Program: {stem}")
                };
                if self.auto_close_on_load {
                    self.drawer_open = false;
                }
            }
            Err(err) => {
                self.error_message = Some(format!("Failed to load '{entry_name}': {err}"));
            }
        }
    }

    /// Schedules decoding of a `.npno` entry from the opened `.ns4b` archive (or serves from cache).
    pub fn request_piano_entry(&mut self, entry_name: &str) {
        self.schedule_piano_entry(entry_name, self.piano_slot_b, false);
        if self.auto_close_on_load {
            self.drawer_open = false;
        }
    }

    fn schedule_piano_entry(&mut self, entry_name: &str, slot_b: bool, from_program: bool) {
        if let Some(cached) = self.piano_cache.get(entry_name) {
            let _ = self.result_tx.send(LibraryLoadOutcome::Piano {
                key: entry_name.to_string(),
                slot_b,
                sound: Arc::clone(cached),
                from_program,
            });
            return;
        }
        let (Some(path), Some(archive)) = (self.backup_path.clone(), self.archive.clone()) else {
            self.error_message = Some("Open an .ns4b archive first".to_string());
            return;
        };
        let entry = entry_name.to_string();
        let tx = self.result_tx.clone();
        self.pending_jobs += 1;
        self.status_message = format!("Decoding .npno: {}...", short_name(&entry));
        self.error_message = None;

        std::thread::spawn(move || {
            let outcome = File::open(&path)
                .map_err(|e| format!("open backup failed: {e}"))
                .and_then(|f| {
                    let mut r = BufReader::new(f);
                    archive
                        .read_entry(&mut r, &entry)
                        .map_err(|e| format!("read '{entry}' failed: {e}"))
                })
                .and_then(|bytes| NpnoSound::from_npno_bytes(&bytes));

            let msg = match outcome {
                Ok(mut sound) => {
                    if sound.name.is_empty() {
                        sound.name = short_name(&entry).to_string();
                    }
                    LibraryLoadOutcome::Piano {
                        key: entry,
                        slot_b,
                        sound: Arc::new(sound),
                        from_program,
                    }
                }
                Err(e) => LibraryLoadOutcome::Error(format!("NPNO decode error: {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    /// Schedules decoding of a `.nsmp4` entry from the opened `.ns4b` archive (or serves from cache).
    pub fn request_sample_entry(&mut self, entry_name: &str) {
        self.schedule_sample_entry(entry_name, self.synth_layer_idx.min(2), false);
        if self.auto_close_on_load {
            self.drawer_open = false;
        }
    }

    fn schedule_sample_entry(&mut self, entry_name: &str, layer_idx: usize, from_program: bool) {
        let layer_idx = layer_idx.min(2);
        if let Some(cached) = self.sample_cache.get(entry_name) {
            let _ = self.result_tx.send(LibraryLoadOutcome::SynthSample {
                key: entry_name.to_string(),
                layer_idx,
                sound: Arc::clone(cached),
                from_program,
            });
            return;
        }
        let (Some(path), Some(archive)) = (self.backup_path.clone(), self.archive.clone()) else {
            self.error_message = Some("Open an .ns4b archive first".to_string());
            return;
        };
        let entry = entry_name.to_string();
        let tx = self.result_tx.clone();
        self.pending_jobs += 1;
        self.status_message = format!("Decoding .nsmp4: {}...", short_name(&entry));
        self.error_message = None;

        std::thread::spawn(move || {
            let outcome = File::open(&path)
                .map_err(|e| format!("open backup failed: {e}"))
                .and_then(|f| {
                    let mut r = BufReader::new(f);
                    archive
                        .read_entry(&mut r, &entry)
                        .map_err(|e| format!("read '{entry}' failed: {e}"))
                })
                .and_then(|bytes| {
                    SynthSampleSound::from_nsmp4_bytes(&bytes).map_err(|e| format!("{e}"))
                });

            let msg = match outcome {
                Ok(mut sound) => {
                    if sound.name.is_empty() {
                        sound.name = short_name(&entry).to_string();
                    }
                    LibraryLoadOutcome::SynthSample {
                        key: entry,
                        layer_idx,
                        sound: Arc::new(sound),
                        from_program,
                    }
                }
                Err(e) => LibraryLoadOutcome::Error(format!("NSMP4 decode error: {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    /// Loads a standalone `.npno` file (e.g., via CLI flag or drag-and-drop) on a background thread.
    pub fn request_piano_file(&mut self, path: PathBuf, slot_b: bool) {
        let tx = self.result_tx.clone();
        let key = path.display().to_string();
        let label = short_name(&key).to_string();
        self.pending_jobs += 1;
        self.status_message = format!("Decoding .npno: {label}...");
        self.error_message = None;

        std::thread::spawn(move || {
            let msg = match std::fs::read(&path) {
                Ok(bytes) => match NpnoSound::from_npno_bytes(&bytes) {
                    Ok(mut sound) => {
                        if sound.name.is_empty() {
                            sound.name = label;
                        }
                        LibraryLoadOutcome::Piano {
                            key,
                            slot_b,
                            sound: Arc::new(sound),
                            from_program: false,
                        }
                    }
                    Err(e) => LibraryLoadOutcome::Error(format!("NPNO decode error: {e}")),
                },
                Err(e) => LibraryLoadOutcome::Error(format!("Failed to read .npno: {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    /// Loads a standalone `.nsmp4` file (e.g., via CLI flag or drag-and-drop) on a background thread.
    pub fn request_sample_file(&mut self, path: PathBuf, layer_idx: usize) {
        let tx = self.result_tx.clone();
        let key = path.display().to_string();
        let label = short_name(&key).to_string();
        let layer_idx = layer_idx.min(2);
        self.pending_jobs += 1;
        self.status_message = format!("Decoding .nsmp4: {label}...");
        self.error_message = None;

        std::thread::spawn(move || {
            let msg = match std::fs::read(&path) {
                Ok(bytes) => match SynthSampleSound::from_nsmp4_bytes(&bytes) {
                    Ok(mut sound) => {
                        if sound.name.is_empty() {
                            sound.name = label;
                        }
                        LibraryLoadOutcome::SynthSample {
                            key,
                            layer_idx,
                            sound: Arc::new(sound),
                            from_program: false,
                        }
                    }
                    Err(e) => LibraryLoadOutcome::Error(format!("NSMP4 decode error: {e}")),
                },
                Err(e) => LibraryLoadOutcome::Error(format!("Failed to read .nsmp4: {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    /// Loads in-memory bytes of `.ns4p`, `.npno`, or `.nsmp4` (e.g. browser/OS byte drag-and-drop).
    pub fn load_dropped_bytes(&mut self, filename_hint: &str, bytes: &[u8]) {
        let lower = filename_hint.to_ascii_lowercase();
        if lower.ends_with(".npno") {
            let slot_b = self.piano_slot_b;
            match NpnoSound::from_npno_bytes(bytes) {
                Ok(sound) => {
                    let _ = self.result_tx.send(LibraryLoadOutcome::Piano {
                        key: filename_hint.to_string(),
                        slot_b,
                        sound: Arc::new(sound),
                        from_program: false,
                    });
                }
                Err(e) => {
                    self.error_message = Some(format!("NPNO decode error: {e}"));
                }
            }
        } else if lower.ends_with(".nsmp4") {
            let layer_idx = self.synth_layer_idx.min(2);
            match SynthSampleSound::from_nsmp4_bytes(bytes) {
                Ok(sound) => {
                    let _ = self.result_tx.send(LibraryLoadOutcome::SynthSample {
                        key: filename_hint.to_string(),
                        layer_idx,
                        sound: Arc::new(sound),
                        from_program: false,
                    });
                }
                Err(e) => {
                    self.error_message = Some(format!("NSMP4 decode error: {e}"));
                }
            }
        } else {
            match CbinFile::parse(bytes).and_then(|c| c.decode_program()) {
                Ok(prog) => {
                    let title = if prog.name.is_empty() {
                        short_name(filename_hint).to_string()
                    } else {
                        prog.name.clone()
                    };
                    let _ = self.result_tx.send(LibraryLoadOutcome::Program {
                        title,
                        program: Box::new(prog),
                    });
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to parse dropped file: {e}"));
                }
            }
        }
    }

    /// Polls decoded results from background threads and dispatches them to `AudioHandle` and `Stage4PanelState`.
    pub fn poll_results(&mut self, panel: &mut Stage4PanelState, audio: &AudioHandle) {
        while let Ok(outcome) = self.result_rx.try_recv() {
            self.pending_jobs = self.pending_jobs.saturating_sub(1);
            match outcome {
                LibraryLoadOutcome::Program { title, mut program } => {
                    program.name = title.clone();
                    panel.apply_program(&program);
                    audio.send_command(AudioCommand::LoadProgram(program));
                    self.error_message = None;
                    self.status_message = format!("Program: {title}");
                }
                LibraryLoadOutcome::Piano {
                    key,
                    slot_b,
                    sound,
                    from_program,
                } => {
                    let sound_name = sound.name.clone();
                    let zones = sound.strokes.len();
                    if let Some(prev) = self.piano_cache.insert(key, Arc::clone(&sound)) {
                        self.retired_pianos.push(prev);
                    }
                    audio.send_command(AudioCommand::LoadPianoSound {
                        slot_b,
                        sound: Some(sound),
                    });
                    if !slot_b {
                        panel.loaded_piano_name = Some(sound_name.clone());
                        if !from_program {
                            panel.piano_a_active = true;
                            audio.send_command(AudioCommand::SetLayerActive(2, true));
                        }
                    } else if !from_program {
                        panel.piano_b_active = true;
                        audio.send_command(AudioCommand::SetLayerActive(3, true));
                    }
                    let slot_lbl = if slot_b { "Piano B" } else { "Piano A" };
                    self.error_message = None;
                    self.status_message =
                        format!("{slot_lbl}: {sound_name} ({zones} decoded strokes)");
                }
                LibraryLoadOutcome::SynthSample {
                    key,
                    layer_idx,
                    sound,
                    from_program,
                } => {
                    let sound_name = sound.name.clone();
                    let zones = sound.zones.len();
                    if let Some(prev) = self.sample_cache.insert(key, Arc::clone(&sound)) {
                        self.retired_samples.push(prev);
                    }
                    audio.send_command(AudioCommand::LoadSynthSample {
                        layer_idx,
                        sound: Some(sound),
                    });
                    if layer_idx == 0 {
                        panel.loaded_sample_name = Some(sound_name.clone());
                        if !from_program {
                            panel.synth_a_active = true;
                            panel.synth_osc_mode = SynthOscMode::Sample;
                            audio.send_command(AudioCommand::SetLayerActive(4, true));
                            audio.send_command(AudioCommand::SetSynthOscMode(SynthOscMode::Sample));
                            if panel.synth_a_level < 0.05 {
                                panel.synth_a_level = 0.80;
                                audio.send_command(AudioCommand::SetSynthLevel(0.80));
                            }
                        }
                    } else if !from_program {
                        if layer_idx == 1 {
                            panel.synth_b_active = true;
                            audio.send_command(AudioCommand::SetLayerActive(5, true));
                        } else {
                            panel.synth_c_active = true;
                            audio.send_command(AudioCommand::SetLayerActive(6, true));
                        }
                    }
                    let slot_lbl = ["Synth A", "Synth B", "Synth C"][layer_idx.min(2)];
                    self.error_message = None;
                    self.status_message =
                        format!("{slot_lbl}: {sound_name} ({zones} decoded zones)");
                }
                LibraryLoadOutcome::Error(err) => {
                    self.error_message = Some(err);
                }
            }
        }
        self.reap_retired_sounds();
    }

    /// Drops retired `Arc` sound allocations only once the audio thread has released its reference.
    pub fn reap_retired_sounds(&mut self) {
        self.retired_pianos.retain(|arc| Arc::strong_count(arc) > 1);
        self.retired_samples
            .retain(|arc| Arc::strong_count(arc) > 1);
    }

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

fn short_name(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_synthetic_zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for (name, data) in files {
            let local_offset = out.len() as u32;
            let name_bytes = name.as_bytes();
            let crc = 0u32;
            let size = data.len() as u32;

            out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        let count = files.len() as u16;
        out.extend_from_slice(&central);

        out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn test_library_open_backup_and_load_program_and_reap() {
        let prog = ProgramDecoded {
            name: "Warm Stage".to_string(),
            ..Default::default()
        };
        let prog_bytes = prog.to_cbin_file(0, 0).expect("to_cbin_file").to_bytes();
        let meta = b"<backup file_version=\"1\" backup_format_version=\"1\" product_id=\"46\" product_version=\"166\" product_build=\"2934\" manager_version=\"786\" manager_build=\"1234\"/>";

        let zip_bytes = build_synthetic_zip_stored(&[
            ("meta.xml", meta),
            ("Program/Bank A/Warm Stage.ns4p", &prog_bytes),
            ("Piano/Grand/Dummy.npno", b"not-decoded-until-clicked"),
            ("Samp Lib/Keys/Dummy.nsmp4", b"not-decoded-until-clicked"),
        ]);

        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "ns4_gui_lib_test_{}_{}.ns4b",
            std::process::id(),
            prog_bytes.len()
        ));
        {
            let mut f = File::create(&tmp_path).expect("create temp ns4b");
            f.write_all(&zip_bytes).expect("write zip");
        }

        let mut lib = BackupLibraryState::new();
        lib.open_backup_path(&tmp_path).expect("open backup");
        assert_eq!(lib.programs, vec!["Program/Bank A/Warm Stage.ns4p"]);
        assert_eq!(lib.pianos, vec!["Piano/Grand/Dummy.npno"]);
        assert_eq!(lib.samples, vec!["Samp Lib/Keys/Dummy.nsmp4"]);
        assert_eq!(lib.manifest_summary.as_deref(), Some("v166 build 2934"));

        let (audio_handle, rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        lib.load_program_entry("Program/Bank A/Warm Stage.ns4p", &mut panel, &audio_handle);
        assert_eq!(panel.prog_title, "Warm Stage");
        assert!(matches!(rx.try_recv(), Ok(AudioCommand::LoadProgram(_))));

        // Verify retired Arc reaping respects audio thread ownership
        let sound_a = Arc::new(NpnoSound::new_synthetic("A", 60, &[0.1, -0.1], 48000.0));
        let sound_b = Arc::new(NpnoSound::new_synthetic("B", 60, &[0.2, -0.2], 48000.0));
        let _ = lib.result_tx.send(LibraryLoadOutcome::Piano {
            key: "shared_slot".to_string(),
            slot_b: false,
            sound: Arc::clone(&sound_a),
            from_program: false,
        });
        lib.poll_results(&mut panel, &audio_handle);
        assert_eq!(panel.loaded_piano_name.as_deref(), Some("A"));

        // Second replacement pushes `sound_a` to `retired_pianos` while audio queue still holds a ref
        let _ = lib.result_tx.send(LibraryLoadOutcome::Piano {
            key: "shared_slot".to_string(),
            slot_b: false,
            sound: Arc::clone(&sound_b),
            from_program: false,
        });
        lib.poll_results(&mut panel, &audio_handle);
        assert_eq!(lib.retired_pianos.len(), 1);

        // Drain audio queue (dropping audio thread's Arc refs), keep local `sound_a` cleared, then reap
        drop(sound_a);
        while rx.try_recv().is_ok() {}
        lib.reap_retired_sounds();
        assert_eq!(lib.retired_pianos.len(), 0);

        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn test_library_load_program_auto_resolves_piano_and_sample() {
        let piano_stem = "Astoria Grand Lrg 6.1";
        let sample_stem = "Ac Guitar Finger_ST Mart 4.1";
        let mut prog = ProgramDecoded {
            name: "Auto Resolve".to_string(),
            ..Default::default()
        };
        prog.global.scene_1_mask = (1 << 2) | (1 << 4);
        prog.piano.layer_a.enabled = true;
        prog.piano.layer_a.name_hash = nord_core::cbin::nord_name_hash(piano_stem);
        prog.synth.layer_a.enabled = true;
        prog.synth.layer_a.osc_mode = SynthOscMode::Sample;
        prog.synth.layer_a.sample_id = nord_core::cbin::nord_name_hash(sample_stem);
        let prog_bytes = prog.to_cbin_file(0, 0).expect("to_cbin_file").to_bytes();
        let meta = b"<backup file_version=\"1\" backup_format_version=\"1\" product_id=\"46\" product_version=\"166\" product_build=\"2934\" manager_version=\"786\" manager_build=\"1234\"/>";

        let zip_bytes = build_synthetic_zip_stored(&[
            ("meta.xml", meta),
            ("Program/Bank A/Auto Resolve.ns4p", &prog_bytes),
            (
                "Piano/Grand/Astoria Grand Lrg 6.1.npno",
                b"invalid-or-cached",
            ),
            (
                "Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4",
                b"invalid-or-cached",
            ),
        ]);
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "ns4_gui_autoresolve_test_{}.ns4b",
            std::process::id()
        ));
        std::fs::write(&tmp_path, &zip_bytes).expect("write temp ns4b");

        let mut lib = BackupLibraryState::new();
        lib.open_backup_path(&tmp_path).expect("open backup");
        // Seed caches so worker spawns aren't racing on synthetic non-CBIN payloads
        lib.piano_cache.insert(
            "Piano/Grand/Astoria Grand Lrg 6.1.npno".to_string(),
            Arc::new(NpnoSound::new_synthetic(
                piano_stem,
                60,
                &[0.1, -0.1],
                48000.0,
            )),
        );
        lib.sample_cache.insert(
            "Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4".to_string(),
            Arc::new(SynthSampleSound::new_synthetic(
                sample_stem,
                60,
                &[0.2, -0.2],
                48000.0,
                None,
                0,
            )),
        );

        let (audio_handle, rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        lib.drawer_open = true;
        lib.load_program_entry(
            "Program/Bank A/Auto Resolve.ns4p",
            &mut panel,
            &audio_handle,
        );
        assert!(lib.status_message.contains("+2 resolved sample banks"));
        assert!(
            !lib.drawer_open,
            "drawer should auto-close after loading a program by default"
        );
        lib.drawer_open = true;
        lib.auto_close_on_load = false;
        lib.load_program_entry(
            "Program/Bank A/Auto Resolve.ns4p",
            &mut panel,
            &audio_handle,
        );
        assert!(
            lib.drawer_open,
            "drawer should remain open when auto_close_on_load is disabled"
        );

        lib.poll_results(&mut panel, &audio_handle);
        assert_eq!(panel.loaded_piano_name.as_deref(), Some(piano_stem));
        assert_eq!(panel.loaded_sample_name.as_deref(), Some(sample_stem));
        let cmds: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(cmds
            .iter()
            .any(|c| matches!(c, AudioCommand::LoadProgram(_))));
        assert!(cmds.iter().any(|c| matches!(
            c,
            AudioCommand::LoadPianoSound {
                slot_b: false,
                sound: Some(_)
            }
        )));
        assert!(cmds.iter().any(|c| matches!(
            c,
            AudioCommand::LoadSynthSample {
                layer_idx: 0,
                sound: Some(_)
            }
        )));

        let _ = std::fs::remove_file(&tmp_path);
    }
}
