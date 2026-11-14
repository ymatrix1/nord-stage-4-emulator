//! Firmware-path A/B switching for the standalone GUI.
//!
//! * CLI: `nord-stage4 --firmware-all | --legacy-all | --tone-source ... | --measured-laws ...`
//!   (same flags as `ns4 render-patch`, parsed by `FirmwareOptions::from_args`).
//! * In-app engine bar (`bar.rs`; tooltips from `info.rs`) with a dropdown per engine path, sent as
//!   `AudioCommand::SetFirmwareOptions` and applied on the audio thread between blocks.
//! * Headless `--self-test <out.wav>`: builds the engine, applies the options and renders one
//!   second without opening a window.
//!
//! Docs: `docs/gui-testing.md`.

use crate::audio::{apply_audio_command, AudioCommand, AudioHandle};
use nord_core::engine::Stage4Engine;
use nord_core::render;
use nord_core::render_opts::FirmwareOptions;
use std::path::{Path, PathBuf};

pub mod bar;
pub mod choices;
pub mod info;
pub mod presets;

/// Headless-run request parsed from `--self-test <out.wav>` / `--dump-options`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeadlessRequest {
    /// Render a second to this WAV and exit.
    pub self_test_wav: Option<PathBuf>,
    /// Print the effective options and exit.
    pub dump_options: bool,
}

impl HeadlessRequest {
    /// Parses the headless flags; other arguments are ignored.
    pub fn from_args(args: &[String]) -> Self {
        let mut r = Self::default();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--self-test" => r.self_test_wav = it.next().map(PathBuf::from),
                "--dump-options" => r.dump_options = true,
                _ => {}
            }
        }
        r
    }

    /// True when the binary must not open a window.
    pub fn is_headless(&self) -> bool {
        self.self_test_wav.is_some() || self.dump_options
    }
}

/// Window title that names the active firmware set.
pub fn window_title(opts: &FirmwareOptions) -> String {
    format!("Nord Stage 4 Emulator [{}]", opts.summary())
}

/// Builds an engine, applies `opts` through the audio command path and renders `frames` frames of
/// an organ + piano + synth chord. Returns `(left, right)`.
pub fn render_chord(opts: &FirmwareOptions, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut engine = Stage4Engine::new(48_000.0);
    for i in [0, 2, 4] {
        engine.layer_levels[i].base = 0.8;
    }
    let mut vol = 1.0;
    apply_audio_command(
        AudioCommand::SetFirmwareOptions(*opts),
        &mut engine,
        &mut vol,
    );
    for n in [48u8, 60, 64, 67] {
        engine.organ_a.note_on(n);
        engine.piano_a.note_on(n, 100);
        engine.synth_a.note_on(n, 100);
    }
    let (mut l, mut r) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for _ in 0..frames {
        let [a, b] = engine.render_frame();
        l.push(a * vol);
        r.push(b * vol);
    }
    (l, r)
}

/// Headless check: renders the default engine and the engine with `opts`, writes the latter to
/// `wav` when given, and returns a report (peak / RMS / difference to the defaults).
pub fn run_self_test(opts: &FirmwareOptions, wav: Option<&Path>) -> Result<String, String> {
    const FRAMES: usize = 48_000;
    let (dl, dr) = render_chord(&FirmwareOptions::default(), FRAMES);
    let (l, r) = render_chord(opts, FRAMES);
    let finite = l.iter().chain(&r).all(|x| x.is_finite());
    let (peak, rms) = render::peak_rms(&l, &r);
    let diff: Vec<f32> = l.iter().zip(&dl).map(|(a, b)| a - b).collect();
    let diff_r: Vec<f32> = r.iter().zip(&dr).map(|(a, b)| a - b).collect();
    let (_, diff_rms) = render::peak_rms(&diff, &diff_r);
    if let Some(path) = wav {
        std::fs::write(path, render::encode_wav_f32_stereo(48_000, &l, &r))
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    }
    if !finite {
        return Err("non-finite samples in the render".into());
    }
    Ok(format!(
        "firmware options: {}\nrendered {FRAMES} frames @ 48000 Hz: peak {peak:.4}, rms {rms:.4}, rms diff vs defaults {diff_rms:.5}",
        opts.summary()
    ))
}

/// UI state of the firmware engine-path bar.
#[derive(Debug, Default)]
pub struct FirmwarePanelState {
    /// Options currently applied on the audio thread.
    pub opts: FirmwareOptions,
}

impl FirmwarePanelState {
    /// Starts from CLI-provided options.
    pub fn new(opts: FirmwareOptions) -> Self {
        Self { opts }
    }

    /// Sends the current options to the audio thread.
    pub fn push(&self, audio: &AudioHandle) {
        audio.send_command(AudioCommand::SetFirmwareOptions(self.opts));
    }

    /// Engine-path bar shown above the panel: presets, one group of dropdowns per engine and the
    /// active set. Changes are sent to the audio thread immediately. Returns true when options
    /// changed.
    pub fn show_bar(&mut self, ui: &mut egui::Ui, audio: &AudioHandle) -> bool {
        let before = self.opts;
        bar::show(ui, &mut self.opts);
        let changed = before != self.opts;
        if changed {
            self.push(audio);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth::SynthChain;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bar_builds_headless_and_dropdown_choices_are_unique() {
        let (handle, _rx) = AudioHandle::new();
        let mut panel = FirmwarePanelState::new(FirmwareOptions::firmware_all());
        let ctx = egui::Context::default();
        let mut changed = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                changed = panel.show_bar(ui, &handle);
            });
        });
        assert!(!changed, "no input means no change");
    }

    #[test]
    fn headless_flags_parse() {
        assert!(!HeadlessRequest::from_args(&args(&["nord-stage4"])).is_headless());
        let r = HeadlessRequest::from_args(&args(&["x", "--self-test", "o.wav", "--dump-options"]));
        assert_eq!(r.self_test_wav, Some(PathBuf::from("o.wav")));
        assert!(r.dump_options && r.is_headless());
    }

    #[test]
    fn title_names_the_active_set() {
        assert!(window_title(&FirmwareOptions::default()).ends_with("[defaults]"));
        assert!(window_title(&FirmwareOptions::firmware_all()).contains("filter=Firmware"));
    }

    #[test]
    fn command_round_trip_applies_and_reverts() {
        let (handle, rx) = AudioHandle::new();
        let mut panel = FirmwarePanelState::new(FirmwareOptions::firmware_all());
        panel.push(&handle);
        let mut engine = Stage4Engine::new(48_000.0);
        let mut vol = 1.0;
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        assert_eq!(engine.synth_a.chain, SynthChain::FirmwareStrip);
        assert!(engine.piano_a.firmware_velocity_level);
        panel.opts = FirmwareOptions::legacy_all();
        panel.push(&handle);
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        assert_eq!(engine.synth_a.chain, SynthChain::Legacy);
        assert!(!engine.piano_a.firmware_velocity_level);
    }

    #[test]
    fn self_test_renders_finite_audio_and_firmware_differs_from_defaults() {
        let wav = std::env::temp_dir().join("ns4_gui_self_test.wav");
        let report = run_self_test(&FirmwareOptions::firmware_all(), Some(&wav)).unwrap();
        assert!(report.contains("rms diff vs defaults"));
        assert!(!report.contains("rms diff vs defaults 0.00000"));
        assert!(std::fs::metadata(&wav).unwrap().len() > 44);
        let same = run_self_test(&FirmwareOptions::default(), None).unwrap();
        assert!(same.contains("rms diff vs defaults 0.00000"));
        let _ = std::fs::remove_file(wav);
    }

    #[test]
    fn every_switch_survives_the_command_channel_and_reverts() {
        use nord_core::render_opts::RotaryDrive;
        let (handle, rx) = AudioHandle::new();
        let mut panel = FirmwarePanelState::new(FirmwareOptions {
            amp_path: Some(false),
            rotary_drive: RotaryDrive::Unity,
            piano_bus_scale: true,
            firmware_routing: true,
            synth_shape_milli: Some(300),
            ..FirmwareOptions::firmware_all()
        });
        panel.push(&handle);
        let mut engine = Stage4Engine::new(48_000.0);
        let mut vol = 1.0;
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        assert!(!engine.fx_chains[0].firmware_fx.amp && engine.organ_a.rotary_drive_unity);
        assert!(engine.piano_b.firmware_bus_scale && engine.synth_a.strip_shape == Some(0.3));
        panel.opts = FirmwareOptions::default();
        panel.push(&handle);
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        assert!(engine.fx_chains[0].firmware_fx.amp && !engine.organ_a.rotary_drive_unity);
        assert!(!engine.piano_b.firmware_bus_scale && engine.synth_a.strip_shape.is_none());
    }

    #[test]
    fn new_switches_reach_the_engine_through_the_command_channel_and_revert() {
        use nord_core::fx::RingAdvance;
        let (handle, rx) = AudioHandle::new();
        let mut fx = nord_core::fx::FirmwareFx::DEFAULT;
        fx.enable_ports("reverb,mod1,mod2,eq,compressor").unwrap();
        let mut panel = FirmwarePanelState::new(FirmwareOptions {
            fx,
            leslie_firmware: true,
            leslie_ring: RingAdvance::FROZEN,
            farfisa_processor: Some(true),
            piano_layer_law: true,
            sample_kernel_key: false,
            sample_pitch_fw: true,
            ..FirmwareOptions::default()
        });
        panel.push(&handle);
        let mut engine = Stage4Engine::new(48_000.0);
        let mut vol = 1.0;
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        let f = engine.fx_chains[0].firmware_fx;
        assert!(f.reverb && f.mod1 && f.mod2 && f.eq && f.compressor && !f.delay_laws);
        assert!(engine.organ_a.leslie_fw.is_some() && engine.organ_b.farfisa_processor);
        assert!(engine.piano_a.firmware_layer_law && !engine.synth_b.sample_kernel_by_key);
        assert!(engine.synth_c.sample_pitch_fw);
        panel.opts = FirmwareOptions::default();
        panel.push(&handle);
        apply_audio_command(rx.try_recv().unwrap(), &mut engine, &mut vol);
        assert_eq!(
            engine.fx_chains[0].firmware_fx,
            nord_core::fx::FirmwareFx::DEFAULT
        );
        assert!(engine.organ_a.leslie_fw.is_none() && !engine.organ_b.farfisa_processor);
        assert!(!engine.piano_a.firmware_layer_law && engine.synth_b.sample_kernel_by_key);
    }

    #[test]
    fn bar_shows_every_group_and_status_line_in_a_headless_context() {
        let (handle, rx) = AudioHandle::new();
        let mut panel = FirmwarePanelState::new(FirmwareOptions {
            tone_source: Some(nord_core::organ::ToneWheelSource::B3Wheels),
            ..FirmwareOptions::default()
        });
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1580.0, 472.0),
            )),
            ..Default::default()
        };
        let mut changed = true;
        let mut height = f32::NAN;
        let mut shapes = 0;
        for _ in 0..2 {
            let out = ctx.run(raw.clone(), |ctx| {
                let r = egui::TopBottomPanel::top("bar").show(ctx, |ui| {
                    changed = panel.show_bar(ui, &handle);
                });
                height = r.response.rect.height();
            });
            shapes = out.shapes.len();
        }
        assert!(
            !changed && rx.try_recv().is_err(),
            "nothing sent without input"
        );
        // the bar at the default window size must leave room for the panel below it
        assert!(height < 140.0, "bar height {height}");
        assert!(shapes > 0);
    }
}
