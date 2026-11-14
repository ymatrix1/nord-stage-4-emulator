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
/// Firmware Spring reverb (`--firmware-reverb`).
pub const REVERB: SwitchInfo = sw(
    "FX",
    "--firmware-reverb",
    "Firmware reverb",
    Mixed,
    "Firmware Spring reverb routine on every chain (Spring types only, 4 frames latency).",
);
/// Synth chain (`--synth-chain`).
pub const CHAIN: SwitchInfo = sw(
    "Synth",
    "--synth-chain",
    "Chain",
    Mixed,
    "Per-voice chain: classic engine or the firmware strip (partly VERIFIED, ADR/R laws open).",
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
/// Amp path (`--amp-path`).
pub const AMP: SwitchInfo = sw(
    "FX",
    "--amp-path",
    "Amp path",
    Verified,
    "Firmware drive stage (default) or the engine tanh drive and RBJ cabinet; 4 frames latency.",
);
/// Firmware FX ports (`--firmware-fx`).
pub const FX_PORTS: SwitchInfo = sw(
    "FX",
    "--firmware-fx",
    "Firmware FX",
    Mixed,
    "Every FirmwareFx port: delay laws, delay time, reverb mix, compressor (4 frames latency).",
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
pub const ALL: [SwitchInfo; 14] = [
    TONE,
    B3_TAIL,
    ROTARY,
    ORGAN_LEVEL,
    CHAIN,
    FILTER,
    MEASURED,
    SHAPE,
    PIANO_LEVEL,
    PIANO_BUS,
    AMP,
    FX_PORTS,
    REVERB,
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
