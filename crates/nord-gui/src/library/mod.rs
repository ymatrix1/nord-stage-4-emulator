//! Backup Archive (`.ns4b`) and CNSP Sample (`.npno` / `.nsmp4`) Browser & Async Loader.
//!
//! Decodes multi-megabyte `.npno` and `.nsmp4` files on a background worker thread and keeps
//! retained `Arc` references on the UI thread so the real-time CPAL audio callback never drops
//! heap allocations when hot-swapping instruments.

mod export;
mod loaders;
mod piano_bridge;
mod piano_catalog;
mod poll;
mod preset_export;
mod preset_load;
mod program;
#[cfg(test)]
mod support;
mod ui;

use crate::panel::bay_program::store::PER_BANK;
use crate::panel::FwMessage;
use crossbeam_channel::{unbounded, Receiver, Sender};
use nord_core::cbin::{Ns4BackupArchive, PresetIndex, PresetSection, ProgramDecoded, SlotIndex};
use nord_core::piano::{NpnoSound, PianoCategory};
use nord_core::synth::SynthSampleSound;
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
    /// `.npno` entry names in hardware order (type, then position in the type).
    pub pianos: Vec<String>,
    /// Piano type of each entry of `pianos` (parallel).
    pub piano_types: Vec<PianoCategory>,
    pub samples: Vec<String>,
    pub filter: String,
    pub active_tab: BackupTab,
    pub piano_slot_b: bool,
    pub synth_layer_idx: usize,
    pub pending_jobs: usize,
    /// Firmware messages for the Program OLED raised by loads (`Piano not found`, ...); the panel
    /// poll turns them into toasts.
    notices: Vec<FwMessage>,
    pub status_message: String,
    pub error_message: Option<String>,
    /// Where Export backup writes (empty: `<name>-edited.ns4b` next to the opened backup).
    pub export_path: String,
    /// `(flat location, entry path)` of every program of the opened backup, from the entry headers.
    program_slots: Vec<(u16, String)>,
    /// The same for the Preset Library: `(section, flat location, entry path)`.
    preset_slots: Vec<(PresetSection, u16, String)>,
    layout_rev: u32,
    synced_layout_rev: u32,
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
            piano_types: Vec::new(),
            samples: Vec::new(),
            filter: String::new(),
            active_tab: BackupTab::Programs,
            piano_slot_b: false,
            synth_layer_idx: 0,
            pending_jobs: 0,
            notices: Vec::new(),
            status_message: "No .ns4b backup loaded".to_string(),
            error_message: None,
            export_path: String::new(),
            program_slots: Vec::new(),
            preset_slots: Vec::new(),
            layout_rev: 0,
            synced_layout_rev: 0,
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

        // Which entry sits at which location (header bank/location), for Organize and Export.
        let slots = SlotIndex::read(&archive, &mut reader)
            .map(|idx| {
                idx.programs
                    .into_iter()
                    .filter(|(b, l, _)| *b < 8 && *l < PER_BANK)
                    .map(|(b, l, n)| (b * PER_BANK + l, n))
                    .collect()
            })
            .unwrap_or_default();

        let preset_slots = PresetIndex::read(&archive, &mut reader)
            .map(|idx| {
                PresetSection::ALL
                    .into_iter()
                    .flat_map(|s| idx.slots(s).into_iter().map(move |(i, n)| (s, i, n)))
                    .collect()
            })
            .unwrap_or_default();

        let mut programs: Vec<String> = archive
            .entries_with_ext(".ns4p")
            .into_iter()
            .map(|e| e.name.clone())
            .collect();
        programs.sort();

        let models = piano_catalog::read_models(&archive, &mut reader);
        let pianos: Vec<String> = models.iter().map(|m| m.entry.clone()).collect();
        let piano_types: Vec<PianoCategory> = models.iter().map(|m| m.category).collect();

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
        self.export_path = export::default_export_path(path).display().to_string();
        self.program_slots = slots;
        self.preset_slots = preset_slots;
        self.layout_rev = self.layout_rev.wrapping_add(1);
        self.archive = Some(Arc::new(archive));
        self.manifest_summary = manifest_summary;
        self.programs = programs;
        self.pianos = pianos;
        self.piano_types = piano_types;
        self.samples = samples;
        self.reap_retired_sounds();
        Ok(())
    }

    /// Drops retired `Arc` sound allocations only once the audio thread has released its reference.
    pub fn reap_retired_sounds(&mut self) {
        self.retired_pianos.retain(|arc| Arc::strong_count(arc) > 1);
        self.retired_samples
            .retain(|arc| Arc::strong_count(arc) > 1);
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
    use super::support::build_synthetic_zip_stored;
    use super::*;
    use crate::audio::{AudioCommand, AudioHandle};
    use crate::panel::Stage4PanelState;
    use std::io::Write;

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
}
