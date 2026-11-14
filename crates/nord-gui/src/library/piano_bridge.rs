//! Connects the Piano bay's PIANO SELECT dial to the piano library: publishes the model names of
//! the opened `.ns4b` and loads the model the dial asked for.

use super::{short_name, BackupLibraryState};
use crate::panel::Stage4PanelState;

impl BackupLibraryState {
    /// Call once per frame before the panel is drawn: copy the piano model list into
    /// `panel.piano_ui`, follow the loaded model with the dial position, and start loading a
    /// model the dial requested (on the focused Piano layer).
    pub fn sync_piano_ui(&mut self, panel: &mut Stage4PanelState) {
        if panel.piano_ui.models != self.pianos {
            panel.piano_ui.models = self.pianos.clone();
            let last = panel.piano_ui.models.len().saturating_sub(1);
            panel.piano_ui.model_idx = panel.piano_ui.model_idx.min(last);
        }
        if let Some(name) = panel.loaded_piano_name.as_deref() {
            if let Some(i) = self
                .pianos
                .iter()
                .position(|e| short_name(e).eq_ignore_ascii_case(name))
            {
                if panel.piano_ui.model_request.is_none() {
                    panel.piano_ui.model_idx = i;
                }
            }
        }
        if let Some(entry) = panel.piano_ui.model_request.take() {
            self.piano_slot_b = panel.piano_ui.focus == 1;
            self.schedule_piano_entry(&entry, self.piano_slot_b, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_with(pianos: &[&str]) -> BackupLibraryState {
        let mut lib = BackupLibraryState::new();
        lib.pianos = pianos.iter().map(|s| s.to_string()).collect();
        lib
    }

    #[test]
    fn model_list_is_published_to_the_panel() {
        let mut lib = lib_with(&["Piano/A.npno", "Piano/B.npno"]);
        let mut panel = Stage4PanelState::default();
        lib.sync_piano_ui(&mut panel);
        assert_eq!(panel.piano_ui.models, lib.pianos);
    }

    #[test]
    fn dial_follows_the_loaded_model() {
        let mut lib = lib_with(&["Piano/A.npno", "Piano/B.npno"]);
        let mut panel = Stage4PanelState {
            loaded_piano_name: Some("b".into()),
            ..Stage4PanelState::default()
        };
        lib.sync_piano_ui(&mut panel);
        assert_eq!(panel.piano_ui.model_idx, 1);
    }

    #[test]
    fn dial_request_is_consumed_and_scheduled_on_the_focused_layer() {
        let mut lib = lib_with(&["Piano/A.npno"]);
        let mut panel = Stage4PanelState::default();
        panel.piano_ui.focus = 1;
        panel.piano_ui.model_request = Some("Piano/A.npno".into());
        lib.sync_piano_ui(&mut panel);
        assert!(panel.piano_ui.model_request.is_none());
        assert!(lib.piano_slot_b);
        // No archive is open, so the job is rejected with a message instead of panicking.
        assert!(lib.error_message.is_some());
    }
}
