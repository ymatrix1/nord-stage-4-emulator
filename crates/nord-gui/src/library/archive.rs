//! Opening `.ns4b` archives and loading `.ns4p` program entries from them.

use super::BackupLibraryState;
use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use nord_core::cbin::{CbinFile, Ns4BackupArchive};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

impl BackupLibraryState {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::support::{write_temp_ns4b, META};
    use nord_core::cbin::ProgramDecoded;
    use nord_core::piano::NpnoSound;
    use nord_core::synth::{SynthOscMode, SynthSampleSound};

    #[test]
    fn test_library_open_backup_and_load_program() {
        let prog = ProgramDecoded {
            name: "Warm Stage".to_string(),
            ..Default::default()
        };
        let prog_bytes = prog.to_cbin_file(0, 0).expect("to_cbin_file").to_bytes();
        let tmp_path = write_temp_ns4b(
            "open_load",
            &[
                ("meta.xml", META),
                ("Program/Bank A/Warm Stage.ns4p", &prog_bytes),
                ("Piano/Grand/Dummy.npno", b"not-decoded-until-clicked"),
                ("Samp Lib/Keys/Dummy.nsmp4", b"not-decoded-until-clicked"),
            ],
        );

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

        let tmp_path = write_temp_ns4b(
            "autoresolve",
            &[
                ("meta.xml", META),
                ("Program/Bank A/Auto Resolve.ns4p", &prog_bytes),
                (
                    "Piano/Grand/Astoria Grand Lrg 6.1.npno",
                    b"invalid-or-cached",
                ),
                (
                    "Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4",
                    b"invalid-or-cached",
                ),
            ],
        );

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
