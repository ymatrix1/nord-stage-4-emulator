//! Program-entry loading from an opened `.ns4b` archive.

use super::BackupLibraryState;
use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use nord_core::cbin::CbinFile;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

impl BackupLibraryState {
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
    use super::super::support::build_synthetic_zip_stored;
    use super::*;
    use nord_core::cbin::ProgramDecoded;
    use nord_core::piano::NpnoSound;
    use nord_core::synth::{SynthOscMode, SynthSampleSound};
    use std::sync::Arc;

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
