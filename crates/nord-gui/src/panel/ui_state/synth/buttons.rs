//! Synth buttons: primary and SHIFT / hold functions (manual p.12, p.27-37, p.44).
//!
//! The widget layer turns a click into a gesture with `nord_panel::behaviour::ButtonTracker`,
//! resolves it with `behaviour::resolve` and calls [`SynthUi::press`].

use super::SynthUi;

/// Buttons that run through [`SynthUi::press`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    On,
    Mode,
    Waveform,
    PitchSmp,
    OscEnvelope,
    FilterType,
    FilterEnvelope,
    AmpEnvelope,
    LfoWaveform,
    LfoDest,
    VoiceMode,
    Unison,
    VibratoSel,
    VibratoMenu,
    ArpRun,
    ArpMode,
    ArpMenu,
    KbHold,
    OctaveDown,
    OctaveUp,
}

impl Btn {
    pub const ALL: [Btn; 20] = [
        Btn::On,
        Btn::Mode,
        Btn::Waveform,
        Btn::PitchSmp,
        Btn::OscEnvelope,
        Btn::FilterType,
        Btn::FilterEnvelope,
        Btn::AmpEnvelope,
        Btn::LfoWaveform,
        Btn::LfoDest,
        Btn::VoiceMode,
        Btn::Unison,
        Btn::VibratoSel,
        Btn::VibratoMenu,
        Btn::ArpRun,
        Btn::ArpMode,
        Btn::ArpMenu,
        Btn::KbHold,
        Btn::OctaveDown,
        Btn::OctaveUp,
    ];

    /// Inventory id of the button.
    pub fn id(self) -> &'static str {
        match self {
            Self::On => "synth.on",
            Self::Mode => "synth.mode",
            Self::Waveform => "synth.waveform",
            Self::PitchSmp => "synth.osc.pitch_smp",
            Self::OscEnvelope => "synth.osc.envelope",
            Self::FilterType => "synth.filter.type",
            Self::FilterEnvelope => "synth.filter.envelope",
            Self::AmpEnvelope => "synth.amp.envelope",
            Self::LfoWaveform => "synth.lfo.waveform",
            Self::LfoDest => "synth.lfo.dest",
            Self::VoiceMode => "synth.voice.mode",
            Self::Unison => "synth.unison",
            Self::VibratoSel => "synth.vibrato.selector",
            Self::VibratoMenu => "synth.vibrato.menu",
            Self::ArpRun => "synth.arp.run",
            Self::ArpMode => "synth.arp.mode",
            Self::ArpMenu => "synth.arp.menu",
            Self::KbHold => "synth.kb_hold",
            Self::OctaveDown => "synth.octave_down",
            Self::OctaveUp => "synth.octave_up",
        }
    }

    /// Inventory id of the function that shares this button's rectangle (SHIFT, hold or the
    /// second function of one physical button), with its legend.
    pub fn alias(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::On => ("synth.solo", "SOLO"),
            Self::Mode => ("synth.extern", "EXTERN"),
            Self::Waveform => ("synth.sound_init", "SOUND INIT"),
            Self::PitchSmp => ("synth.osc.env_to_pitch", "ENV TO PITCH"),
            Self::OscEnvelope => ("synth.osc.velocity", "VELOCITY"),
            Self::FilterType => ("synth.filter.group", "GROUP"),
            Self::FilterEnvelope => ("synth.filter.velocity", "VELOCITY"),
            Self::AmpEnvelope => ("synth.amp.velocity", "VELOCITY"),
            Self::LfoWaveform => ("synth.lfo.group", "GROUP"),
            Self::VoiceMode => ("synth.voice.priority", "LO/HI"),
            Self::ArpRun => ("synth.arp.kb_sync", "KB SYNC"),
            Self::ArpMode => ("synth.arp.pattern", "PATTERN"),
            Self::ArpMenu => ("synth.arp.group", "GROUP"),
            Self::KbHold => ("synth.kb_hold_exclude", "EXCLUDE"),
            Self::OctaveDown => ("synth.kb_zone_down", "KB ZONE"),
            Self::OctaveUp => ("synth.kb_zone_up", "KB ZONE"),
            _ => return None,
        })
    }

    /// The button has a SHIFT function, so a long press selects it (manual p.44).
    /// ON is excluded: its long press is SOLO, not a shifted function.
    pub fn has_shift_fn(self) -> bool {
        self != Self::On && self.alias().is_some()
    }
}

impl SynthUi {
    /// The button's primary LED / state is lit.
    pub fn primary_lit(&self, b: Btn) -> bool {
        let l = self.layer();
        match b {
            Btn::On => self.section_on,
            Btn::Mode => l.sample_mode,
            Btn::VoiceMode => l.voice != 0,
            Btn::Unison => l.unison != 0,
            Btn::VibratoSel => l.vib_mode != 0,
            Btn::LfoDest => l.lfo_dest != 3,
            Btn::ArpRun => l.arp_run,
            Btn::ArpMode => l.arp_kind != 0,
            Btn::KbHold => self.kb_hold,
            _ => false,
        }
    }

    /// The button's SHIFT / hold function is on (its legend lights up).
    pub fn alias_lit(&self, b: Btn) -> bool {
        let l = self.layer();
        match b {
            Btn::On => self.solo,
            Btn::Mode => l.extern_on,
            Btn::PitchSmp => l.env_to_pitch,
            Btn::OscEnvelope => l.osc_vel,
            Btn::FilterType => self.group_filter.is_active(),
            Btn::FilterEnvelope => l.filter_vel,
            Btn::AmpEnvelope => l.amp_vel > 0,
            Btn::LfoWaveform => self.group_lfo.is_active(),
            Btn::VoiceMode => l.priority != 0,
            Btn::ArpRun => l.kb_sync,
            Btn::ArpMode => l.pattern_on,
            Btn::ArpMenu => self.group_arp.is_active(),
            Btn::KbHold => l.kb_hold_exclude,
            Btn::OctaveDown | Btn::OctaveUp => l.kb_zone > 0,
            _ => false,
        }
    }

    /// Short text of the selector's current position, drawn on the button.
    pub fn status_text(&self, b: Btn) -> String {
        let l = self.layer();
        match b {
            Btn::Mode => if l.sample_mode { "SMP" } else { "ANA" }.to_string(),
            Btn::VoiceMode => ["POLY", "MONO", "LEG"][usize::from(l.voice) % 3].to_string(),
            Btn::Unison => ["OFF", "1", "2", "3"][usize::from(l.unison) % 4].to_string(),
            Btn::VibratoSel => super::tables::VIBRATO_MODES[usize::from(l.vib_mode) % 6]
                .0
                .to_string(),
            Btn::LfoDest => super::tables::LFO_DESTS[usize::from(l.lfo_dest) % 4]
                .0
                .to_string(),
            Btn::ArpMode => super::tables::ARP_KINDS[usize::from(l.arp_kind) % 3].to_string(),
            Btn::OctaveDown | Btn::OctaveUp => format!("{:+}", l.octave),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_has_a_unique_inventory_id_and_shift_ids_exist() {
        let all = nord_panel::all_controls();
        let mut seen = std::collections::HashSet::new();
        for b in Btn::ALL {
            assert!(all.iter().any(|c| c.id == b.id()), "{}", b.id());
            assert!(seen.insert(b.id()));
            if let Some((id, _)) = b.alias() {
                assert!(all.iter().any(|c| c.id == id), "{id}");
            }
        }
    }

    #[test]
    fn readouts_follow_the_layer_state() {
        let mut s = SynthUi::new();
        assert!(s.primary_lit(Btn::On));
        assert!(!s.primary_lit(Btn::ArpRun));
        s.layer_mut().arp_run = true;
        s.layer_mut().voice = 2;
        s.layer_mut().octave = -2;
        assert!(s.primary_lit(Btn::ArpRun));
        assert_eq!(s.status_text(Btn::VoiceMode), "LEG");
        assert_eq!(s.status_text(Btn::OctaveDown), "-2");
        assert_eq!(s.status_text(Btn::Mode), "ANA");
        assert_eq!(s.status_text(Btn::Waveform), "");
        assert!(!s.alias_lit(Btn::FilterType));
        s.group_filter.double_tap();
        assert!(s.alias_lit(Btn::FilterType));
        s.layer_mut().amp_vel = 0;
        assert!(!s.alias_lit(Btn::AmpEnvelope));
    }
}
