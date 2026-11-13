//! Draining decoded results from worker threads and RT-safe `Arc` retirement.

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

    /// Drops retired `Arc` sound allocations only once the audio thread has released its reference.
    pub fn reap_retired_sounds(&mut self) {
        self.retired_pianos.retain(|arc| Arc::strong_count(arc) > 1);
        self.retired_samples
            .retain(|arc| Arc::strong_count(arc) > 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::piano::NpnoSound;

    #[test]
    fn test_retired_arc_reaping_respects_audio_thread_ownership() {
        let mut lib = BackupLibraryState::new();
        let (audio_handle, rx) = AudioHandle::new();
        let mut panel = Stage4PanelState::default();

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
    }
}
