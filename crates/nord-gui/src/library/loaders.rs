//! Asynchronous `.npno` / `.nsmp4` / dropped-file loading for the library drawer.

use super::{short_name, BackupLibraryState, LibraryLoadOutcome};
use nord_core::cbin::CbinFile;
use nord_core::piano::NpnoSound;
use nord_core::synth::SynthSampleSound;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

impl BackupLibraryState {
    /// Schedules decoding of a `.npno` entry from the opened `.ns4b` archive (or serves from cache).
    pub fn request_piano_entry(&mut self, entry_name: &str) {
        self.schedule_piano_entry(entry_name, self.piano_slot_b, false);
        if self.auto_close_on_load {
            self.drawer_open = false;
        }
    }

    pub(super) fn schedule_piano_entry(
        &mut self,
        entry_name: &str,
        slot_b: bool,
        from_program: bool,
    ) {
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

    pub(super) fn schedule_sample_entry(
        &mut self,
        entry_name: &str,
        layer_idx: usize,
        from_program: bool,
    ) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_without_archive_reports_error_and_bad_drop_is_rejected() {
        let mut lib = BackupLibraryState::new();
        lib.request_piano_entry("Piano/x.npno");
        assert_eq!(
            lib.error_message.as_deref(),
            Some("Open an .ns4b archive first")
        );
        assert_eq!(lib.pending_jobs, 0);

        lib.error_message = None;
        lib.load_dropped_bytes("bad.npno", b"nope");
        assert!(lib
            .error_message
            .as_deref()
            .is_some_and(|m| m.starts_with("NPNO decode error")));
        lib.error_message = None;
        lib.load_dropped_bytes("bad.nsmp4", b"nope");
        assert!(lib
            .error_message
            .as_deref()
            .is_some_and(|m| m.starts_with("NSMP4 decode error")));
    }
}
