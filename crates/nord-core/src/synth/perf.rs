//! Synth performance features: Gate (rhythmic gate), Pattern, KB Hold, KB Sync and
//! Master Clock sync of the Arpeggiator/Gate and LFO rates (manual p.35-37).
//!
//! Every field defaults to "off", so a layer that never touches `perf` renders exactly as before.

use super::*;

/// Number of preset gate patterns.
pub const PATTERN_PRESET_COUNT: usize = 8;

/// A gate pattern: `len` steps, each a gate length in eighths of a step (`0` = rest, `8` = full step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatePattern {
    pub len: u8,
    pub steps: [u8; 16],
}

const fn pat(len: u8, steps: [u8; 16]) -> GatePattern {
    GatePattern { len, steps }
}

/// ASSUMED: the factory pattern presets are not decoded from firmware. These eight are plausible
/// simple and syncopated rhythms (2..16 steps, as the manual's Length dial allows); confirm by
/// stepping the hardware Preset dial and noting each pattern's grid.
pub const PATTERN_PRESETS: [GatePattern; PATTERN_PRESET_COUNT] = [
    pat(4, [4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(4, [0, 4, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(4, [6, 2, 6, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(8, [4, 4, 0, 4, 4, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(4, [3, 3, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(8, [4, 0, 0, 4, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(2, [8, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    pat(16, [4, 0, 4, 4, 0, 4, 0, 4, 4, 0, 4, 0, 4, 4, 0, 0]),
];

/// Plain gate (Pattern off): a 50 % duty gate on every step.
/// ASSUMED: 50 % duty (4/8); the manual only says the level is gated at the Rate setting.
const PLAIN_GATE_EIGHTHS: u8 = 4;

/// Master Clock subdivision selected by the Rate knob when synced (manual p.37, "1/2 .. 1/32,
/// T = triplet").
///
/// ASSUMED: the exact list of positions (here 1/2, 1/4, 1/8, 1/16, 1/32 plus triplets); confirm
/// by turning the Rate knob with Master Clock sync on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockDivision {
    Half,
    HalfT,
    Quarter,
    QuarterT,
    #[default]
    Eighth,
    EighthT,
    Sixteenth,
    SixteenthT,
    ThirtySecond,
    ThirtySecondT,
}

impl ClockDivision {
    /// Maps a host index `0..=9` (clamped) to a division.
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => Self::Half,
            1 => Self::HalfT,
            2 => Self::Quarter,
            3 => Self::QuarterT,
            4 => Self::Eighth,
            5 => Self::EighthT,
            6 => Self::Sixteenth,
            7 => Self::SixteenthT,
            8 => Self::ThirtySecond,
            _ => Self::ThirtySecondT,
        }
    }

    /// Length in quarter-note beats (a triplet is 2/3 of the straight value).
    pub fn beats(self) -> f32 {
        match self {
            Self::Half => 2.0,
            Self::HalfT => 2.0 * 2.0 / 3.0,
            Self::Quarter => 1.0,
            Self::QuarterT => 2.0 / 3.0,
            Self::Eighth => 0.5,
            Self::EighthT => 1.0 / 3.0,
            Self::Sixteenth => 0.25,
            Self::SixteenthT => 1.0 / 6.0,
            Self::ThirtySecond => 0.125,
            Self::ThirtySecondT => 1.0 / 12.0,
        }
    }
}

/// Gate, hold, sync and Master Clock state of one Synth layer.
#[derive(Debug, Clone)]
pub struct PerfState {
    /// KB Hold: notes keep sounding after the keys are lifted.
    pub kb_hold: bool,
    /// Per-layer exclusion from KB Hold (`Shift + KB Hold`).
    pub kb_hold_exclude: bool,
    /// KB Sync: a new note after all keys were lifted resets the arpeggio / gate (and LFO).
    pub kb_sync: bool,
    /// Arpeggiator / Gate step length follows `arp_division` of `master_bpm`.
    pub arp_clock_sync: bool,
    pub arp_division: ClockDivision,
    /// LFO period follows `lfo_division` of `master_bpm`.
    pub lfo_clock_sync: bool,
    pub lfo_division: ClockDivision,
    /// Master Clock tempo in BPM; the engine copies it in every frame.
    pub master_bpm: f32,
    /// Gate envelope hardness `0..1` (`0` = soft, `1` = on/off). Driven by the Range knob in Gate mode.
    pub gate_env: f32,
    /// Pattern on: Arp / Poly / Gate steps follow `pattern_preset`.
    pub pattern_on: bool,
    pub pattern_preset: u8,
    /// Set when a KB Sync reset should also reset the Master Clock; the engine consumes it.
    pub clock_reset_pending: bool,
    pub(super) gate_counter: f32,
    pub(super) gate_step: usize,
    pub(super) gate_level: f32,
    pub(super) cur_len: u8,
    pub(super) phys_keys: [bool; 128],
    pub(super) hold_latched: bool,
}

impl Default for PerfState {
    fn default() -> Self {
        Self {
            kb_hold: false,
            kb_hold_exclude: false,
            kb_sync: false,
            arp_clock_sync: false,
            arp_division: ClockDivision::default(),
            lfo_clock_sync: false,
            lfo_division: ClockDivision::default(),
            master_bpm: 120.0,
            gate_env: 0.5,
            pattern_on: false,
            pattern_preset: 0,
            clock_reset_pending: false,
            gate_counter: 0.0,
            gate_step: 0,
            gate_level: 1.0,
            cur_len: PLAIN_GATE_EIGHTHS,
            phys_keys: [false; 128],
            hold_latched: false,
        }
    }
}

impl SynthLayer {
    /// True when the arpeggiator (not the Gate) consumes incoming notes.
    pub(super) fn arp_consumes_notes(&self) -> bool {
        !matches!(self.arp_mode, ArpMode::Off | ArpMode::Gate)
    }

    /// KB Hold applies to this layer.
    pub(super) fn hold_active(&self) -> bool {
        self.perf.kb_hold && !self.perf.kb_hold_exclude
    }

    /// Arp / Gate step length in seconds.
    pub(super) fn step_seconds(&self) -> f32 {
        if self.perf.arp_clock_sync {
            self.perf.arp_division.beats() * 60.0 / self.perf.master_bpm.clamp(30.0, 300.0)
        } else {
            // Arp speed: 16th note division at arp_rate_bpm
            60.0 / (self.arp_rate_bpm.max(30.0) * 4.0)
        }
    }

    /// Effective LFO rate: free `lfo_rate_hz`, or one cycle per `lfo_division` of the Master Clock.
    pub(super) fn lfo_hz(&self) -> f32 {
        if self.perf.lfo_clock_sync {
            self.perf.master_bpm.clamp(30.0, 300.0) / (60.0 * self.perf.lfo_division.beats())
        } else {
            self.lfo_rate_hz
        }
    }

    /// Takes the next pattern step (gate length in eighths) and advances the pattern.
    fn next_pattern_len(&mut self) -> u8 {
        let p = PATTERN_PRESETS[usize::from(self.perf.pattern_preset) % PATTERN_PRESET_COUNT];
        let idx = self.perf.gate_step % usize::from(p.len);
        self.perf.gate_step = (idx + 1) % usize::from(p.len);
        p.steps[idx]
    }

    /// Arp/Poly: advances the pattern at a step boundary; `true` when this step is a rest.
    pub(super) fn pattern_rest(&mut self) -> bool {
        if !self.perf.pattern_on {
            return false;
        }
        self.perf.cur_len = self.next_pattern_len();
        self.perf.cur_len == 0
    }

    /// Arp/Poly with a pattern: ends the note after the step's gate length.
    pub(super) fn pattern_release(&mut self, step_sec: f32) {
        if self.arp_sample_counter / step_sec >= f32::from(self.perf.cur_len) / 8.0 {
            self.release_arp_notes();
        }
    }

    /// Advances the rhythmic gate and its smoothed level (`ArpMode::Gate`).
    pub(super) fn tick_gate(&mut self, inv_fs: f32) {
        let step_sec = self.step_seconds();
        self.perf.gate_counter += inv_fs;
        if self.perf.gate_counter >= step_sec {
            self.perf.gate_counter -= step_sec;
            self.perf.cur_len = self.step_len();
        }
        let phase = self.perf.gate_counter / step_sec;
        let open = phase < f32::from(self.perf.cur_len) / 8.0;
        let target = if open { 1.0 } else { 0.0 };
        // ASSUMED: tau = 25 % of a step at ENV 0 down to 0.5 ms at ENV 1; the manual only says
        // "very soft" to "very hard". Confirm by scoping the gate edge on hardware.
        let env = self.perf.gate_env.clamp(0.0, 1.0);
        let tau = step_sec * 0.25 * (1.0 - env) + 0.0005;
        let coeff = 1.0 - (-inv_fs / tau).exp();
        self.perf.gate_level += (target - self.perf.gate_level) * coeff;
    }

    /// Gate length (eighths of a step) of the step that starts now.
    fn step_len(&mut self) -> u8 {
        if self.perf.pattern_on {
            self.next_pattern_len()
        } else {
            PLAIN_GATE_EIGHTHS
        }
    }

    /// Key press bookkeeping before the note is handled: KB Hold replacement and KB Sync reset.
    pub(super) fn on_key_down(&mut self, note_idx: usize) {
        let first = !self.perf.phys_keys.iter().any(|&k| k);
        if first {
            if self.perf.hold_latched {
                self.release_latched();
            }
            if self.perf.kb_sync {
                self.sync_reset();
            }
        }
        self.perf.phys_keys[note_idx] = true;
    }

    /// Releases every note that only KB Hold keeps alive.
    pub(super) fn release_latched(&mut self) {
        for n in 0..128usize {
            if self.held_keys[n] && !self.perf.phys_keys[n] {
                self.held_keys[n] = false;
                for v in &mut self.voices {
                    if v.active && v.note as usize == n {
                        v.gate = false;
                    }
                }
            }
        }
        self.release_arp_notes();
        self.perf.hold_latched = false;
    }

    /// Per-frame check: KB Hold was switched off while notes were latched.
    pub(super) fn update_hold(&mut self) {
        if self.perf.hold_latched && !self.hold_active() {
            self.release_latched();
        }
    }

    /// KB Sync: restart the arpeggio / gate (first step fires at once) and the synced LFO.
    fn sync_reset(&mut self) {
        self.arp_sample_counter = self.step_seconds();
        self.arp_step = 0;
        self.perf.gate_counter = 0.0;
        self.perf.gate_step = 0;
        self.perf.cur_len = self.step_len();
        if self.perf.lfo_clock_sync {
            self.lfo_phase = 0.0;
        }
        self.perf.clock_reset_pending = self.perf.arp_clock_sync || self.perf.lfo_clock_sync;
    }

    /// Takes the pending Master Clock reset request (called by the engine).
    pub fn take_clock_reset(&mut self) -> bool {
        std::mem::take(&mut self.perf.clock_reset_pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_lengths_in_beats() {
        assert_eq!(ClockDivision::Quarter.beats(), 1.0);
        assert_eq!(ClockDivision::Half.beats(), 2.0);
        assert_eq!(ClockDivision::Eighth.beats(), 0.5);
        assert!((ClockDivision::EighthT.beats() - 1.0 / 3.0).abs() < 1e-6);
        assert!((ClockDivision::ThirtySecondT.beats() - 1.0 / 12.0).abs() < 1e-6);
        assert_eq!(ClockDivision::from_index(0), ClockDivision::Half);
        assert_eq!(ClockDivision::from_index(99), ClockDivision::ThirtySecondT);
    }

    #[test]
    fn presets_are_well_formed() {
        for p in PATTERN_PRESETS {
            assert!((2..=16).contains(&p.len));
            assert!(p.steps.iter().all(|&s| s <= 8));
            assert!(p.steps[..usize::from(p.len)].iter().any(|&s| s > 0));
        }
    }

    #[test]
    fn defaults_are_off() {
        let p = PerfState::default();
        assert!(!p.kb_hold && !p.kb_sync && !p.arp_clock_sync && !p.lfo_clock_sync);
        assert!(!p.pattern_on);
    }
}
