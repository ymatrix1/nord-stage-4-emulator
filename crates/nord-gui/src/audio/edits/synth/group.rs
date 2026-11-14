//! GROUP (manual p.33, p.35, p.36): "applies the current settings to all Synth Layers".
//!
//! Filter, LFO and Arpeggiator each have a GROUP function. [`SynthGroup::copy`] copies the
//! settings of one source layer to the other two; later edits stay in sync because the panel then
//! sends them with [`super::SynthTarget::All`].

use nord_core::synth::SynthLayer;

/// Which settings a GROUP copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthGroup {
    /// Filter type, KBD TRK, Drive, Freq, Res, Env Amt, filter envelope and velocity.
    Filter,
    /// LFO waveform, destination, rate, amount and Master Clock sync.
    Lfo,
    /// Arpeggiator / Gate mode, range, rate, pattern, sync and clock division.
    Arp,
}

impl SynthGroup {
    /// Copy this group's settings from `src` to `dst` (no allocation).
    pub fn copy(self, src: &SynthLayer, dst: &mut SynthLayer) {
        match self {
            Self::Filter => {
                dst.filter_model = src.filter_model;
                dst.kb_track = src.kb_track;
                dst.filter_drive = src.filter_drive;
                dst.cutoff_hz = src.cutoff_hz;
                dst.resonance = src.resonance;
                dst.filter_env_amount = src.filter_env_amount;
                dst.filter_env_dedicated = src.filter_env_dedicated;
                dst.filter_env_attack_sec = src.filter_env_attack_sec;
                dst.filter_env_decay_sec = src.filter_env_decay_sec;
                dst.filter_env_release_sec = src.filter_env_release_sec;
                dst.filter_env_vel_amount = src.filter_env_vel_amount;
            }
            Self::Lfo => {
                dst.lfo_shape = src.lfo_shape;
                dst.lfo_dest = src.lfo_dest;
                dst.lfo_rate_hz = src.lfo_rate_hz;
                dst.lfo_amount = src.lfo_amount;
                dst.perf.lfo_clock_sync = src.perf.lfo_clock_sync;
                dst.perf.lfo_division = src.perf.lfo_division;
            }
            Self::Arp => {
                dst.arp_mode = src.arp_mode;
                dst.arp_octave_range = src.arp_octave_range;
                dst.arp_rate_bpm = src.arp_rate_bpm;
                dst.perf.gate_env = src.perf.gate_env;
                dst.perf.pattern_on = src.perf.pattern_on;
                dst.perf.pattern_preset = src.perf.pattern_preset;
                dst.perf.kb_sync = src.perf.kb_sync;
                dst.perf.arp_clock_sync = src.perf.arp_clock_sync;
                dst.perf.arp_division = src.perf.arp_division;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth::FilterModel;
    use nord_core::synth_types::{ArpMode, LfoShape};

    #[test]
    fn filter_group_copies_filter_settings_only() {
        let mut a = SynthLayer::new(48_000.0);
        let mut b = SynthLayer::new(48_000.0);
        a.filter_model = FilterModel::HighPass;
        a.cutoff_hz = 777.0;
        a.filter_env_dedicated = true;
        a.lfo_amount = 0.9;
        SynthGroup::Filter.copy(&a, &mut b);
        assert_eq!(b.filter_model, FilterModel::HighPass);
        assert_eq!(b.cutoff_hz, 777.0);
        assert!(b.filter_env_dedicated);
        assert_eq!(b.lfo_amount, 0.0, "LFO is not part of the filter group");
    }

    #[test]
    fn lfo_group_copies_lfo_settings() {
        let mut a = SynthLayer::new(48_000.0);
        let mut b = SynthLayer::new(48_000.0);
        a.lfo_shape = LfoShape::Square;
        a.lfo_rate_hz = 9.0;
        a.perf.lfo_clock_sync = true;
        SynthGroup::Lfo.copy(&a, &mut b);
        assert_eq!(b.lfo_shape, LfoShape::Square);
        assert_eq!(b.lfo_rate_hz, 9.0);
        assert!(b.perf.lfo_clock_sync);
    }

    #[test]
    fn arp_group_copies_arp_settings() {
        let mut a = SynthLayer::new(48_000.0);
        let mut b = SynthLayer::new(48_000.0);
        a.arp_mode = ArpMode::Poly;
        a.arp_rate_bpm = 90.0;
        a.perf.pattern_on = true;
        a.perf.pattern_preset = 3;
        SynthGroup::Arp.copy(&a, &mut b);
        assert_eq!(b.arp_mode, ArpMode::Poly);
        assert_eq!(b.arp_rate_bpm, 90.0);
        assert!(b.perf.pattern_on);
        assert_eq!(b.perf.pattern_preset, 3);
        assert!(!b.perf.kb_hold);
    }
}
