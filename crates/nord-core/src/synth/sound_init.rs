//! SOUND INIT (Shift+Waveform, manual p.37): every synth parameter returns to its
//! `SynthLayer::new` default except the Synth Mode and the selected waveform / sample.

use super::*;

impl SynthLayer {
    /// Resets all parameters except the oscillator mode, waveform, wavetable and loaded sample.
    /// Sounding notes are dropped. Control thread only (rebuilds the layer).
    ///
    /// ASSUMED: "sample preset values" (timbre, envelope presets) are plain layer fields and so are
    /// reset with the rest; the manual (p.37) says they return to their defaults.
    pub fn sound_init(&mut self) {
        let keep = (
            self.osc_mode,
            self.waveform,
            self.wavetable_index,
            self.sample_sound.take(),
            self.sample_rate,
        );
        *self = Self::new(keep.4);
        self.osc_mode = keep.0;
        self.waveform = keep.1;
        self.wavetable_index = keep.2;
        self.sample_sound = keep.3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_mode_and_waveform_resets_the_rest() {
        let mut s = SynthLayer::new(48_000.0);
        s.osc_mode = SynthOscMode::Fm;
        s.waveform = VaWaveform::Triangle;
        s.cutoff_hz = 321.0;
        s.arp_mode = ArpMode::Up;
        s.perf.zigzag = true;
        s.set_step_gate(1, 8);
        s.note_on(60, 100);
        s.sound_init();
        let fresh = SynthLayer::new(48_000.0);
        assert_eq!(s.osc_mode, SynthOscMode::Fm);
        assert_eq!(s.waveform, VaWaveform::Triangle);
        assert_eq!(s.cutoff_hz, fresh.cutoff_hz);
        assert_eq!(s.arp_mode, ArpMode::Off);
        assert!(!s.perf.zigzag);
        assert_eq!(s.perf.custom, fresh.perf.custom);
        assert_eq!(s.render_frame(), [0.0, 0.0]);
    }
}
