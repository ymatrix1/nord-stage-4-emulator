//! Positions and pick-up trackers of the Synth pots (`nord_panel::behaviour::PotTracker`).
//!
//! A pot is a physical object: one position, shared by the three layers. After the focus moves to
//! another layer the stored value differs from the pot position, so the pot ignores input until
//! it crosses or comes near the stored value ([`nord_panel::behaviour::SNAP_TOLERANCE`]).

use super::knobs::Knob;
use super::layer::SynthLayerUi;
use nord_panel::behaviour::PotTracker;

/// Lowest and highest cutoff of the FREQ pot in Hz.
/// ASSUMED: log scale 40 Hz .. 15 kHz (the previous panel's range); the hardware law is unknown.
pub const FREQ_MIN: f32 = 40.0;
pub const FREQ_MAX: f32 = 15_000.0;
/// ASSUMED: LFO rate log scale 0.05 .. 40 Hz (the host parameter range in `synth_mod_params.rs`).
pub const LFO_MIN: f32 = 0.05;
pub const LFO_MAX: f32 = 40.0;
/// ASSUMED: glide seconds per octave `4 n^2` (the host range 0..4 with a skewed taper).
pub const GLIDE_MAX: f32 = 4.0;

/// Pot positions and pick-up trackers.
#[derive(Debug, Clone, PartialEq)]
pub struct Pots {
    pub(super) phys: [f32; 10],
    pub(super) tr: [PotTracker; 10],
}

impl Default for Pots {
    fn default() -> Self {
        Self {
            phys: [0.5; 10],
            tr: [PotTracker::new(0.5); 10],
        }
    }
}

impl Pots {
    /// Physical position `0..=1` of a pot.
    pub fn phys(&self, k: Knob) -> f32 {
        self.phys[k.idx()]
    }

    /// Value stored for the focused layer (`0..=1`).
    pub fn stored(&self, k: Knob) -> f32 {
        self.tr[k.idx()].stored()
    }

    /// The pot has picked up the stored value (edits reach the layer).
    pub fn is_engaged(&self, k: Knob) -> bool {
        self.tr[k.idx()].is_engaged()
    }

    /// Put every pot at the stored value of `l` and engage it (program load, Sound Init).
    pub fn reset(&mut self, l: &SynthLayerUi) {
        for k in Knob::ALL {
            let n = norm_of(l, k);
            self.phys[k.idx()] = n;
            self.tr[k.idx()] = PotTracker::new(n);
        }
    }

    /// The focus moved to layer `l`: the pots stay where they are, the stored values change.
    pub fn layer_changed(&mut self, l: &SynthLayerUi) {
        for k in Knob::ALL {
            self.tr[k.idx()].stored_changed(norm_of(l, k));
        }
    }
}

/// Position `0..=1` of knob `k` for layer values `l`.
pub(super) fn norm_of(l: &SynthLayerUi, k: Knob) -> f32 {
    let n = match k {
        Knob::OscCtrl => l.osc_ctrl,
        Knob::OscEnvAmt => (l.osc_env_amt + 1.0) * 0.5,
        Knob::FilterFreq => (l.cutoff_hz / FREQ_MIN).max(1.0).ln() / (FREQ_MAX / FREQ_MIN).ln(),
        Knob::FilterRes => l.resonance,
        Knob::FilterEnvAmt => (l.filter_env_amt + 1.0) * 0.5,
        Knob::LfoRate if l.lfo_sync => f32::from(l.lfo_div) / 9.0,
        Knob::LfoRate => (l.lfo_rate_hz / LFO_MIN).max(1.0).ln() / (LFO_MAX / LFO_MIN).ln(),
        Knob::LfoModAmt => l.lfo_amt,
        Knob::Glide => (l.glide / GLIDE_MAX).max(0.0).sqrt(),
        Knob::ArpRate if l.arp_sync => f32::from(l.arp_div) / 9.0,
        Knob::ArpRate => (l.arp_rate - 20.0) / 280.0,
        Knob::ArpRange if l.arp_kind == 2 => l.gate_env,
        Knob::ArpRange => (l.arp_range - 1.0) / 3.0,
    };
    n.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_engages_every_pot_and_layer_change_disengages_it() {
        let mut p = Pots::default();
        let a = SynthLayerUi::default();
        p.reset(&a);
        assert!(Knob::ALL.iter().all(|k| p.is_engaged(*k)));
        let b = SynthLayerUi {
            resonance: 0.9,
            ..SynthLayerUi::default()
        };
        p.layer_changed(&b);
        assert!(!p.is_engaged(Knob::FilterRes));
        assert_eq!(p.stored(Knob::FilterRes), 0.9);
        assert_eq!(p.phys(Knob::FilterRes), 0.3);
    }

    #[test]
    fn norm_covers_the_clock_synced_rates_and_gate_range() {
        let l = SynthLayerUi {
            lfo_sync: true,
            lfo_div: 9,
            arp_sync: true,
            arp_div: 0,
            arp_kind: 2,
            gate_env: 0.25,
            ..SynthLayerUi::default()
        };
        assert_eq!(norm_of(&l, Knob::LfoRate), 1.0);
        assert_eq!(norm_of(&l, Knob::ArpRate), 0.0);
        assert_eq!(norm_of(&l, Knob::ArpRange), 0.25);
        let r = SynthLayerUi {
            arp_range: 2.0,
            ..SynthLayerUi::default()
        };
        assert!((norm_of(&r, Knob::ArpRange) - 1.0 / 3.0).abs() < 1e-6);
    }
}
