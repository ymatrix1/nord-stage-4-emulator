//! Pages of the Synth display (manual p.27-38) and what the three dials do on each.
//!
//! The red-framed function buttons choose the page; the three dials under the display edit the
//! page's parameters. `SynthUi::open` is the page state machine, [`dial_defs`] the dial table.

use nord_core::synth::SynthOscMode;

/// What the Synth OLED currently shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SynthPage {
    /// Waveform select: TYPE / CAT / WAVE dials (SAMPLES mode: BRIGHT / CAT / SAMPLE). Default view.
    #[default]
    Waveform,
    /// Filter type: TYPE / KBD TRK / DRIVE.
    FilterType,
    /// Oscillator envelope: ATTACK / DECAY / RELEASE.
    EnvOsc,
    /// Filter envelope.
    EnvFilter,
    /// Amp envelope.
    EnvAmp,
    /// LFO waveform select.
    LfoWave,
    /// Arpeggiator MENU page `1..=5` (manual p.36).
    Arp(u8),
    /// Vibrato MENU: RATE / AMOUNT / DLY TIME.
    Vibrato,
    /// PITCH/SMP: PITCH / FINE (and the Sample options in SAMPLES mode).
    PitchSmp,
    /// Extern mode: Program and Parameter changes on external gear (manual p.46).
    Extern,
}

/// Number of Arpeggiator MENU pages.
pub const ARP_PAGES: u8 = 5;

impl SynthPage {
    /// Inventory id of the button that selects this page (it gets the red frame).
    pub fn button_id(self) -> &'static str {
        match self {
            Self::Waveform => "synth.waveform",
            Self::FilterType => "synth.filter.type",
            Self::EnvOsc => "synth.osc.envelope",
            Self::EnvFilter => "synth.filter.envelope",
            Self::EnvAmp => "synth.amp.envelope",
            Self::LfoWave => "synth.lfo.waveform",
            Self::Arp(_) => "synth.arp.menu",
            Self::Vibrato => "synth.vibrato.menu",
            Self::PitchSmp => "synth.osc.pitch_smp",
            Self::Extern => "synth.mode",
        }
    }

    /// Header line of the page.
    pub fn title(self) -> &'static str {
        match self {
            Self::Waveform => "OSC WAVEFORM",
            Self::FilterType => "FILTER",
            Self::EnvOsc => "OSC ENVELOPE",
            Self::EnvFilter => "FILTER ENVELOPE",
            Self::EnvAmp => "AMP ENVELOPE",
            Self::LfoWave => "LFO WAVEFORM",
            Self::Arp(1) => "ARPEGGIATOR",
            Self::Arp(2) => "PATTERN",
            Self::Arp(3) => "PATTERN EDIT",
            Self::Arp(4) => "PATTERN ACCENT",
            Self::Arp(_) => "PATTERN PAN",
            Self::Vibrato => "VIBRATO",
            Self::PitchSmp => "OSC PITCH",
            Self::Extern => "EXTERN",
        }
    }
}

/// One dial of a page: its soft label and, when the engine cannot do it, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialDef {
    pub label: &'static str,
    /// `Some(reason)`: the dial keeps its value in the panel state but has no engine effect.
    pub unsupported: Option<&'static str>,
}

const fn d(label: &'static str) -> Option<DialDef> {
    Some(DialDef {
        label,
        unsupported: None,
    })
}

const fn dim(label: &'static str, why: &'static str) -> Option<DialDef> {
    Some(DialDef {
        label,
        unsupported: Some(why),
    })
}

/// Dial table: `[left, middle, right]`, `None` = the dial is unused on this page.
pub fn dial_defs(page: SynthPage, mode: SynthOscMode) -> [Option<DialDef>; 3] {
    match page {
        SynthPage::Waveform if mode == SynthOscMode::Sample => [d("BRIGHT"), d("CAT"), d("SAMPLE")],
        SynthPage::Waveform => [d("TYPE"), d("CAT"), d("WAVE")],
        SynthPage::FilterType => [d("TYPE"), d("KBD TRK"), d("DRIVE")],
        SynthPage::EnvOsc | SynthPage::EnvFilter | SynthPage::EnvAmp => {
            [d("ATTACK"), d("DECAY"), d("RELEASE")]
        }
        SynthPage::LfoWave => [d("WAVE"), None, None],
        SynthPage::Arp(1) => [d("PAGE"), d("DIRECTION"), d("ZIG ZAG")],
        SynthPage::Arp(2) => [d("PAGE"), d("PRESET"), d("LENGTH")],
        SynthPage::Arp(3) => [d("PAGE"), d("POSITION"), d("GATE")],
        SynthPage::Arp(4) => [d("PAGE"), d("POSITION"), d("ACCENT")],
        SynthPage::Arp(_) => [d("PAGE"), d("POSITION"), d("PAN")],
        SynthPage::Vibrato => [d("RATE"), d("AMOUNT"), d("DLY TIME")],
        SynthPage::PitchSmp if mode == SynthOscMode::Sample => [
            d("PITCH"),
            d("FINE"),
            dim(
                "SAMPLE OPT",
                "Natural / No Dyn / Fast Atk need the sample preset data, not in the engine",
            ),
        ],
        SynthPage::PitchSmp => [d("PITCH"), d("FINE"), None],
        SynthPage::Extern => [d("CC VAL 1"), d("CC VAL 2"), d("PROGRAM")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_maps_to_a_synth_button() {
        let pages = [
            SynthPage::Waveform,
            SynthPage::FilterType,
            SynthPage::EnvOsc,
            SynthPage::EnvFilter,
            SynthPage::EnvAmp,
            SynthPage::LfoWave,
            SynthPage::Arp(1),
            SynthPage::Vibrato,
            SynthPage::PitchSmp,
            SynthPage::Extern,
        ];
        for p in pages {
            assert!(p.button_id().starts_with("synth."), "{p:?}");
            assert!(!p.title().is_empty());
        }
    }

    #[test]
    fn waveform_dials_follow_the_oscillator_mode() {
        let a = dial_defs(SynthPage::Waveform, SynthOscMode::Analog);
        assert_eq!(a.map(|x| x.unwrap().label), ["TYPE", "CAT", "WAVE"]);
        let s = dial_defs(SynthPage::Waveform, SynthOscMode::Sample);
        assert_eq!(s.map(|x| x.unwrap().label), ["BRIGHT", "CAT", "SAMPLE"]);
        assert!(s[0].unwrap().unsupported.is_none());
        assert!(s.iter().flatten().all(|x| x.unsupported.is_none()));
    }

    #[test]
    fn envelope_pages_share_attack_decay_release() {
        for p in [SynthPage::EnvOsc, SynthPage::EnvFilter, SynthPage::EnvAmp] {
            let defs = dial_defs(p, SynthOscMode::Analog);
            assert_eq!(
                defs.map(|x| x.unwrap().label),
                ["ATTACK", "DECAY", "RELEASE"]
            );
        }
    }

    #[test]
    fn arp_pages_navigate_with_the_left_dial_and_every_pattern_dial_works() {
        for n in 1..=ARP_PAGES {
            let defs = dial_defs(SynthPage::Arp(n), SynthOscMode::Analog);
            assert_eq!(defs[0].unwrap().label, "PAGE");
        }
        for n in 1..=ARP_PAGES {
            let defs = dial_defs(SynthPage::Arp(n), SynthOscMode::Analog);
            for d in defs.iter().flatten() {
                assert!(d.unsupported.is_none(), "{n} {}", d.label);
            }
        }
        let p3 = dial_defs(SynthPage::Arp(3), SynthOscMode::Analog);
        assert_eq!(p3.map(|x| x.unwrap().label), ["PAGE", "POSITION", "GATE"]);
        let p5 = dial_defs(SynthPage::Arp(5), SynthOscMode::Analog);
        assert_eq!(p5[2].unwrap().label, "PAN");
    }

    #[test]
    fn extern_dials_follow_the_manual_and_lfo_uses_one_dial() {
        let e = dial_defs(SynthPage::Extern, SynthOscMode::Analog);
        assert_eq!(
            e.map(|x| x.unwrap().label),
            ["CC VAL 1", "CC VAL 2", "PROGRAM"]
        );
        assert!(e.iter().all(|x| x.unwrap().unsupported.is_none()));
        let l = dial_defs(SynthPage::LfoWave, SynthOscMode::Analog);
        assert!(l[1].is_none() && l[2].is_none());
    }
}
