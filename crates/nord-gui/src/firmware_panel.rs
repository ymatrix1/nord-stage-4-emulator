//! Firmware-path A/B switching for the standalone GUI.
//!
//! * CLI: `nord-stage4 --firmware-all | --legacy-all | --tone-source ... | --measured-laws ...`
//!   (same flags as `ns4 render-patch`, parsed by `FirmwareOptions::from_args`).
//! * In-app engine bar with a dropdown per engine path, sent as
//!   `AudioCommand::SetFirmwareOptions` and applied on the audio thread between blocks.
//! * Headless `--self-test <out.wav>`: builds the engine, applies the options and renders one
//!   second without opening a window.
//!
//! Docs: `docs/gui-testing.md`.

use crate::audio::{apply_audio_command, AudioCommand, AudioHandle};
use nord_core::engine::Stage4Engine;
use nord_core::organ::ToneWheelSource;
use nord_core::render;
use nord_core::render_opts::FirmwareOptions;
use nord_core::synth::{FilterEngine, SynthChain};
use std::path::{Path, PathBuf};

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
    format!("Clavia Nord Stage 4 [{}]", opts.summary())
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

    /// Engine-path bar shown above the panel: one dropdown per engine, presets and the active
    /// set. Changes are sent to the audio thread immediately. Returns true when options changed.
    pub fn show_bar(&mut self, ui: &mut egui::Ui, audio: &AudioHandle) -> bool {
        let before = self.opts;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Firmware preset").clicked() {
                self.opts = FirmwareOptions::firmware_all();
            }
            if ui.button("Legacy preset").clicked() {
                self.opts = FirmwareOptions::default();
            }
            let o = &mut self.opts;
            dropdown(ui, "Organ", "fw_tone", &mut o.tone_source, &TONES);
            if o.tone_source == Some(ToneWheelSource::B3Wheels) {
                dropdown(
                    ui,
                    "B3 tail",
                    "fw_tail",
                    &mut o.b3_tail,
                    &[(None, "On (default)"), (Some(false), "Off")],
                );
            }
            dropdown(ui, "Synth chain", "fw_chain", &mut o.synth_chain, &CHAINS);
            dropdown(
                ui,
                "Synth filter",
                "fw_filter",
                &mut o.filter_engine,
                &FILTERS,
            );
            ui.checkbox(&mut o.measured_laws, "Measured laws");
            dropdown(
                ui,
                "Piano level",
                "fw_piano",
                &mut o.piano_firmware_level,
                &[(false, "Legacy"), (true, "Firmware")],
            );
            dropdown(
                ui,
                "FX amp",
                "fw_fx",
                &mut o.firmware_fx,
                &[(false, "Engine"), (true, "Firmware")],
            );
            ui.label(format!("Engine: {}", self.opts.summary()));
        });
        let changed = before != self.opts;
        if changed {
            self.push(audio);
        }
        changed
    }
}

type Choice<T> = (T, &'static str);

const TONES: [Choice<Option<ToneWheelSource>>; 6] = [
    (None, "Model default"),
    (Some(ToneWheelSource::Legacy), "Legacy"),
    (Some(ToneWheelSource::Firmware), "Pipe (firmware)"),
    (Some(ToneWheelSource::B3Wheels), "B3 wheels (firmware)"),
    (Some(ToneWheelSource::Vox), "Vox bank (firmware)"),
    (Some(ToneWheelSource::FarfisaWheels), "Farfisa (firmware)"),
];
const CHAINS: [Choice<Option<SynthChain>>; 2] = [
    (None, "Classic"),
    (Some(SynthChain::FirmwareStrip), "Firmware strip"),
];
const FILTERS: [Choice<Option<FilterEngine>>; 2] = [
    (None, "ZDF (default)"),
    (Some(FilterEngine::Firmware), "Firmware"),
];

/// Labelled combo box over `choices` bound to `value`.
fn dropdown<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    value: &mut T,
    choices: &[Choice<T>],
) {
    ui.label(label);
    let shown = choices
        .iter()
        .find(|(v, _)| v == value)
        .map_or("?", |(_, n)| *n);
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, name) in choices {
                ui.selectable_value(value, *v, *name);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let names: Vec<_> = TONES.iter().map(|c| c.1).collect();
        let mut dedup = names.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(names.len(), dedup.len());
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
        panel.opts = FirmwareOptions::default();
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
}
