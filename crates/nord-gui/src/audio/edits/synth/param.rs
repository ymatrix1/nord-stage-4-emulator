//! One panel control of the Synth area as an engine write (`SynthParam::apply`).
//!
//! Every variant maps to exactly one `SynthLayer` field (or a small fixed set of fields), so the
//! DSP thread applies it without allocating. The panel (`panel/ui_state/synth`) decides which
//! layer(s) receive it; see [`super::SynthEdit`].

use nord_core::synth::{
    ClockDivision, FilterModel, SampleTimbre, SynthLayer, SynthOscMode, VaWaveform,
};
use nord_core::synth_types::{ArpMode, LfoDestination, LfoShape, NotePriority, VibratoMode};

/// Highest cutoff the filter accepts, as a fraction of the sample rate (`dsp-and-realtime.md`).
const MAX_CUTOFF_FRACTION: f32 = 0.49;
/// Lowest cutoff the filter accepts in Hz (`dsp-and-realtime.md`).
const MIN_CUTOFF_HZ: f32 = 10.0;
/// ASSUMED: OSC CTRL on a Pulse waveform sweeps the duty cycle from 50 % (0) to 5 % (1), the
/// "square to a very narrow pulse" of manual p.30; the engine clamps at 5 %.
const PULSE_WIDTH_MIN: f32 = 0.05;

/// A Synth panel control value, ready to write into a `SynthLayer`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynthParam {
    OscMode(SynthOscMode),
    Waveform(VaWaveform),
    /// OSC CTRL `0..=1`; the engine field depends on the oscillator mode and waveform.
    OscCtrl(f32),
    /// Fixed pulse duty `0.05..=0.95` (Pulse 33 / Pulse 10 waveforms).
    PulseWidth(f32),
    WavetableIndex(u8),
    FmRatio(f32),
    FmSubAmount(f32),
    SampleTimbre(SampleTimbre),
    /// Pitch plus fine tune in cents (`-2450..=2450`).
    PitchCents(f32),
    OscEnvAttack(f32),
    OscEnvDecay(f32),
    OscEnvRelease(f32),
    OscEnvAmount(f32),
    OscEnvToPitch(bool),
    OscEnvVelocity(bool),
    FilterModel(FilterModel),
    KbTrack(u8),
    FilterDrive(f32),
    Cutoff(f32),
    Resonance(f32),
    FilterEnvAmount(f32),
    FilterEnvAttack(f32),
    FilterEnvDecay(f32),
    FilterEnvRelease(f32),
    FilterEnvVelocity(f32),
    AmpAttack(f32),
    AmpDecay(f32),
    AmpRelease(f32),
    AmpVelocity(f32),
    LfoShape(LfoShape),
    LfoDest(LfoDestination),
    LfoRate(f32),
    LfoAmount(f32),
    LfoClockSync(bool),
    LfoDivision(u8),
    /// 0 Poly, 1 Mono, 2 Legato.
    Voice(u8),
    Priority(NotePriority),
    Glide(f32),
    Unison(u8),
    VibratoMode(VibratoMode),
    VibratoRate(f32),
    VibratoDepth(f32),
    VibratoDelay(f32),
    ArpMode(ArpMode),
    ArpRange(u8),
    ArpRate(f32),
    GateEnv(f32),
    PatternOn(bool),
    PatternPreset(u8),
    KbSync(bool),
    ArpClockSync(bool),
    ArpDivision(u8),
    KbHold(bool),
    KbHoldExclude(bool),
    Octave(i8),
}

impl SynthParam {
    /// Write the value into `l` (DSP thread; no allocation).
    pub fn apply(self, l: &mut SynthLayer) {
        use SynthParam as P;
        match self {
            P::OscMode(m) => l.osc_mode = m,
            P::Waveform(w) => l.waveform = w,
            P::OscCtrl(v) => apply_osc_ctrl(l, v.clamp(0.0, 1.0)),
            P::PulseWidth(v) => l.pulse_width = v.clamp(PULSE_WIDTH_MIN, 0.95),
            P::WavetableIndex(i) => l.wavetable_index = i.min(63),
            P::FmRatio(r) => l.fm_ratio = r.clamp(0.25, 16.0),
            P::FmSubAmount(a) => l.fm_sub_amount = a.clamp(0.0, 1.0),
            P::SampleTimbre(t) => l.sample_timbre = t,
            P::PitchCents(c) => l.pitch_cents = c.clamp(-2450.0, 2450.0),
            P::OscEnvAttack(s) => l.osc_env_attack_sec = secs(s),
            P::OscEnvDecay(s) => l.osc_env_decay_sec = secs(s),
            P::OscEnvRelease(s) => l.osc_env_release_sec = secs(s),
            P::OscEnvAmount(a) => l.osc_env_amount = a.clamp(-1.0, 1.0),
            P::OscEnvToPitch(b) => l.osc_env_to_pitch = b,
            P::OscEnvVelocity(b) => l.osc_env_velocity = b,
            P::FilterModel(m) => l.filter_model = m,
            P::KbTrack(k) => l.kb_track = k.min(3),
            P::FilterDrive(d) => l.filter_drive = d.clamp(0.0, 1.0),
            P::Cutoff(hz) => {
                l.cutoff_hz = hz.clamp(MIN_CUTOFF_HZ, MAX_CUTOFF_FRACTION * l.sample_rate)
            }
            P::Resonance(r) => l.resonance = r.clamp(0.0, 1.0),
            P::FilterEnvAmount(a) => l.filter_env_amount = a.clamp(-1.0, 1.0),
            P::FilterEnvAttack(s) => {
                l.filter_env_dedicated = true;
                l.filter_env_attack_sec = secs(s);
            }
            P::FilterEnvDecay(s) => {
                l.filter_env_dedicated = true;
                l.filter_env_decay_sec = secs(s);
            }
            P::FilterEnvRelease(s) => {
                l.filter_env_dedicated = true;
                l.filter_env_release_sec = secs(s);
            }
            P::FilterEnvVelocity(v) => l.filter_env_vel_amount = v.clamp(0.0, 1.0),
            P::AmpAttack(s) => l.attack_sec = secs(s),
            P::AmpDecay(s) => l.decay_sec = secs(s),
            P::AmpRelease(s) => l.release_sec = secs(s),
            P::AmpVelocity(v) => l.amp_velocity = v.clamp(0.0, 1.0),
            P::LfoShape(s) => l.lfo_shape = s,
            P::LfoDest(d) => l.lfo_dest = d,
            P::LfoRate(hz) => l.lfo_rate_hz = hz.clamp(0.05, 40.0),
            P::LfoAmount(a) => l.lfo_amount = a.clamp(0.0, 1.0),
            P::LfoClockSync(b) => l.perf.lfo_clock_sync = b,
            P::LfoDivision(i) => l.perf.lfo_division = ClockDivision::from_index(i32::from(i)),
            P::Voice(v) => {
                l.mono = v >= 1;
                l.legato = v >= 2;
            }
            P::Priority(p) => l.note_priority = p,
            P::Glide(s) => l.glide_sec = s.clamp(0.0, 4.0),
            P::Unison(u) => l.unison = u.min(3),
            P::VibratoMode(m) => l.vibrato_mode = m,
            P::VibratoRate(hz) => l.vibrato_rate_hz = hz.clamp(2.0, 8.0),
            P::VibratoDepth(c) => l.vibrato_depth_cents = c.clamp(0.0, 100.0),
            P::VibratoDelay(s) => l.vibrato_delay_sec = s.clamp(0.0, 2.0),
            P::ArpMode(m) => l.arp_mode = m,
            P::ArpRange(o) => l.arp_octave_range = o.clamp(1, 4),
            P::ArpRate(bpm) => l.arp_rate_bpm = bpm.clamp(20.0, 300.0),
            P::GateEnv(g) => l.perf.gate_env = g.clamp(0.0, 1.0),
            P::PatternOn(b) => l.perf.pattern_on = b,
            P::PatternPreset(p) => l.perf.pattern_preset = p.min(7),
            P::KbSync(b) => l.perf.kb_sync = b,
            P::ArpClockSync(b) => l.perf.arp_clock_sync = b,
            P::ArpDivision(i) => l.perf.arp_division = ClockDivision::from_index(i32::from(i)),
            P::KbHold(b) => l.perf.kb_hold = b,
            P::KbHoldExclude(b) => l.perf.kb_hold_exclude = b,
            P::Octave(o) => l.octave_offset = o.clamp(-3, 3),
        }
    }
}

/// Envelope time guard: finite and within 0.5 ms .. 60 s.
fn secs(s: f32) -> f32 {
    if s.is_finite() {
        s.clamp(0.0005, 60.0)
    } else {
        0.3
    }
}

/// OSC CTRL (manual p.28-31): Multi/Super detune, Pulse width, Wavetable position, FM amount.
///
/// ASSUMED: the panel's per-category behaviour (Shape, Sub, Bell) is not in the engine; only the
/// four engine fields above are driven. Pure/Wave waveforms ignore the knob as in the manual.
fn apply_osc_ctrl(l: &mut SynthLayer, v: f32) {
    match l.osc_mode {
        SynthOscMode::Analog => match l.waveform {
            VaWaveform::SuperSaw => l.supersaw_detune = v,
            VaWaveform::Pulse => l.pulse_width = 0.5 - (0.5 - PULSE_WIDTH_MIN) * v,
            _ => {}
        },
        SynthOscMode::Wavetable => l.wavetable_position = v,
        SynthOscMode::Fm => l.fm_amount = v,
        SynthOscMode::Sample => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> SynthLayer {
        SynthLayer::new(48_000.0)
    }

    #[test]
    fn osc_params_reach_their_fields() {
        let mut l = fresh();
        SynthParam::OscMode(SynthOscMode::Fm).apply(&mut l);
        SynthParam::Waveform(VaWaveform::Triangle).apply(&mut l);
        SynthParam::WavetableIndex(200).apply(&mut l);
        SynthParam::FmRatio(99.0).apply(&mut l);
        SynthParam::FmSubAmount(0.4).apply(&mut l);
        SynthParam::SampleTimbre(SampleTimbre::Bright).apply(&mut l);
        SynthParam::PitchCents(-5000.0).apply(&mut l);
        SynthParam::PulseWidth(0.33).apply(&mut l);
        assert_eq!(l.osc_mode, SynthOscMode::Fm);
        assert_eq!(l.waveform, VaWaveform::Triangle);
        assert_eq!(l.wavetable_index, 63);
        assert_eq!(l.fm_ratio, 16.0);
        assert_eq!(l.fm_sub_amount, 0.4);
        assert_eq!(l.sample_timbre, SampleTimbre::Bright);
        assert_eq!(l.pitch_cents, -2450.0);
        assert_eq!(l.pulse_width, 0.33);
    }

    #[test]
    fn osc_ctrl_follows_the_mode_and_waveform() {
        let mut l = fresh();
        SynthParam::Waveform(VaWaveform::SuperSaw).apply(&mut l);
        SynthParam::OscCtrl(0.8).apply(&mut l);
        assert_eq!(l.supersaw_detune, 0.8);
        SynthParam::Waveform(VaWaveform::Pulse).apply(&mut l);
        SynthParam::OscCtrl(1.0).apply(&mut l);
        assert!((l.pulse_width - 0.05).abs() < 1e-6);
        SynthParam::OscMode(SynthOscMode::Wavetable).apply(&mut l);
        SynthParam::OscCtrl(0.25).apply(&mut l);
        assert_eq!(l.wavetable_position, 0.25);
        SynthParam::OscMode(SynthOscMode::Fm).apply(&mut l);
        SynthParam::OscCtrl(0.6).apply(&mut l);
        assert_eq!(l.fm_amount, 0.6);
        let before = l.fm_amount;
        SynthParam::OscMode(SynthOscMode::Sample).apply(&mut l);
        SynthParam::OscCtrl(0.1).apply(&mut l);
        assert_eq!(l.fm_amount, before);
    }

    #[test]
    fn envelope_params_reach_their_fields_and_guard_values() {
        let mut l = fresh();
        SynthParam::OscEnvAttack(0.1).apply(&mut l);
        SynthParam::OscEnvDecay(0.2).apply(&mut l);
        SynthParam::OscEnvRelease(0.3).apply(&mut l);
        SynthParam::OscEnvAmount(-3.0).apply(&mut l);
        SynthParam::OscEnvToPitch(true).apply(&mut l);
        SynthParam::OscEnvVelocity(true).apply(&mut l);
        assert_eq!(
            (
                l.osc_env_attack_sec,
                l.osc_env_decay_sec,
                l.osc_env_release_sec
            ),
            (0.1, 0.2, 0.3)
        );
        assert_eq!(l.osc_env_amount, -1.0);
        assert!(l.osc_env_to_pitch && l.osc_env_velocity);
        SynthParam::AmpAttack(f32::NAN).apply(&mut l);
        SynthParam::AmpDecay(1000.0).apply(&mut l);
        SynthParam::AmpRelease(0.0).apply(&mut l);
        SynthParam::AmpVelocity(2.0).apply(&mut l);
        assert_eq!(l.attack_sec, 0.3);
        assert_eq!(l.decay_sec, 60.0);
        assert_eq!(l.release_sec, 0.0005);
        assert_eq!(l.amp_velocity, 1.0);
    }

    #[test]
    fn filter_params_reach_their_fields() {
        let mut l = fresh();
        SynthParam::FilterModel(FilterModel::HighPass).apply(&mut l);
        SynthParam::KbTrack(9).apply(&mut l);
        SynthParam::FilterDrive(0.67).apply(&mut l);
        SynthParam::Resonance(0.9).apply(&mut l);
        SynthParam::FilterEnvAmount(0.5).apply(&mut l);
        SynthParam::FilterEnvVelocity(1.0).apply(&mut l);
        assert_eq!(l.filter_model, FilterModel::HighPass);
        assert_eq!(l.kb_track, 3);
        assert_eq!(l.filter_drive, 0.67);
        assert_eq!(l.resonance, 0.9);
        assert_eq!(l.filter_env_amount, 0.5);
        assert_eq!(l.filter_env_vel_amount, 1.0);
        SynthParam::Cutoff(1.0e9).apply(&mut l);
        assert_eq!(l.cutoff_hz, 0.49 * 48_000.0);
        SynthParam::Cutoff(-5.0).apply(&mut l);
        assert_eq!(l.cutoff_hz, 10.0);
    }

    #[test]
    fn filter_envelope_edit_makes_the_envelope_dedicated() {
        for p in [
            SynthParam::FilterEnvAttack(0.2),
            SynthParam::FilterEnvDecay(0.2),
            SynthParam::FilterEnvRelease(0.2),
        ] {
            let mut l = fresh();
            assert!(!l.filter_env_dedicated);
            p.apply(&mut l);
            assert!(l.filter_env_dedicated);
        }
        let mut l = fresh();
        SynthParam::FilterEnvAttack(0.4).apply(&mut l);
        SynthParam::FilterEnvDecay(0.5).apply(&mut l);
        SynthParam::FilterEnvRelease(0.6).apply(&mut l);
        assert_eq!(
            (
                l.filter_env_attack_sec,
                l.filter_env_decay_sec,
                l.filter_env_release_sec
            ),
            (0.4, 0.5, 0.6)
        );
    }

    #[test]
    fn lfo_and_voice_params_reach_their_fields() {
        let mut l = fresh();
        SynthParam::LfoShape(LfoShape::Square).apply(&mut l);
        SynthParam::LfoDest(LfoDestination::OscPitch).apply(&mut l);
        SynthParam::LfoRate(500.0).apply(&mut l);
        SynthParam::LfoAmount(0.4).apply(&mut l);
        SynthParam::LfoClockSync(true).apply(&mut l);
        SynthParam::LfoDivision(6).apply(&mut l);
        assert_eq!(l.lfo_shape, LfoShape::Square);
        assert_eq!(l.lfo_dest, LfoDestination::OscPitch);
        assert_eq!(l.lfo_rate_hz, 40.0);
        assert_eq!(l.lfo_amount, 0.4);
        assert!(l.perf.lfo_clock_sync);
        assert_eq!(l.perf.lfo_division, ClockDivision::Sixteenth);
        SynthParam::Voice(1).apply(&mut l);
        assert!(l.mono && !l.legato);
        SynthParam::Voice(2).apply(&mut l);
        assert!(l.mono && l.legato);
        SynthParam::Voice(0).apply(&mut l);
        assert!(!l.mono && !l.legato);
        SynthParam::Priority(NotePriority::Hi).apply(&mut l);
        SynthParam::Glide(9.0).apply(&mut l);
        SynthParam::Unison(7).apply(&mut l);
        assert_eq!(l.note_priority, NotePriority::Hi);
        assert_eq!(l.glide_sec, 4.0);
        assert_eq!(l.unison, 3);
    }

    #[test]
    fn vibrato_and_arp_params_reach_their_fields() {
        let mut l = fresh();
        SynthParam::VibratoMode(VibratoMode::Delay).apply(&mut l);
        SynthParam::VibratoRate(20.0).apply(&mut l);
        SynthParam::VibratoDepth(30.0).apply(&mut l);
        SynthParam::VibratoDelay(0.7).apply(&mut l);
        assert_eq!(l.vibrato_mode, VibratoMode::Delay);
        assert_eq!(l.vibrato_rate_hz, 8.0);
        assert_eq!(l.vibrato_depth_cents, 30.0);
        assert_eq!(l.vibrato_delay_sec, 0.7);
        SynthParam::ArpMode(ArpMode::Gate).apply(&mut l);
        SynthParam::ArpRange(0).apply(&mut l);
        SynthParam::ArpRate(1000.0).apply(&mut l);
        SynthParam::GateEnv(0.75).apply(&mut l);
        SynthParam::PatternOn(true).apply(&mut l);
        SynthParam::PatternPreset(99).apply(&mut l);
        assert_eq!(l.arp_mode, ArpMode::Gate);
        assert_eq!(l.arp_octave_range, 1);
        assert_eq!(l.arp_rate_bpm, 300.0);
        assert_eq!(l.perf.gate_env, 0.75);
        assert!(l.perf.pattern_on);
        assert_eq!(l.perf.pattern_preset, 7);
    }

    #[test]
    fn performance_params_reach_the_perf_state() {
        let mut l = fresh();
        SynthParam::KbSync(true).apply(&mut l);
        SynthParam::ArpClockSync(true).apply(&mut l);
        SynthParam::ArpDivision(2).apply(&mut l);
        SynthParam::KbHold(true).apply(&mut l);
        SynthParam::KbHoldExclude(true).apply(&mut l);
        SynthParam::Octave(-9).apply(&mut l);
        assert!(l.perf.kb_sync && l.perf.arp_clock_sync);
        assert_eq!(l.perf.arp_division, ClockDivision::Quarter);
        assert!(l.perf.kb_hold && l.perf.kb_hold_exclude);
        assert_eq!(l.octave_offset, -3);
    }
}
