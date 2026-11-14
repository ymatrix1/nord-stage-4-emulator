//! Result polling: dispatches decoded background jobs to the audio thread and panel state.

use super::{BackupLibraryState, LibraryLoadOutcome};
use crate::audio::{AudioCommand, AudioHandle, PianoEdit};
use crate::panel::{LoadRequest, Stage4PanelState};
use nord_core::piano::PianoCategory;
use nord_core::synth::SynthOscMode;
use std::sync::Arc;

impl BackupLibraryState {
    /// Hands the backup names to the Program area (LIST, Preset Library, Num order) and runs the
    /// load requests it queued (program dial, program buttons, preset browser).
    pub fn sync_program_panel(&mut self, panel: &mut Stage4PanelState, audio: &AudioHandle) {
        panel
            .program_ui
            .sync_catalog(&self.programs, &self.pianos, &self.samples);
        if self.synced_layout_rev != self.layout_rev {
            panel.program_ui.library.set_factory(&self.program_slots);
            panel.program_ui.organize.reset();
            self.synced_layout_rev = self.layout_rev;
        }
        for req in panel.program_ui.take_requests() {
            match req {
                LoadRequest::Program(entry) => self.load_program_entry(&entry, panel, audio),
                LoadRequest::Piano { entry, slot_b } => {
                    self.piano_slot_b = slot_b;
                    self.request_piano_entry(&entry);
                }
                LoadRequest::Sample { entry, layer } => {
                    self.synth_layer_idx = layer.min(2);
                    self.request_sample_entry(&entry);
                }
            }
        }
    }

    /// Polls decoded results from background threads and dispatches them to `AudioHandle` and `Stage4PanelState`.
    pub fn poll_results(&mut self, panel: &mut Stage4PanelState, audio: &AudioHandle) {
        self.sync_program_panel(panel, audio);
        while let Ok(outcome) = self.result_rx.try_recv() {
            self.pending_jobs = self.pending_jobs.saturating_sub(1);
            match outcome {
                LibraryLoadOutcome::Program { title, mut program } => {
                    program.name = title.clone();
                    panel.apply_program(&program);
                    audio.send_command(AudioCommand::LoadProgram(program));
                    panel.program_ui.mark_clean();
                    self.error_message = None;
                    self.status_message = format!("Program: {title}");
                }
                LibraryLoadOutcome::Piano {
                    key,
                    slot_b,
                    sound,
                    from_program,
                } => {
                    if from_program {
                        panel.program_ui.mark_clean(); // follows a program load
                    }
                    let sound_name = sound.name.clone();
                    let zones = sound.strokes.len();
                    // The panel type follows the model on the focused layer: header type of the
                    // file, else the archive folder; the engine type drives the timbre sets.
                    let category = sound
                        .category
                        .or_else(|| PianoCategory::from_entry_path(&key));
                    if let Some(c) = category {
                        if slot_b == (panel.piano_ui.focus == 1) {
                            panel.piano_cat = c;
                            audio.send_command(AudioCommand::Piano(PianoEdit::Category(c)));
                        }
                    }
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
                    if from_program {
                        panel.program_ui.mark_clean(); // follows a program load
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_panel_receives_names_and_its_requests_are_run() {
        let mut lib = BackupLibraryState::new();
        let (audio, _rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        lib.programs = vec!["Program/Bank A/One.ns4p".to_string()];
        lib.pianos = vec!["Piano/p.npno".to_string()];
        lib.sync_program_panel(&mut panel, &audio);
        assert_eq!(panel.program_ui.catalog.programs.len(), 1);
        panel
            .program_ui
            .requests
            .push(LoadRequest::Program("Program/Bank A/One.ns4p".into()));
        panel.program_ui.requests.push(LoadRequest::Piano {
            entry: "Piano/p.npno".into(),
            slot_b: true,
        });
        lib.sync_program_panel(&mut panel, &audio);
        assert!(panel.program_ui.requests.is_empty(), "queue drained");
        assert!(lib.piano_slot_b, "slot B was requested");
        assert!(
            lib.error_message.is_some(),
            "no archive open is reported, not dropped"
        );
    }

    #[test]
    fn test_poll_results_reports_error_outcome() {
        let mut lib = BackupLibraryState::new();
        let (audio, _rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        lib.pending_jobs = 1;
        let _ = lib
            .result_tx
            .send(LibraryLoadOutcome::Error("boom".to_string()));
        lib.poll_results(&mut panel, &audio);
        assert_eq!(lib.pending_jobs, 0);
        assert_eq!(lib.error_message.as_deref(), Some("boom"));
    }

    fn piano_outcome(
        key: &str,
        slot_b: bool,
        header_type: Option<PianoCategory>,
    ) -> LibraryLoadOutcome {
        let mut sound = nord_core::piano::NpnoSound::new_synthetic("x", 60, &[0.1; 64], 48_000.0);
        sound.category = header_type;
        LibraryLoadOutcome::Piano {
            key: key.to_string(),
            slot_b,
            sound: Arc::new(sound),
            from_program: false,
        }
    }

    #[test]
    fn test_loaded_piano_sets_the_panel_and_engine_type() {
        let mut lib = BackupLibraryState::new();
        let (audio, rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();
        // The archive folder decides when the sound has no header type.
        lib.pending_jobs = 2;
        let _ = lib
            .result_tx
            .send(piano_outcome("Piano/Electric/x.npno", false, None));
        lib.poll_results(&mut panel, &audio);
        assert_eq!(panel.piano_cat, PianoCategory::Electric);
        assert!(rx.try_iter().any(|c| matches!(
            c,
            AudioCommand::Piano(PianoEdit::Category(PianoCategory::Electric))
        )));
        // The header type wins; a model for the unfocused layer B leaves the type alone.
        let _ = lib
            .result_tx
            .send(piano_outcome("loose.npno", true, Some(PianoCategory::Clav)));
        lib.poll_results(&mut panel, &audio);
        assert_eq!(panel.piano_cat, PianoCategory::Electric);
        let _ = lib.result_tx.send(piano_outcome(
            "loose.npno",
            false,
            Some(PianoCategory::Clav),
        ));
        lib.poll_results(&mut panel, &audio);
        assert_eq!(panel.piano_cat, PianoCategory::Clav);
    }
}
