//! The hint table: inventory control id -> label, value formatter and provenance.
//!
//! Provenance tags follow `.agents/rules/provenance-in-code.md`. A label is `VERIFIED` when the
//! same text sits in the firmware string pool (`bank0_CODE.bin`, OS v1.66, offsets below), checked
//! by `tools/tests/test_ns4_oled_strings.py::test_param_hint_labels_are_in_the_firmware`. A
//! formatter is `VERIFIED` only where the firmware fixes its unit string (`%.1f dB`, `%d bpm`).
//! The knob laws (log or linear sweep) are `ASSUMED` unless a row says otherwise: no measurement
//! of the hardware pots exists.

use super::fmt::Fmt;
use crate::panel::ui_state::{FREQ_MAX, FREQ_MIN, GLIDE_MAX, LFO_MAX, LFO_MIN};

/// EQ gain range in dB (VERIFIED in `nord-plugin` `fx_params.rs`: 7-bit signed x 0.25 dB).
pub const EQ_DB: (f32, f32) = (-16.0, 16.0);
/// EQ MID FREQ sweep (VERIFIED from manual p.51: 200 Hz to 8 kHz; the log law is ASSUMED).
pub const MID_HZ: (f32, f32) = (200.0, 8000.0);
/// Shortest and longest free delay time (VERIFIED, `FxPreset::delay_time_ms`: 20..1500 ms).
pub const DELAY_MS: (f32, f32) = (20.0, 1500.0);
/// Feedback limit of the delay knob (ASSUMED: the host range 0..0.95 of `FxPreset`).
pub const FEEDBACK_MAX: f32 = 0.95;
/// Arpeggiator rate range in bpm (ASSUMED: the previous panel's `20 + 280 n`).
pub const ARP_BPM: (f32, f32) = (20.0, 300.0);

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// Inventory control id (`nord-panel` `inventory`).
    pub id: &'static str,
    /// Hint label.
    pub label: &'static str,
    /// Value formatter.
    pub fmt: Fmt,
}

const fn e(id: &'static str, label: &'static str, fmt: Fmt) -> Entry {
    Entry { id, label, fmt }
}

/// Every control with a hint line.
///
/// Labels `VERIFIED` against the firmware pool (file offset in `bank0_CODE.bin`): `Oscillator
/// Ctrl` 0x2CD61C, `LFO Destination` 0x2CD654, `LFO Rate` 0x2CD664, `LFO Amount` 0x2CD684,
/// `Oscillator Env Amount` 0x2CD690, `Filter Env Amount` 0x2CD6B0, `Filter Frequency` 0x2CD640,
/// `Filter Resonance` 0x2CAE18, `Glide Rate` 0x2CD56C, `Arpeggiator Range` 0x2CD5D8,
/// `Arpeggiator Rate` 0x2CD5EC. The firmware does not print the remaining labels as such
/// (Effects and Master knobs): those are `ASSUMED`, built from the panel silkscreen.
///
/// `LFO Rate 0.08 Hz` and `LFO Amount 7.1` are EMPIRICAL: LFO course video 0:27-0:45
/// (`docs/research/dsoundman-course-findings.md`), one reading each.
pub const TABLE: &[Entry] = &[
    e("synth.osc.ctrl", "Oscillator Ctrl", Fmt::Ten),
    e(
        "synth.osc.env_amt",
        "Oscillator Env Amount",
        Fmt::TenBipolar,
    ),
    e(
        "synth.filter.freq",
        "Filter Frequency",
        Fmt::LogHz(FREQ_MIN, FREQ_MAX),
    ),
    e("synth.filter.res", "Filter Resonance", Fmt::Ten),
    e("synth.filter.env_amt", "Filter Env Amount", Fmt::TenBipolar),
    e("synth.lfo.dest", "LFO Destination", Fmt::Caller),
    e("synth.lfo.rate", "LFO Rate", Fmt::LogHz(LFO_MIN, LFO_MAX)),
    e("synth.lfo.mod_amt", "LFO Amount", Fmt::Ten),
    e(
        "synth.voice.glide",
        "Glide Rate",
        Fmt::TimeSquared(GLIDE_MAX),
    ),
    e(
        "synth.arp.rate",
        "Arpeggiator Rate",
        Fmt::Bpm(ARP_BPM.0, ARP_BPM.1),
    ),
    e(
        "synth.arp.range",
        "Arpeggiator Range",
        Fmt::Steps(1, 4, "oct"),
    ),
    e("mod1.rate", "Mod 1 Rate", Fmt::Caller),
    e("mod1.amount", "Mod 1 Amount", Fmt::Percent(100.0)),
    e("mod2.rate", "Mod 2 Rate", Fmt::Caller),
    e("mod2.amount", "Mod 2 Amount", Fmt::Percent(100.0)),
    e("delay.tempo", "Delay Tempo", Fmt::Caller),
    e(
        "delay.feedback",
        "Delay Feedback",
        Fmt::Percent(FEEDBACK_MAX * 100.0),
    ),
    e("delay.dry_wet", "Delay Dry/Wet", Fmt::Percent(100.0)),
    e("amp.drive", "Amp Drive", Fmt::Ten),
    e("amp.bass", "Amp Bass", Fmt::Db(EQ_DB.0, EQ_DB.1)),
    e("amp.mid", "Amp Mid", Fmt::Db(EQ_DB.0, EQ_DB.1)),
    e(
        "amp.mid_freq",
        "Amp Mid Freq",
        Fmt::LogHz(MID_HZ.0, MID_HZ.1),
    ),
    e("amp.treble", "Amp Treble", Fmt::Db(EQ_DB.0, EQ_DB.1)),
    e("amp.res", "Amp Filter Res", Fmt::Ten),
    e("comp.amount", "Comp Amount", Fmt::Percent(100.0)),
    e("reverb.dry_wet", "Reverb Dry/Wet", Fmt::Percent(100.0)),
    e("master.level", "Master Level", Fmt::Ten),
    e("rotary.drive", "Rotary Drive", Fmt::Ten),
];

/// Value text of the LFO DEST hint, in `LFO_DESTS` order (Filter, Pitch, Osc Ctrl, Off).
/// VERIFIED: `Filter` 0x2CD6A8, `Osc Pitch` 0x2CD704, `OscCtrl` 0x2CD6FC in the firmware pool,
/// checked by `test_param_hint_labels_are_in_the_firmware`; `Off` is ASSUMED (no `Off` string in
/// the pool; the video shows the destination dropping out). The LFO video (0:27-0:45) reads
/// `OscPitch` on a unit with an older OS (about 1.12), so the spelling differs by version.
pub const LFO_DEST_HINT: [&str; 4] = ["Filter", "Osc Pitch", "OscCtrl", "Off"];

/// Row of control `id`.
pub fn lookup(id: &str) -> Option<&'static Entry> {
    TABLE.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::inventory::all_controls;
    use nord_panel::types::ControlKind;

    #[test]
    fn every_row_is_a_knob_or_encoder_of_the_inventory() {
        let inv = all_controls();
        for r in TABLE {
            let c = inv.iter().find(|c| c.id == r.id);
            let c = c.unwrap_or_else(|| panic!("{} is not an inventory id", r.id));
            assert!(
                matches!(c.kind, ControlKind::Knob | ControlKind::Button),
                "{} is {:?}",
                r.id,
                c.kind
            );
        }
    }

    #[test]
    fn ids_are_unique_and_labels_have_no_vendor_name() {
        for (i, r) in TABLE.iter().enumerate() {
            assert!(TABLE[i + 1..].iter().all(|o| o.id != r.id), "{}", r.id);
            assert!(!r.label.to_lowercase().contains("clavia"), "{}", r.label);
        }
    }

    #[test]
    fn course_video_readings_are_reproduced() {
        // LFO video 0:27-0:45: `LFO Rate 0.08 Hz`, `LFO Amount 7.1`.
        let rate = lookup("synth.lfo.rate").expect("row");
        let n = (0.08_f32 / LFO_MIN).ln() / (LFO_MAX / LFO_MIN).ln();
        assert_eq!(rate.fmt.render(n).as_deref(), Some("0.08 Hz"));
        let amt = lookup("synth.lfo.mod_amt").expect("row");
        assert_eq!(amt.fmt.render(0.71).as_deref(), Some("7.1"));
    }

    #[test]
    fn ranges_match_their_firmware_unit_strings() {
        let bass = lookup("amp.bass").expect("row");
        assert_eq!(bass.fmt.render(0.75).as_deref(), Some("8.0 dB"));
        let rate = lookup("synth.arp.rate").expect("row");
        assert_eq!(rate.fmt.render(1.0).as_deref(), Some("300 bpm"));
    }
}
