//! Engine writes of a layer's panel values: `emit_osc` (mode, waveform, OSC CTRL) and `emit_all`
//! (Sound Init, tests). Defaults equal `SynthLayer::new`, see the first test.

use super::layer::SynthLayerUi;
use super::tables::{self, FILTER_TYPES, VIBRATO_DELAYS};
use crate::audio::SynthParam;
use nord_core::synth::{SampleTimbre, SynthOscMode};

impl SynthLayerUi {
    /// Emit the oscillator writes of the current selection (mode, waveform, OSC CTRL, ...).
    pub fn emit_osc(&self, f: &mut impl FnMut(SynthParam)) {
        f(SynthParam::OscMode(self.osc_mode()));
        match self.osc_mode() {
            SynthOscMode::Analog => {
                if let Some(e) = self.analog_entry() {
                    f(SynthParam::Waveform(e.wave));
                    if let Some(pw) = e.pulse {
                        f(SynthParam::PulseWidth(pw));
                    }
                }
            }
            SynthOscMode::Fm => {
                let ratio = if self.wave_type == 1 {
                    tables::harmonic_partial(self.wave_idx)
                } else {
                    2.0_f32.powf(f32::from(tables::inharmonic_semitones(self.wave_idx)) / 12.0)
                };
                f(SynthParam::FmRatio(ratio));
                f(SynthParam::FmSubAmount(if self.fm_algo == 0 {
                    0.0
                } else {
                    0.4
                }));
            }
            SynthOscMode::Wavetable => f(SynthParam::WavetableIndex(self.table_index())),
            SynthOscMode::Sample => f(SynthParam::SampleTimbre(if self.sample_bright {
                SampleTimbre::Bright
            } else {
                SampleTimbre::Off
            })),
        }
        // A fixed Pulse waveform (Pulse 33 / 10) keeps its duty; OSC CTRL would overwrite it.
        if self.analog_entry().is_none_or(|e| e.pulse.is_none()) {
            f(SynthParam::OscCtrl(self.osc_ctrl));
        }
    }

    /// Emit every control value (Sound Init, tests). Oscillator mode and waveform are included.
    pub fn emit_all(&self, f: &mut impl FnMut(SynthParam)) {
        self.emit_osc(f);
        f(SynthParam::PitchCents(self.pitch_cents()));
        f(SynthParam::OscEnvAttack(self.osc_env.attack));
        f(SynthParam::OscEnvDecay(self.osc_env.decay));
        f(SynthParam::OscEnvRelease(self.osc_env.release));
        f(SynthParam::OscEnvAmount(self.osc_env_amt));
        f(SynthParam::OscEnvToPitch(self.env_to_pitch));
        f(SynthParam::OscEnvVelocity(self.osc_vel));
        if let Some(m) = FILTER_TYPES[usize::from(self.filter_type) % FILTER_TYPES.len()].model {
            f(SynthParam::FilterModel(m));
        }
        f(SynthParam::KbTrack(self.kb_track));
        f(SynthParam::FilterDrive(self.drive_value()));
        f(SynthParam::Cutoff(self.cutoff_hz));
        f(SynthParam::Resonance(self.resonance));
        f(SynthParam::FilterEnvAmount(self.filter_env_amt));
        f(SynthParam::FilterEnvVelocity(if self.filter_vel {
            1.0
        } else {
            0.0
        }));
        f(SynthParam::AmpAttack(self.amp_env.attack));
        f(SynthParam::AmpDecay(self.amp_env.decay));
        f(SynthParam::AmpRelease(self.amp_env.release));
        f(SynthParam::AmpVelocity(
            f32::from(self.amp_vel.min(3)) / 3.0,
        ));
        f(SynthParam::LfoShape(
            tables::LFO_SHAPES[usize::from(self.lfo_shape) % 5].1,
        ));
        f(SynthParam::LfoDest(
            tables::LFO_DESTS[usize::from(self.lfo_dest) % 4].1,
        ));
        f(SynthParam::LfoRate(self.lfo_rate_hz));
        f(SynthParam::LfoAmount(self.lfo_amt));
        f(SynthParam::LfoClockSync(self.lfo_sync));
        f(SynthParam::LfoDivision(self.lfo_div));
        f(SynthParam::Voice(self.voice));
        f(SynthParam::Priority(
            tables::PRIORITIES[usize::from(self.priority) % 3].1,
        ));
        f(SynthParam::Glide(self.glide));
        f(SynthParam::Unison(self.unison));
        f(SynthParam::VibratoMode(
            tables::VIBRATO_MODES[usize::from(self.vib_mode) % 6].1,
        ));
        f(SynthParam::VibratoRate(self.vib_rate));
        f(SynthParam::VibratoDepth(self.vib_amount * 10.0));
        f(SynthParam::VibratoDelay(
            VIBRATO_DELAYS[usize::from(self.vib_delay) % 4],
        ));
        f(SynthParam::ArpMode(self.arp_engine_mode()));
        f(SynthParam::ArpRange(self.arp_range.round().max(1.0) as u8));
        f(SynthParam::ArpRate(self.arp_rate));
        f(SynthParam::GateEnv(self.gate_env));
        f(SynthParam::PatternOn(self.pattern_on));
        f(SynthParam::PatternPreset(self.pattern_preset));
        f(SynthParam::KbSync(self.kb_sync));
        f(SynthParam::ArpClockSync(self.arp_sync));
        f(SynthParam::ArpDivision(self.arp_div));
        f(SynthParam::KbHoldExclude(self.kb_hold_exclude));
        f(SynthParam::Octave(self.octave));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::synth::SynthLayer;
    use nord_core::synth_types::ArpMode;

    fn engine_after(ui: &SynthLayerUi) -> SynthLayer {
        let mut l = SynthLayer::new(48_000.0);
        ui.emit_all(&mut |p| p.apply(&mut l));
        l
    }

    #[test]
    fn default_panel_values_equal_the_engine_defaults() {
        let fresh = SynthLayer::new(48_000.0);
        let l = engine_after(&SynthLayerUi::default());
        assert_eq!(l.osc_mode, fresh.osc_mode);
        assert_eq!(l.waveform, fresh.waveform);
        assert_eq!(l.cutoff_hz, fresh.cutoff_hz);
        assert_eq!(l.resonance, fresh.resonance);
        assert_eq!(l.attack_sec, fresh.attack_sec);
        assert_eq!(l.decay_sec, fresh.decay_sec);
        assert_eq!(l.release_sec, fresh.release_sec);
        assert_eq!(l.amp_velocity, fresh.amp_velocity);
        assert_eq!(l.lfo_rate_hz, fresh.lfo_rate_hz);
        assert_eq!(l.arp_mode, fresh.arp_mode);
        assert_eq!(l.arp_octave_range, fresh.arp_octave_range);
        assert_eq!(l.arp_rate_bpm, fresh.arp_rate_bpm);
        assert_eq!(l.unison, fresh.unison);
        assert_eq!(l.vibrato_rate_hz, fresh.vibrato_rate_hz);
        assert_eq!(l.perf.gate_env, fresh.perf.gate_env);
        assert_eq!(l.perf.arp_division, fresh.perf.arp_division);
        assert_eq!(l.perf.lfo_division, fresh.perf.lfo_division);
        assert_eq!(l.kb_track, fresh.kb_track);
    }

    #[test]
    fn emit_all_reaches_the_engine_for_changed_values() {
        let ui = SynthLayerUi {
            cutoff_hz: 900.0,
            voice: 2,
            lfo_sync: true,
            arp_run: true,
            arp_kind: 1,
            octave: -2,
            vib_amount: 5.0,
            ..SynthLayerUi::default()
        };
        let l = engine_after(&ui);
        assert_eq!(l.cutoff_hz, 900.0);
        assert!(l.mono && l.legato);
        assert!(l.perf.lfo_clock_sync);
        assert_eq!(l.arp_mode, ArpMode::Poly);
        assert_eq!(l.octave_offset, -2);
        assert_eq!(l.vibrato_depth_cents, 50.0);
    }

    #[test]
    fn fixed_pulse_waveform_is_not_overwritten_by_osc_ctrl() {
        let u = SynthLayerUi {
            wave_idx: 4,
            ..SynthLayerUi::default()
        };
        let mut sent = Vec::new();
        u.emit_osc(&mut |p| sent.push(p));
        assert!(sent.contains(&SynthParam::PulseWidth(0.33)));
        assert!(!sent.iter().any(|p| matches!(p, SynthParam::OscCtrl(_))));
    }
}
