//! Result polling: dispatches decoded background jobs to the audio thread and panel state.

use super::{BackupLibraryState, LibraryLoadOutcome};
use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use nord_core::synth::SynthOscMode;
use std::sync::Arc;

impl BackupLibraryState {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
