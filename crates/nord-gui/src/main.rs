//! Nord Stage 4 Standalone Application Entry Point.
//!
//! Launches the interactive 1:1 hardware control surface and spawns the
//! real-time CPAL audio host thread and MIDIR input listener.

use eframe::egui;
use nord_gui::audio::start_audio_and_midi;
use nord_gui::panel::{render_stage4_panel, Stage4PanelState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Stage4App {
    state: Stage4PanelState,
    audio_handle: nord_gui::audio::AudioHandle,
    active_keys: Arc<[AtomicBool; 128]>,
}

impl Stage4App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        nord_gui::style::setup_nord_visuals(&cc.egui_ctx);

        let active_keys = Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        let audio_handle = start_audio_and_midi(Arc::clone(&active_keys));

        let state = Stage4PanelState {
            panel_texture: Some(nord_gui::style::load_panel_texture(&cc.egui_ctx)),
            ..Default::default()
        };

        Self {
            state,
            audio_handle,
            active_keys,
        }
    }
}

impl eframe::App for Stage4App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Sync active keys from MIDI input into UI state
        for (i, pressed) in self.state.midi_active_keys.iter_mut().enumerate() {
            *pressed = self.active_keys[i].load(Ordering::Relaxed);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(nord_gui::style::CHASSIS_RED))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_stage4_panel(ui, &mut self.state, &self.audio_handle);
                    });
            });

        // Request repaint if keys are pressed or audio is playing
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1580.0, 440.0])
            .with_min_inner_size([960.0, 360.0])
            .with_title("Clavia Nord Stage 4"),
        ..Default::default()
    };

    eframe::run_native(
        "Nord Stage 4",
        native_options,
        Box::new(|cc| Ok(Box::new(Stage4App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_version() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    fn test_active_keys_atomic_init() {
        let keys: Arc<[AtomicBool; 128]> =
            Arc::new(std::array::from_fn(|_| AtomicBool::new(false)));
        assert!(!keys[60].load(Ordering::Relaxed));
        keys[60].store(true, Ordering::Relaxed);
        assert!(keys[60].load(Ordering::Relaxed));
    }
}
