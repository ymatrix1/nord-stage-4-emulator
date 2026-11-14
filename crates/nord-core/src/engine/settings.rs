//! Global (non-program) menu settings the engine honours: System, Sound and Output menus
//! (manual OS v1.6x p.57-59). Spec: `docs/specs/midi-and-control.md` section 5.
//!
//! Every default equals the value the engine hard-coded before these settings existed, so an
//! engine that never calls [`Stage4Engine::set_global_settings`] and one that calls it with
//! `GlobalSettings::default()` render bit-identically (`tests::defaults_render_bit_identically`).
//! The setter is idempotent and allocation free, so a host may call it every block.
//!
//! Provenance of the numeric choices: the manual names the entries and their choices (`VERIFIED`
//! text on p.57-58) but never the resulting gains, speeds or ramp times. Those factors are
//! `ASSUMED`; the dB ranges and defaults are `VERIFIED` against the manual text.

use super::*;
use crate::organ::OrganGlobals;

/// Fine Tune range in cents (manual p.57: +/- 50 cents).
pub const FINE_TUNE_RANGE_CENTS: f32 = 50.0;
/// Global Transpose range in semitones (manual p.57: +/- 6).
pub const GLOBAL_TRANSPOSE_RANGE: i8 = 6;
/// Piano Pedal Noise / String Res level range in dB (manual p.58: +/- 6 dB).
pub const PIANO_LEVEL_DB_RANGE: f32 = 6.0;

/// Three-step Low / Normal / High choice used by the Rotary menu entries (manual p.58).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level3 {
    Low,
    #[default]
    Normal,
    High,
}

impl Level3 {
    /// Rotation speed factor. `ASSUMED`: +-15 %; confirm by measuring the horn rate on hardware.
    pub fn speed_scale(self) -> f32 {
        match self {
            Level3::Low => 0.85,
            Level3::Normal => 1.0,
            Level3::High => 1.15,
        }
    }

    /// Time-constant factor of the speed ramp (`High` accelerates faster, so a shorter constant).
    /// `ASSUMED`: 1.5x / 1x / 0.67x; confirm by timing a Slow-to-Fast transition on hardware.
    pub fn tau_scale(self) -> f32 {
        match self {
            Level3::Low => 1.5,
            Level3::Normal => 1.0,
            Level3::High => 0.67,
        }
    }
}

/// B3 Organ Tonewheel Mode (manual p.58: level of tonewheel crosstalk and cable leakage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TonewheelMode {
    Clean,
    #[default]
    Vintage1,
    Vintage2,
}

impl TonewheelMode {
    /// Factor on the layer's `crosstalk_level` (`1.0` = the existing nominal -42 dB bleed).
    /// `ASSUMED`: Clean removes the bleed, Vintage 2 is 1.5x; confirm with a spectrum of a single
    /// B3 key on hardware.
    pub fn crosstalk_scale(self) -> f32 {
        match self {
            TonewheelMode::Clean => 0.0,
            TonewheelMode::Vintage1 => 1.0,
            TonewheelMode::Vintage2 => 1.5,
        }
    }
}

/// Global menu settings. `Default` reproduces the pre-menu engine exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalSettings {
    /// System 2 Global Transpose in semitones (+/-6), added to the per-program transpose.
    pub transpose: i8,
    /// System 3 Fine Tune in cents (+/-50), applied to Organ, Piano and Synth pitch.
    pub fine_tune_cents: f32,
    /// Sound 2 Piano Pedal Noise Level in dB (+/-6).
    pub piano_pedal_noise_db: f32,
    /// Sound 3 Piano String Res Level in dB (+/-6).
    pub piano_string_res_db: f32,
    /// Sound 5 B3 Organ Tonewheel Mode.
    pub tonewheel_mode: TonewheelMode,
    /// Sound 6 B3 Organ Click Level: `false` Normal (default), `true` High.
    pub organ_click_high: bool,
    /// Sound 7 Rotary Bass/Horn Level: bass share in percent (70, 60, 50, 40 or 30).
    pub rotary_bass_percent: u8,
    /// Sound 8 Rotary Rotor Speed.
    pub rotor_speed: Level3,
    /// Sound 9 Rotary Rotor Acceleration.
    pub rotor_acc: Level3,
    /// Sound 10 Rotary Horn Speed.
    pub horn_speed: Level3,
    /// Sound 11 Rotary Horn Acceleration.
    pub horn_acc: Level3,
    /// Output Mono: both output channels carry the same `(L + R) / 2` signal (manual p.59 "Mono
    /// Output"; the hardware does this by routing a bus to a single output).
    pub mono_output: bool,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            transpose: 0,
            fine_tune_cents: 0.0,
            piano_pedal_noise_db: 0.0,
            piano_string_res_db: 0.0,
            tonewheel_mode: TonewheelMode::default(),
            organ_click_high: false,
            rotary_bass_percent: 50,
            rotor_speed: Level3::Normal,
            rotor_acc: Level3::Normal,
            horn_speed: Level3::Normal,
            horn_acc: Level3::Normal,
            mono_output: false,
        }
    }
}

impl GlobalSettings {
    /// Clamps every field to its documented range (non-finite floats become the default).
    pub fn sanitized(mut self) -> Self {
        let finite = |x: f32| if x.is_finite() { x } else { 0.0 };
        self.transpose = self
            .transpose
            .clamp(-GLOBAL_TRANSPOSE_RANGE, GLOBAL_TRANSPOSE_RANGE);
        self.fine_tune_cents =
            finite(self.fine_tune_cents).clamp(-FINE_TUNE_RANGE_CENTS, FINE_TUNE_RANGE_CENTS);
        self.piano_pedal_noise_db =
            finite(self.piano_pedal_noise_db).clamp(-PIANO_LEVEL_DB_RANGE, PIANO_LEVEL_DB_RANGE);
        self.piano_string_res_db =
            finite(self.piano_string_res_db).clamp(-PIANO_LEVEL_DB_RANGE, PIANO_LEVEL_DB_RANGE);
        self.rotary_bass_percent = self.rotary_bass_percent.clamp(30, 70);
        self
    }

    fn organ_globals(&self) -> OrganGlobals {
        let bass = f32::from(self.rotary_bass_percent) / 100.0;
        OrganGlobals {
            crosstalk_scale: self.tonewheel_mode.crosstalk_scale(),
            // ASSUMED: High doubles the click excitation (+6 dB).
            click_scale: if self.organ_click_high { 2.0 } else { 1.0 },
            master_tune_cents: self.fine_tune_cents,
            rotor_speed_scale: self.rotor_speed.speed_scale(),
            horn_speed_scale: self.horn_speed.speed_scale(),
            rotor_tau_scale: self.rotor_acc.tau_scale(),
            horn_tau_scale: self.horn_acc.tau_scale(),
            // 2 * share keeps 50/50 at exactly 1.0 (the previous, unweighted split).
            bass_gain: 2.0 * bass,
            horn_gain: 2.0 * (1.0 - bass),
        }
    }
}

/// Linear gain of a dB value; exactly `1.0` at 0 dB.
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

impl Stage4Engine {
    /// Applies the global menu settings to every layer. Idempotent and allocation free.
    pub fn set_global_settings(&mut self, settings: GlobalSettings) {
        let s = settings.sanitized();
        self.settings = s;
        let og = s.organ_globals();
        self.organ_a.globals = og;
        self.organ_b.globals = og;
        self.rotary_bus.globals = og;
        let noise = db_gain(s.piano_pedal_noise_db);
        let res = db_gain(s.piano_string_res_db);
        for piano in [&mut self.piano_a, &mut self.piano_b] {
            piano.pedal_noise_gain = noise;
            piano.string_res_gain = res;
            piano.master_tune_cents = s.fine_tune_cents;
        }
        for synth in [&mut self.synth_a, &mut self.synth_b, &mut self.synth_c] {
            synth.master_tune_cents = s.fine_tune_cents;
        }
    }

    /// The settings last applied with [`Self::set_global_settings`].
    pub fn global_settings(&self) -> GlobalSettings {
        self.settings
    }

    /// Per-program plus global transpose in semitones.
    pub(super) fn effective_transpose(&self) -> i8 {
        self.transpose + self.settings.transpose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(e: &mut Stage4Engine, frames: usize) -> Vec<[f32; 2]> {
        for n in [48u8, 60, 67] {
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: n,
                velocity: 100,
            });
        }
        (0..frames).map(|_| e.render_frame()).collect()
    }

    #[test]
    fn defaults_render_bit_identically() {
        let mut a = Stage4Engine::new(44_100.0);
        let mut b = Stage4Engine::new(44_100.0);
        b.set_global_settings(GlobalSettings::default());
        assert_eq!(play(&mut a, 4096), play(&mut b, 4096));
        assert_eq!(b.global_settings(), GlobalSettings::default());
    }

    #[test]
    fn defaults_render_bit_identically_with_rotary_bus_and_piano_pedal() {
        let run = |apply: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if apply {
                e.set_global_settings(GlobalSettings::default());
            }
            e.fx_chains[2].amp_cabinet = AmpCabinet::ToRotary;
            e.handle_midi(MidiEvent::ControlChange {
                channel: 0,
                controller: 64,
                value: 127,
            });
            play(&mut e, 4096)
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn sanitize_clamps_and_rejects_nan() {
        let s = GlobalSettings {
            transpose: 99,
            fine_tune_cents: f32::NAN,
            piano_pedal_noise_db: 40.0,
            piano_string_res_db: -40.0,
            rotary_bass_percent: 5,
            ..GlobalSettings::default()
        }
        .sanitized();
        assert_eq!(s.transpose, 6);
        assert_eq!(s.fine_tune_cents, 0.0);
        assert_eq!(s.piano_pedal_noise_db, 6.0);
        assert_eq!(s.piano_string_res_db, -6.0);
        assert_eq!(s.rotary_bass_percent, 30);
    }

    #[test]
    fn global_transpose_adds_to_program_transpose_and_note_off_matches() {
        let mut shifted = Stage4Engine::new(44_100.0);
        shifted.set_global_settings(GlobalSettings {
            transpose: 3,
            ..GlobalSettings::default()
        });
        shifted.set_transpose(2);
        let mut reference = Stage4Engine::new(44_100.0);
        reference.set_transpose(5);
        assert_eq!(play(&mut shifted, 1024), play(&mut reference, 1024));
        // changing the setting while keys are held must not strand them: note-off still
        // releases the pitch the key started with
        shifted.set_global_settings(GlobalSettings::default());
        shifted.set_transpose(5);
        for n in [48u8, 60, 67] {
            for e in [&mut shifted, &mut reference] {
                e.handle_midi(MidiEvent::NoteOff {
                    channel: 0,
                    note: n,
                    velocity: 0,
                });
            }
        }
        let tail = |e: &mut Stage4Engine| (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>();
        assert_eq!(tail(&mut shifted), tail(&mut reference));
    }

    #[test]
    fn mono_output_duplicates_the_mid_signal() {
        let mut e = Stage4Engine::new(44_100.0);
        e.set_global_settings(GlobalSettings {
            mono_output: true,
            ..GlobalSettings::default()
        });
        e.fx_chains[0].amp_cabinet = AmpCabinet::ToRotary;
        let out = play(&mut e, 2048);
        assert!(out.iter().any(|[l, _]| l.abs() > 1e-6));
        assert!(out.iter().all(|[l, r]| l == r));
    }

    /// Energy of the render difference between click on and click off (isolates the click).
    fn click_energy(settings: GlobalSettings) -> f32 {
        let render = |click: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(settings);
            e.organ_a.rotary_enabled = false;
            e.organ_a.key_click_enabled = click;
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        let (on, off) = (render(true), render(false));
        on.iter()
            .zip(&off)
            .map(|(a, b)| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
            .sum()
    }

    #[test]
    fn click_level_high_doubles_the_click_amplitude() {
        let normal = click_energy(GlobalSettings::default());
        let high = click_energy(GlobalSettings {
            organ_click_high: true,
            ..GlobalSettings::default()
        });
        assert!(normal > 0.0);
        let ratio = high / normal;
        assert!((3.5..4.5).contains(&ratio), "energy ratio {ratio}, want ~4");
    }

    #[test]
    fn tonewheel_mode_scales_crosstalk_on_both_organ_layers() {
        for (mode, want) in [
            (TonewheelMode::Clean, 0.0),
            (TonewheelMode::Vintage1, 1.0),
            (TonewheelMode::Vintage2, 1.5),
        ] {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(GlobalSettings {
                tonewheel_mode: mode,
                ..GlobalSettings::default()
            });
            assert_eq!(e.organ_a.globals.crosstalk_scale, want);
            assert_eq!(e.organ_b.globals.crosstalk_scale, want);
        }
        // Clean differs from Vintage 1 in the rendered B3 chord.
        let render = |mode| {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(GlobalSettings {
                tonewheel_mode: mode,
                ..GlobalSettings::default()
            });
            play(&mut e, 2048)
        };
        assert_ne!(
            render(TonewheelMode::Clean),
            render(TonewheelMode::Vintage1)
        );
    }

    #[test]
    fn pedal_noise_level_scales_the_pedal_thump() {
        let thump = |db: f32| {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(GlobalSettings {
                piano_pedal_noise_db: db,
                ..GlobalSettings::default()
            });
            e.handle_midi(MidiEvent::ControlChange {
                channel: 0,
                controller: 64,
                value: 127,
            });
            let out: Vec<[f32; 2]> = (0..2048).map(|_| e.render_frame()).collect();
            out.iter().map(|[l, r]| l * l + r * r).sum::<f32>()
        };
        let (quiet, loud) = (thump(-6.0), thump(6.0));
        assert!(quiet > 0.0, "pedal noise must be audible at -6 dB");
        let ratio = loud / quiet;
        assert!(
            (ratio - 10f32.powf(12.0 / 10.0)).abs() / ratio < 0.05,
            "{ratio}"
        );
    }

    #[test]
    fn piano_levels_scale_in_db() {
        let mut e = Stage4Engine::new(44_100.0);
        e.set_global_settings(GlobalSettings {
            piano_pedal_noise_db: 6.0,
            piano_string_res_db: -6.0,
            ..GlobalSettings::default()
        });
        assert!((e.piano_a.pedal_noise_gain - 1.9953).abs() < 1e-3);
        assert!((e.piano_b.string_res_gain - 0.5012).abs() < 1e-3);
        assert_eq!(db_gain(0.0), 1.0);
    }

    /// Goertzel power of `x` at `freq` Hz.
    fn power_at(x: &[f32], freq: f32, fs: f32) -> f32 {
        let w = std::f32::consts::TAU * freq / fs;
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (n, v) in x.iter().enumerate() {
            re += v * (w * n as f32).cos();
            im += v * (w * n as f32).sin();
        }
        re * re + im * im
    }

    #[test]
    fn fine_tune_reaches_every_engine_and_moves_the_organ_pitch() {
        let mut e = Stage4Engine::new(44_100.0);
        e.set_global_settings(GlobalSettings {
            fine_tune_cents: -50.0,
            ..GlobalSettings::default()
        });
        assert_eq!(e.organ_a.globals.master_tune_cents, -50.0);
        assert_eq!(e.piano_b.master_tune_cents, -50.0);
        assert_eq!(e.synth_c.master_tune_cents, -50.0);
        let render = |cents: f32| {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(GlobalSettings {
                fine_tune_cents: cents,
                ..GlobalSettings::default()
            });
            e.organ_a.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
            e.organ_a.percussion_enabled = false;
            e.organ_a.rotary_enabled = false;
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            (0..44_100)
                .map(|_| e.render_frame()[0])
                .collect::<Vec<f32>>()
        };
        let c4 = 261.626f32;
        let flat_hz = c4 * (-50.0f32 / 1200.0).exp2();
        let sharp_hz = c4 * (50.0f32 / 1200.0).exp2();
        let flat = render(-50.0);
        let sharp = render(50.0);
        assert!(power_at(&flat, flat_hz, 44_100.0) > power_at(&flat, sharp_hz, 44_100.0));
        assert!(power_at(&sharp, sharp_hz, 44_100.0) > power_at(&sharp, flat_hz, 44_100.0));
    }

    #[test]
    fn fine_tune_moves_synth_pitch() {
        let render = |cents: f32| {
            let mut e = Stage4Engine::new(44_100.0);
            e.set_global_settings(GlobalSettings {
                fine_tune_cents: cents,
                ..GlobalSettings::default()
            });
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 69,
                velocity: 100,
            });
            (0..8192).map(|_| e.render_frame()[0]).collect::<Vec<f32>>()
        };
        assert_ne!(render(50.0), render(0.0));
    }

    #[test]
    fn rotary_choices_change_speed_and_balance() {
        assert!(Level3::High.speed_scale() > Level3::Low.speed_scale());
        assert!(Level3::High.tau_scale() < Level3::Low.tau_scale());
        let mut e = Stage4Engine::new(44_100.0);
        e.set_global_settings(GlobalSettings {
            rotary_bass_percent: 70,
            horn_speed: Level3::High,
            rotor_acc: Level3::Low,
            ..GlobalSettings::default()
        });
        let g = e.organ_a.globals;
        assert!((g.bass_gain - 1.4).abs() < 1e-6 && (g.horn_gain - 0.6).abs() < 1e-6);
        assert_eq!(g.horn_speed_scale, 1.15);
        assert_eq!(g.rotor_tau_scale, 1.5);
        assert_eq!(e.rotary_bus.globals, g);
    }
}
