//! Switch catalogue of the firmware bar: flag, group, status and one-line meaning of every
//! `ns4 render-patch` firmware switch. Drives the tooltips and is checked against
//! `docs/gui-testing.md` so the docs table cannot drift from the bar.

/// Evidence status of a switch (`.agents/rules/provenance-in-code.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Matches firmware or an oracle.
    Verified,
    /// Fitted against recordings.
    Empirical,
    /// Chosen without evidence.
    Assumed,
    /// Mixed per choice or per port; see the meaning.
    Mixed,
}

impl Status {
    /// Upper-case tag shown in tooltips.
    pub fn tag(self) -> &'static str {
        match self {
            Status::Verified => "VERIFIED",
            Status::Empirical => "EMPIRICAL",
            Status::Assumed => "ASSUMED",
            Status::Mixed => "MIXED",
        }
    }
}

/// One switch of the bar.
#[derive(Debug, Clone, Copy)]
pub struct SwitchInfo {
    /// Bar group.
    pub group: &'static str,
    /// CLI flag (`ns4 render-patch` / `nord-stage4`).
    pub flag: &'static str,
    /// Label in the bar.
    pub label: &'static str,
    /// Evidence status.
    pub status: Status,
    /// One-line meaning.
    pub meaning: &'static str,
}

impl SwitchInfo {
    /// Tooltip text: status tag, flag and meaning.
    pub fn tooltip(&self) -> String {
        format!("[{}] {}\n{}", self.status.tag(), self.flag, self.meaning)
    }
}

const fn sw(
    group: &'static str,
    flag: &'static str,
    label: &'static str,
    status: Status,
    meaning: &'static str,
) -> SwitchInfo {
    SwitchInfo {
        group,
        flag,
        label,
        status,
        meaning,
    }
}

use Status::{Assumed, Empirical, Mixed, Verified};

/// Organ source (`--tone-source`).
pub const TONE: SwitchInfo = sw(
    "Organ",
    "--tone-source",
    "Source",
    Mixed,
    "Tone generator: firmware pipe / B3 ladder VERIFIED, Vox and Farfisa partial, legacy ASSUMED.",
);
/// B3 tail (`--b3-tail`).
pub const B3_TAIL: SwitchInfo = sw(
    "Organ",
    "--b3-tail",
    "B3 tail",
    Empirical,
    "B3 processor-tail frequency response of the B3 wheels source (B3 wheels only).",
);
/// Rotary drive law (`--rotary-drive`).
pub const ROTARY: SwitchInfo = sw(
    "Organ",
    "--rotary-drive",
    "Rotary drive",
    Empirical,
    "Engine tanh(g x), unity tanh(g x)/g, or scaled (preset unit 0..10 to engine 0..1).",
);
/// Organ output level law (`--organ-level`).
pub const ORGAN_LEVEL: SwitchInfo = sw(
    "Organ",
    "--organ-level",
    "Level",
    Mixed,
    "Organ output level: model default, legacy bus scale, or firmware swell word (docs/research/organ-output-law.md).",
);
/// Synth chain (`--synth-chain`).
pub const CHAIN: SwitchInfo = sw(
    "Synth",
    "--synth-chain",
    "Chain",
    Mixed,
    "Per-voice chain: firmware strip (default, D22: validated on hardware clips, see docs/research/synth-validation.md) or the classic engine chain.",
);
/// Stage oscillators (`--synth-stages` / `--no-synth-stages`).
pub const STAGES: SwitchInfo = sw(
    "Synth",
    "--synth-stages",
    "Stage oscillators",
    Mixed,
    "Bit-exact firmware stages (noise, Shape Square, SUPER, MULTI, SHAPE SINE) on the strip chain; on by default, off with --no-synth-stages.",
);
/// Envelope rates from the stored knobs (`--synth-knob-laws` / `--no-synth-knob-laws`).
pub const KNOB_LAWS: SwitchInfo = sw(
    "Synth",
    "--synth-knob-laws",
    "Knob envelope laws",
    Verified,
    "Strip envelope rates straight from the stored knobs (index = knob - 64, VERIFIED run, D55); on by default since D57, off with --no-synth-knob-laws.",
);
/// Oscillator P table (`--synth-p`).
pub const P_TABLE: SwitchInfo = sw(
    "Synth",
    "--synth-p",
    "P table",
    Empirical,
    "Per-wave level, cutoff factor and SHAPE word law fitted to hardware clips (default); off = unit P, fitted = the Smooth Lead block.",
);
/// Synth filter (`--filter-engine`).
pub const FILTER: SwitchInfo = sw(
    "Synth",
    "--filter-engine",
    "Filter",
    Verified,
    "ZDF engine filter or the firmware filter (checked against the SHARC oracle).",
);
/// Measured laws (`--measured-laws`).
pub const MEASURED: SwitchInfo = sw(
    "Synth",
    "--measured-laws",
    "Measured laws",
    Empirical,
    "16-voice Super Saw, envelope depth, decay, pitch and vibrato fitted to Audible Genius clips.",
);
/// Synth shape (`--synth-shape`).
pub const SHAPE: SwitchInfo = sw(
    "Synth",
    "--synth-shape",
    "Shape",
    Assumed,
    "Shape word of the firmware SHAPE oscillator (strip chain); off keeps the saw placeholder.",
);
/// Piano level (`--firmware-piano-level` / `--legacy-piano-level`).
pub const PIANO_LEVEL: SwitchInfo = sw(
    "Piano",
    "--firmware-piano-level",
    "Level",
    Verified,
    "Firmware velocity level law for .npno sounds; legacy is the touch curve.",
);
/// Piano SMP bus scale (`--firmware-piano-bus`).
pub const PIANO_BUS: SwitchInfo = sw(
    "Piano",
    "--firmware-piano-bus",
    "SMP bus scale",
    Assumed,
    "Sample path scaled by the SMP bus convention 0.5 instead of 0.55 (Q31 receiver ASSUMED).",
);
/// Piano receiver chain scale (`--firmware-piano-chain`).
pub const PIANO_CHAIN: SwitchInfo = sw(
    "Piano",
    "--firmware-piano-chain",
    "FX path scale",
    Mixed,
    "Sample path scaled by the verified FX receiver path 0.7464 (engine unit = DAC fraction ASSUMED).",
);
/// Amp path (`--amp-path`).
pub const AMP: SwitchInfo = sw(
    "FX",
    "--amp-path",
    "Amp path",
    Verified,
    "Firmware drive stage (default) or the engine tanh drive and RBJ cabinet; 4 frames latency.",
);
/// Farfisa processor (`--farfisa-processor`).
pub const FARFISA: SwitchInfo = sw(
    "Organ",
    "--farfisa-processor",
    "Farfisa processor",
    Assumed,
    "Key-down weights and measured line gain of the Farfisa processor 0x122B85 (Farfisa wheels source; partial).",
);
/// B3 Bass processor (`--b3-bass-processor`).
pub const B3_BASS: SwitchInfo = sw(
    "Organ",
    "--b3-bass-processor",
    "B3 Bass processor",
    Mixed,
    "B3 Bass key stage (generator weights, window words, measured line gain of 0x122296); needs B3 wheels on a B3 Bass layer, replaces the B3 tail.",
);
/// Firmware Leslie (`--leslie`).
pub const LESLIE: SwitchInfo = sw(
    "Organ",
    "--leslie",
    "Leslie",
    Mixed,
    "Firmware Leslie processor instead of the engine rotary: routine VERIFIED against the oracle, history ring EMPIRICAL.",
);
/// Leslie ring (`--leslie-ring`).
pub const LESLIE_RING: SwitchInfo = sw(
    "Organ",
    "--leslie-ring",
    "Ring",
    Empirical,
    "History ring pointers of the firmware Leslie: advance (fits the B3 clip), firmware (re-base every 8 calls), frozen (first oracle state). Needs Leslie = Firmware.",
);
/// Leslie gain (`--leslie-gain-db`).
pub const LESLIE_GAIN: SwitchInfo = sw(
    "Organ",
    "--leslie-gain-db",
    "Leslie gain",
    Assumed,
    "Gain after the firmware Leslie in dB (-24..+24, default 0). Needs Leslie = Firmware.",
);
/// Piano layer fader law (`--firmware-piano-layer-law`).
pub const PIANO_LAYER_LAW: SwitchInfo = sw(
    "Piano",
    "--firmware-piano-layer-law",
    "Layer law",
    Assumed,
    "Piano layer fader follows (v / 127)^2 instead of the engine curve (opt-in).",
);
/// Sample kernel (`--sample-kernel`).
pub const SAMPLE_KERNEL: SwitchInfo = sw(
    "Synth",
    "--sample-kernel",
    "Sample kernel",
    Mixed,
    "SAMPLES oscillator 12-tap kernel pick: firmware key rule (default; run VERIFIED, key field ASSUMED) or the ASSUMED ratio rule (--sample-kernel ratio).",
);
/// Sample pitch (`--sample-pitch`).
pub const SAMPLE_PITCH: SwitchInfo = sw(
    "Synth",
    "--sample-pitch",
    "Sample pitch",
    Mixed,
    "SAMPLES oscillator step: ARM exponent tables (law VERIFIED, root and rate ASSUMED) or exact 2^(n/12).",
);
/// Firmware FX port: delay laws.
pub const FX_DELAY_LAWS: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports delay-laws",
    "Delay laws",
    Mixed,
    "Delay send and dry gains follow kDlyMix (words VERIFIED, roles EMPIRICAL).",
);
/// Firmware FX port: delay time.
pub const FX_DELAY_TIME: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports delay-time",
    "Delay time",
    Verified,
    "Free-running delay time follows kDlyRateClkDiv (ARM harness and oracle).",
);
/// Firmware FX port: reverb mix.
pub const FX_REVERB_MIX: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports reverb-mix",
    "Reverb mix",
    Mixed,
    "Reverb send and dry gains follow kRvbAmt (words VERIFIED, role EMPIRICAL); the network stays the engine's.",
);
/// Firmware FX port: reverb (`--firmware-reverb`).
pub const REVERB: SwitchInfo = sw(
    "FX",
    "--firmware-reverb",
    "Reverb",
    Mixed,
    "Firmware reverb routines: Spring (types 0, 1) and multi-tap (types 2..11), 4 frames latency; does its own dry/wet mix.",
);
/// Firmware FX port: compressor.
pub const FX_COMPRESSOR: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports compressor",
    "Compressor",
    Mixed,
    "Firmware gain computer with the kCmpAmt words (algorithm and words VERIFIED, slope and log gain ASSUMED), 4 frames latency.",
);
/// Firmware FX port: Mod 1.
pub const FX_MOD1: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports mod1",
    "Mod 1",
    Mixed,
    "A-PAN and TREM from the firmware routines (VERIFIED, TREM role EMPIRICAL); free running only, 4 frames latency.",
);
/// Firmware FX port: Mod 2.
pub const FX_MOD2: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports mod2",
    "Mod 2",
    Mixed,
    "Firmware CHOR, PHAS, FLANG, ENS and SPIN stages with the kFx2 words (docs/research/fx-mod-types.md).",
);
/// Firmware FX port: Amp EQ.
pub const FX_EQ: SwitchInfo = sw(
    "FX",
    "--firmware-fx-ports eq",
    "Amp EQ",
    Mixed,
    "Firmware tone stack 0x1243F4 with the kAmp words (routine VERIFIED, joint mid words ASSUMED), 2 frames latency.",
);
/// All FX ports at once (`--firmware-fx`).
pub const FX_ALL: SwitchInfo = sw(
    "FX",
    "--firmware-fx",
    "All ports",
    Mixed,
    "Shortcut: every FirmwareFx port on or off (the amp has its own dropdown).",
);

/// Layer routing (`--firmware-routing`).
pub const ROUTING: SwitchInfo = sw(
    "Global",
    "--firmware-routing",
    "Firmware routing",
    Assumed,
    "Section/layer enables and per-layer KB zones as the program stores them (zone codes open).",
);

/// All switches in bar order.
pub const ALL: [SwitchInfo; 33] = [
    TONE,
    B3_TAIL,
    FARFISA,
    B3_BASS,
    ROTARY,
    ORGAN_LEVEL,
    LESLIE,
    LESLIE_RING,
    LESLIE_GAIN,
    PIANO_LEVEL,
    PIANO_BUS,
    PIANO_CHAIN,
    PIANO_LAYER_LAW,
    CHAIN,
    STAGES,
    KNOB_LAWS,
    P_TABLE,
    FILTER,
    MEASURED,
    SHAPE,
    SAMPLE_KERNEL,
    SAMPLE_PITCH,
    AMP,
    FX_ALL,
    FX_DELAY_LAWS,
    FX_DELAY_TIME,
    FX_REVERB_MIX,
    REVERB,
    FX_COMPRESSOR,
    FX_MOD1,
    FX_MOD2,
    FX_EQ,
    ROUTING,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_carry_status_flag_and_meaning() {
        for s in ALL {
            let t = s.tooltip();
            assert!(t.starts_with('[') && t.contains(s.flag) && t.contains(s.meaning));
            assert!(s.flag.starts_with("--") && !s.meaning.is_empty());
        }
        assert!(TONE.tooltip().contains("MIXED"));
        assert!(SHAPE.tooltip().contains("ASSUMED"));
    }

    #[test]
    fn docs_table_lists_every_switch_with_its_status() {
        let docs = include_str!("../../../../docs/gui-testing.md");
        for s in ALL {
            let row = docs
                .lines()
                .find(|l| l.starts_with('|') && l.contains(&format!("`{}", s.flag)))
                .unwrap_or_else(|| panic!("docs/gui-testing.md has no row for {}", s.flag));
            assert!(
                row.to_ascii_uppercase().contains(s.status.tag()),
                "{} row lacks status {}",
                s.flag,
                s.status.tag()
            );
        }
    }
}
