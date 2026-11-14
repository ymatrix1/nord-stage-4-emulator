//! Per-layer values of every Synth control, as the panel shows them.
//!
//! The panel cannot read the engine (the DSP thread owns it), so each layer keeps its own copy
//! of the control values. `emit_all` / `emit_osc` turn a copy into [`SynthParam`] writes; the
//! defaults equal `SynthLayer::new` (see the test), so an untouched panel leaves sounds alone.

use super::tables::{self, ANALOG, FM_ALGOS};
use crate::audio::SynthGroup;
use nord_core::synth::{SynthOscMode, VaWaveform, PATTERN_PRESETS};
use nord_core::synth_types::ArpMode;

/// Attack / Decay / Release times in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Env {
    pub attack: f32,
    pub decay: f32,
    pub release: f32,
}

impl Default for Env {
    /// Equals the engine's envelope defaults (`SynthLayer::new`).
    fn default() -> Self {
        Self {
            attack: 0.005,
            decay: 0.3,
            release: 0.25,
        }
    }
}

/// Time change of one envelope dial click.
/// ASSUMED: +6 % per click (log scale); the hardware dial law is not measured.
const ENV_STEP: f32 = 1.06;

/// Move an envelope time by `steps` dial clicks (log scale, 0.5 ms .. 60 s).
pub fn step_time(t: f32, steps: i32) -> f32 {
    (t * ENV_STEP.powi(steps)).clamp(0.0005, 60.0)
}

/// Every control value of one Synth layer.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthLayerUi {
    // Oscillators
    pub sample_mode: bool,
    /// Index into `WAVE_TYPES`.
    pub wave_type: u8,
    pub wave_cat: u8,
    pub wave_idx: u8,
    pub fm_algo: u8,
    pub osc_ctrl: f32,
    pub pitch_semi: i8,
    pub fine_cents: i8,
    pub sample_bright: bool,
    /// Natural / No Dyn / Fast Atk (no engine support; kept for the display).
    pub sample_dyn: u8,
    pub osc_env: Env,
    pub osc_env_amt: f32,
    pub env_to_pitch: bool,
    pub osc_vel: bool,
    // Filter and amp
    pub filter_type: u8,
    pub kb_track: u8,
    pub drive: u8,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub filter_env_amt: f32,
    pub filter_env: Env,
    pub filter_vel: bool,
    pub amp_env: Env,
    pub amp_vel: u8,
    // LFO
    pub lfo_shape: u8,
    pub lfo_dest: u8,
    pub lfo_rate_hz: f32,
    pub lfo_amt: f32,
    pub lfo_sync: bool,
    pub lfo_div: u8,
    // Voice
    pub voice: u8,
    pub priority: u8,
    pub glide: f32,
    pub unison: u8,
    // Vibrato
    pub vib_mode: u8,
    pub vib_rate: f32,
    /// Panel scale `0..=10`.
    pub vib_amount: f32,
    pub vib_delay: u8,
    // Arpeggiator / Gate
    pub arp_run: bool,
    pub arp_kind: u8,
    pub arp_dir: u8,
    pub zigzag: bool,
    /// Range knob in octaves `1..=4`, in steps of 1/3 octave (manual p.35).
    pub arp_range: f32,
    pub gate_env: f32,
    pub arp_rate: f32,
    pub arp_sync: bool,
    pub arp_div: u8,
    pub pattern_on: bool,
    pub pattern_preset: u8,
    /// Pattern length `2..=16` (the preset's, or the custom one once edited).
    pub pattern_len: u8,
    /// Cursor of the pattern edit pages (the step Gate / Accent / Pan edit).
    pub pattern_cursor: u8,
    /// The custom pattern plays instead of preset `pattern_preset` (set by the first edit).
    pub pattern_custom: bool,
    /// Gate length per step in eighths of a step, `0` = rest.
    pub step_gate: [u8; 16],
    pub step_accent: [bool; 16],
    /// Pan per step: 0 Left, 1 Center, 2 Right.
    pub step_pan: [u8; 16],
    pub kb_sync: bool,
    pub kb_hold_exclude: bool,
    // Strip
    pub octave: i8,
    pub kb_zone: u8,
}

impl Default for SynthLayerUi {
    fn default() -> Self {
        Self {
            sample_mode: false,
            wave_type: 0,
            wave_cat: 0,
            wave_idx: 2,
            fm_algo: 0,
            osc_ctrl: 0.3,
            pitch_semi: 0,
            fine_cents: 0,
            sample_bright: false,
            sample_dyn: 0,
            osc_env: Env::default(),
            osc_env_amt: 0.0,
            env_to_pitch: false,
            osc_vel: false,
            filter_type: 0,
            kb_track: 0,
            drive: 1,
            cutoff_hz: 2400.0,
            resonance: 0.3,
            filter_env_amt: 0.0,
            filter_env: Env::default(),
            filter_vel: false,
            amp_env: Env::default(),
            amp_vel: 3,
            lfo_shape: 0,
            lfo_dest: 0,
            lfo_rate_hz: 3.5,
            lfo_amt: 0.0,
            lfo_sync: false,
            lfo_div: 4,
            voice: 0,
            priority: 0,
            glide: 0.0,
            unison: 0,
            vib_mode: 0,
            vib_rate: 5.0,
            vib_amount: 0.0,
            vib_delay: 0,
            arp_run: false,
            arp_kind: 0,
            arp_dir: 0,
            zigzag: false,
            arp_range: 1.0,
            gate_env: 0.5,
            arp_rate: 120.0,
            arp_sync: false,
            arp_div: 4,
            pattern_on: false,
            pattern_preset: 0,
            pattern_len: 4,
            pattern_cursor: 0,
            pattern_custom: false,
            step_gate: PATTERN_PRESETS[0].steps,
            step_accent: [false; 16],
            step_pan: [1; 16],
            kb_sync: false,
            kb_hold_exclude: false,
            octave: 0,
            kb_zone: 0,
        }
    }
}

impl SynthLayerUi {
    /// Engine oscillator mode of the current selection.
    pub fn osc_mode(&self) -> SynthOscMode {
        if self.sample_mode {
            return SynthOscMode::Sample;
        }
        match self.wave_type {
            1 | 2 => SynthOscMode::Fm,
            3 => SynthOscMode::Wavetable,
            _ => SynthOscMode::Analog,
        }
    }

    /// Number of categories of the current TYPE.
    pub fn cat_count(&self) -> u8 {
        match self.wave_type {
            1 | 2 => FM_ALGOS.len() as u8,
            3 => tables::WAVE_CATS,
            _ => ANALOG.len() as u8,
        }
    }

    /// Number of waveforms in the current category (FM: partials or pitches).
    pub fn wave_count(&self) -> u8 {
        match self.wave_type {
            1 => tables::HARMONIC_COUNT,
            2 => tables::INHARMONIC_COUNT,
            3 => tables::TABLES_PER_CAT,
            _ => ANALOG[usize::from(self.wave_cat) % ANALOG.len()]
                .waves
                .len() as u8,
        }
    }

    /// Engine waveform of the ANALOG selection (`Saw` outside the ANALOG type).
    pub fn va_waveform(&self) -> VaWaveform {
        self.analog_entry().map_or(VaWaveform::Saw, |e| e.wave)
    }

    /// Selected ANALOG entry, `None` for the other TYPEs.
    pub fn analog_entry(&self) -> Option<tables::WaveEntry> {
        if self.wave_type != 0 {
            return None;
        }
        let cat = &ANALOG[usize::from(self.wave_cat) % ANALOG.len()];
        cat.waves.get(usize::from(self.wave_idx)).copied()
    }

    /// Category name shown on the middle dial.
    pub fn cat_name(&self) -> String {
        match self.wave_type {
            1 | 2 => FM_ALGOS[usize::from(self.fm_algo) % FM_ALGOS.len()].to_string(),
            3 => format!("TABLES {}", self.wave_cat + 1),
            _ => ANALOG[usize::from(self.wave_cat) % ANALOG.len()]
                .name
                .to_string(),
        }
    }

    /// Waveform name shown on the right dial (`~` marks a nearest-shape stand-in).
    pub fn wave_name(&self) -> String {
        match self.wave_type {
            1 => format!("P {}", tables::harmonic_partial(self.wave_idx)),
            2 => format!("{:+} st", tables::inharmonic_semitones(self.wave_idx)),
            3 => format!("Table {}", self.table_index() + 1),
            _ => self.analog_entry().map_or(String::new(), |e| {
                format!("{}{}", e.name, if e.exact { "" } else { " ~" })
            }),
        }
    }

    /// Wavetable index of the WAVE type selection.
    pub fn table_index(&self) -> u8 {
        (self.wave_cat * tables::TABLES_PER_CAT + self.wave_idx).min(63)
    }

    /// Pitch plus fine tune in cents.
    pub fn pitch_cents(&self) -> f32 {
        f32::from(self.pitch_semi) * 100.0 + f32::from(self.fine_cents)
    }

    /// Engine arpeggiator mode of the current RUN / mode / direction selection.
    pub fn arp_engine_mode(&self) -> ArpMode {
        if !self.arp_run {
            return ArpMode::Off;
        }
        match self.arp_kind {
            1 => ArpMode::Poly,
            2 => ArpMode::Gate,
            _ => match self.arp_dir {
                1 => ArpMode::Down,
                2 => ArpMode::UpDown,
                3 => ArpMode::Random,
                _ => ArpMode::Up,
            },
        }
    }

    /// GROUP: take the values of settings group `g` from `src`.
    pub fn copy_group_from(&mut self, src: &SynthLayerUi, g: SynthGroup) {
        match g {
            SynthGroup::Filter => {
                self.filter_type = src.filter_type;
                self.kb_track = src.kb_track;
                self.drive = src.drive;
                self.cutoff_hz = src.cutoff_hz;
                self.resonance = src.resonance;
                self.filter_env_amt = src.filter_env_amt;
                self.filter_env = src.filter_env;
                self.filter_vel = src.filter_vel;
            }
            SynthGroup::Lfo => {
                self.lfo_shape = src.lfo_shape;
                self.lfo_dest = src.lfo_dest;
                self.lfo_rate_hz = src.lfo_rate_hz;
                self.lfo_amt = src.lfo_amt;
                self.lfo_sync = src.lfo_sync;
                self.lfo_div = src.lfo_div;
            }
            SynthGroup::Arp => {
                self.arp_run = src.arp_run;
                self.arp_kind = src.arp_kind;
                self.arp_dir = src.arp_dir;
                self.arp_range = src.arp_range;
                self.gate_env = src.gate_env;
                self.arp_rate = src.arp_rate;
                self.arp_sync = src.arp_sync;
                self.arp_div = src.arp_div;
                self.pattern_on = src.pattern_on;
                self.pattern_preset = src.pattern_preset;
                self.kb_sync = src.kb_sync;
                self.zigzag = src.zigzag;
                self.pattern_len = src.pattern_len;
                self.pattern_custom = src.pattern_custom;
                self.step_gate = src.step_gate;
                self.step_accent = src.step_accent;
                self.step_pan = src.step_pan;
            }
        }
    }

    /// Drive step (`0..=3`) as the engine's `0..=1` value.
    pub fn drive_value(&self) -> f32 {
        f32::from(self.drive.min(3)) / 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_follows_type_and_sample_flag() {
        let mut u = SynthLayerUi::default();
        assert_eq!(u.osc_mode(), SynthOscMode::Analog);
        u.wave_type = 1;
        assert_eq!(u.osc_mode(), SynthOscMode::Fm);
        u.wave_type = 3;
        assert_eq!(u.osc_mode(), SynthOscMode::Wavetable);
        u.sample_mode = true;
        assert_eq!(u.osc_mode(), SynthOscMode::Sample);
    }

    #[test]
    fn arp_engine_mode_combines_run_kind_and_direction() {
        let mut u = SynthLayerUi::default();
        assert_eq!(u.arp_engine_mode(), ArpMode::Off);
        u.arp_run = true;
        assert_eq!(u.arp_engine_mode(), ArpMode::Up);
        u.arp_dir = 3;
        assert_eq!(u.arp_engine_mode(), ArpMode::Random);
        u.arp_kind = 2;
        assert_eq!(u.arp_engine_mode(), ArpMode::Gate);
        u.arp_kind = 1;
        assert_eq!(u.arp_engine_mode(), ArpMode::Poly);
    }

    #[test]
    fn envelope_steps_are_logarithmic_and_clamped() {
        assert!(step_time(0.1, 1) > 0.1);
        assert!(step_time(0.1, -1) < 0.1);
        assert_eq!(step_time(0.1, 10_000), 60.0);
        assert_eq!(step_time(0.1, -10_000), 0.0005);
    }

    #[test]
    fn names_mark_standins_and_report_fm_values() {
        let mut u = SynthLayerUi {
            wave_cat: 6,
            wave_idx: 0,
            ..SynthLayerUi::default()
        };
        assert_eq!(u.wave_name(), "Pink Noise ~");
        u.wave_type = 1;
        u.wave_idx = 1;
        assert_eq!(u.wave_name(), "P 1");
        u.wave_type = 2;
        u.wave_idx = 12;
        assert_eq!(u.wave_name(), "+0 st");
        u.wave_type = 3;
        u.wave_cat = 2;
        u.wave_idx = 3;
        assert_eq!(u.table_index(), 19);
    }
}
