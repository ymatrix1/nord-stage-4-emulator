//! Backup Archive (`.ns4b`) and CNSP Sample (`.npno` / `.nsmp4`) Browser & Async Loader.
//!
//! Decodes multi-megabyte `.npno` and `.nsmp4` files on a background worker thread and keeps
//! retained `Arc` references on the UI thread so the real-time CPAL audio callback never drops
//! heap allocations when hot-swapping instruments.

mod archive;
mod loaders;
mod poll;
#[cfg(test)]
mod support;
mod ui;

use crossbeam_channel::{unbounded, Receiver, Sender};
use nord_core::cbin::{Ns4BackupArchive, ProgramDecoded};
use nord_core::piano::NpnoSound;
use nord_core::synth::SynthSampleSound;
use std::collections::HashMap;
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
}

/// File stem of `path` (used as a display title), falling back to the whole string.
fn short_name(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_name_strips_directory_and_extension() {
        assert_eq!(short_name("Program/Bank A/Warm Stage.ns4p"), "Warm Stage");
        assert_eq!(short_name("plain"), "plain");
    }

    #[test]
    fn test_new_library_starts_closed_and_empty() {
        let lib = BackupLibraryState::default();
        assert!(!lib.drawer_open);
        assert!(lib.auto_close_on_load);
        assert!(lib.programs.is_empty() && lib.pianos.is_empty() && lib.samples.is_empty());
        assert_eq!(lib.active_tab, BackupTab::Programs);
        assert_eq!(lib.status_message, "No .ns4b backup loaded");
    }
}
